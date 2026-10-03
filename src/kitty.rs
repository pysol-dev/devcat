//! Kitty wire backend. The PTY owner supplies every writer and every reply byte.
//! Run negotiation before starting the child; forward all returned bytes unchanged.
//! A positive query is protocol detection, not evidence of a tested terminal.
use crate::{animation::Pose, profile::Anchor};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    io::{self, Write},
    num::NonZeroU32,
    time::{Duration, Instant},
};

pub const PROBE_QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(200);
const RESPONSE_LIMIT: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    Pending,
    Supported,
    Unsupported,
}

/// Incrementally observes replies, retaining at most one bounded APC candidate.
/// Device attributes and all nonmatching replies remain in `passthrough`.
pub struct Probe {
    start: Instant,
    pending: Vec<u8>,
    status: ProbeStatus,
}
impl Probe {
    pub fn start(out: &mut impl Write, now: Instant) -> io::Result<Self> {
        out.write_all(PROBE_QUERY)?;
        out.flush()?;
        Ok(Self {
            start: now,
            pending: Vec::with_capacity(RESPONSE_LIMIT),
            status: ProbeStatus::Pending,
        })
    }
    pub const fn status(&self) -> &ProbeStatus {
        &self.status
    }
    pub fn feed(&mut self, bytes: &[u8], now: Instant, passthrough: &mut Vec<u8>) {
        self.expire(now, passthrough);
        for &byte in bytes {
            match self.status {
                ProbeStatus::Supported | ProbeStatus::Unsupported => passthrough.push(byte),
                ProbeStatus::Pending => {
                    self.pending.push(byte);
                    let prefix = b"\x1b_Gi=31;";
                    if self.pending.len() <= prefix.len() && prefix.starts_with(&self.pending) {
                        continue;
                    }
                    if self.pending.starts_with(prefix) && self.pending.ends_with(b"\x1b\\") {
                        self.status = if self.pending == b"\x1b_Gi=31;OK\x1b\\" {
                            ProbeStatus::Supported
                        } else {
                            ProbeStatus::Unsupported
                        };
                        self.pending.clear();
                    } else if !self.pending.starts_with(prefix)
                        || self.pending.len() >= RESPONSE_LIMIT
                    {
                        // Retain a trailing ESC, which could begin the next response.
                        let retain = usize::from(byte == 0x1b);
                        passthrough.extend(self.pending.drain(..self.pending.len() - retain));
                    }
                }
            }
        }
    }
    pub fn expire(&mut self, now: Instant, passthrough: &mut Vec<u8>) {
        if matches!(self.status, ProbeStatus::Pending)
            && now.saturating_duration_since(self.start) >= PROBE_TIMEOUT
        {
            self.status = ProbeStatus::Unsupported;
            passthrough.append(&mut self.pending);
        }
    }
    /// Releases any unfinished candidate when abandoning negotiation.
    pub fn finish(mut self, passthrough: &mut Vec<u8>) -> Option<Capability> {
        passthrough.append(&mut self.pending);
        match self.status {
            ProbeStatus::Supported => Some(Capability(())),
            ProbeStatus::Pending | ProbeStatus::Unsupported => None,
        }
    }
}

/// Proof that a matching positive probe reply was observed.
pub struct Capability(());

/// An image ID allocated by the terminal, never an arbitrary child image ID.
/// Start this handshake only after the probe and before the child starts.
pub struct Allocation {
    number: Option<NonZeroU32>,
    pending: Vec<u8>,
    start: Instant,
}
impl Allocation {
    pub fn start(_: Capability, number: NonZeroU32, out: &mut impl Write) -> io::Result<Self> {
        let allocation = Self {
            number: Some(number),
            pending: Vec::with_capacity(RESPONSE_LIMIT),
            start: Instant::now(),
        };
        if let Err(error) =
            upload(&format!("I={number}"), png(Pose::Idle), out).and_then(|()| out.flush())
        {
            allocation.cancel(out, &mut Vec::new())?;
            return Err(error);
        }
        Ok(allocation)
    }
    /// Returns a backend only for our matching successful allocation response.
    /// Call `cancel` on timeout (200ms), EOF, or a failed allocation.
    pub fn feed(&mut self, bytes: &[u8], now: Instant, passthrough: &mut Vec<u8>) -> Option<Kitty> {
        if self.expired(now) || self.number.is_none() {
            passthrough.append(&mut self.pending);
            passthrough.extend_from_slice(bytes);
            return None;
        }
        let mut allocated = None;
        for &byte in bytes {
            if self.number.is_none() {
                passthrough.push(byte);
                continue;
            }
            self.pending.push(byte);
            let prefix = b"\x1b_G";
            if self.pending.len() <= prefix.len() && prefix.starts_with(&self.pending) {
                continue;
            }
            if self.pending.starts_with(prefix) && self.pending.ends_with(b"\x1b\\") {
                let response = std::str::from_utf8(&self.pending[3..self.pending.len() - 2]).ok();
                let id =
                    response.and_then(|s| self.number.and_then(|number| allocation_id(s, number)));
                if let Some(id) = id {
                    allocated = Some(Kitty {
                        id,
                        owned: true,
                        placed: false,
                        pose: Some(Pose::Idle),
                    });
                    self.number = None;
                    self.pending.clear();
                } else {
                    passthrough.append(&mut self.pending);
                }
            } else if !self.pending.starts_with(prefix) || self.pending.len() >= RESPONSE_LIMIT {
                let retain = usize::from(byte == 0x1b);
                passthrough.extend(self.pending.drain(..self.pending.len() - retain));
            }
        }
        allocated
    }
    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= PROBE_TIMEOUT
    }
    /// Abort a partial upload and remove our newest numbered allocation.
    /// Negotiation precedes child launch so child replies cannot collide.
    pub fn cancel(mut self, out: &mut impl Write, passthrough: &mut Vec<u8>) -> io::Result<()> {
        passthrough.append(&mut self.pending);
        if let Some(number) = self.number {
            write!(out, "\x1b\\\x1b_Ga=d,d=N,I={number},q=2\x1b\\")?;
        }
        Ok(())
    }
}
fn allocation_id(response: &str, number: NonZeroU32) -> Option<NonZeroU32> {
    let (header, "OK") = response.split_once(';')? else {
        return None;
    };
    let mut id = None;
    let mut matches_number = false;
    for field in header.split(',') {
        if let Some(value) = field.strip_prefix("i=") {
            id = value.parse().ok().and_then(NonZeroU32::new);
        }
        if let Some(value) = field.strip_prefix("I=") {
            matches_number = value.parse::<u32>().ok() == Some(number.get());
        }
    }
    if matches_number { id } else { None }
}

/// Geometry must already have been approved by the caller's screen/profile guard.
#[derive(Clone, Copy)]
pub struct Placement {
    pub anchor: Anchor,
    pub columns: u16,
    pub pose: Pose,
    pub cursor: (u16, u16),
}
pub struct Kitty {
    id: NonZeroU32,
    owned: bool,
    placed: bool,
    pose: Option<Pose>,
}
impl Kitty {
    /// Always re-place: scrolling, erase, and resize can invalidate placements.
    /// Call `hide` before forwarding output that could scroll, clear or reflow.
    pub fn draw(&mut self, frame: Placement, out: &mut impl Write) -> io::Result<()> {
        if !self.owned {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Kitty image ownership has ended",
            ));
        }
        if frame.columns == 0 || frame.anchor.height == 0 {
            return self.hide(out);
        }
        if self.pose != Some(frame.pose) {
            self.pose = None;
            upload(&format!("i={}", self.id), png(frame.pose), out)?;
            self.pose = Some(frame.pose);
        }
        self.placed = true;
        write!(
            out,
            "\x1b[{};{}H",
            u32::from(frame.anchor.row) + 1,
            u32::from(frame.anchor.col) + 1
        )?;
        let placed = write!(
            out,
            "\x1b_Ga=p,i={},p=1,C=1,z=-1,c={},r={},q=2\x1b\\",
            self.id, frame.columns, frame.anchor.height
        );
        let restored = write!(
            out,
            "\x1b[{};{}H",
            u32::from(frame.cursor.0) + 1,
            u32::from(frame.cursor.1) + 1
        );
        placed.and(restored)
    }
    /// Hide only our placement, retaining the allocated image ID for reuse.
    /// Call before forwarding output that can scroll, clear, or resize.
    pub fn hide(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.placed {
            write!(out, "\x1b\\\x1b_Ga=d,d=i,i={},p=1,q=2\x1b\\", self.id)?;
            self.placed = false;
        }
        Ok(())
    }
    /// Permanently release our image; drawing after this returns an error.
    /// Retryable after writer failure. The sole output owner must call on exit.
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.owned {
            // ST terminates any upload interrupted by a previous writer failure.
            write!(out, "\x1b\\\x1b_Ga=d,d=I,i={},q=2\x1b\\", self.id)?;
            self.owned = false;
            self.placed = false;
            self.pose = None;
        }
        Ok(())
    }
}
fn png(pose: Pose) -> &'static [u8] {
    match pose {
        Pose::Idle => include_bytes!("../assets/generated/idle.png"),
        Pose::Left => include_bytes!("../assets/generated/left.png"),
        Pose::Right => include_bytes!("../assets/generated/right.png"),
    }
}
fn upload(identity: &str, bytes: &[u8], out: &mut impl Write) -> io::Result<()> {
    let data = STANDARD.encode(bytes);
    let chunks = data.as_bytes().chunks(4096);
    let total = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        if index == 0 {
            write!(out, "\x1b_Ga=t,t=d,f=100,{identity},")?;
        } else {
            out.write_all(b"\x1b_G")?;
        }
        // Allocation must return its assigned ID; owned uploads suppress replies.
        let quiet = if identity.starts_with("I=") { 0 } else { 2 };
        write!(out, "q={quiet},m={};", u8::from(index + 1 < total))?;
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}
