//! Keep a child's synchronized terminal redraw in one parent-terminal write.
//! Input still goes straight to the PTY; only child output is briefly batched.
use std::time::{Duration, Instant};

const START: &[u8] = b"\x1b[?2026h";
const END: &[u8] = b"\x1b[?2026l";
const MAX_BYTES: usize = 256 * 1024;
const MAX_WAIT: Duration = Duration::from_millis(100);
const PREFIX_WAIT: Duration = Duration::from_millis(10);

#[derive(Default)]
pub struct SyncBatcher {
    pending: Vec<u8>,
    in_sync: bool,
    since: Option<Instant>,
    disabled: bool,
}

impl SyncBatcher {
    pub fn feed(&mut self, bytes: &[u8], now: Instant) -> Vec<Vec<u8>> {
        if self.disabled {
            return vec![bytes.to_vec()];
        }
        self.pending.extend_from_slice(bytes);
        let mut ready = Vec::new();
        loop {
            if self.in_sync {
                if let Some(index) = find(&self.pending, END) {
                    ready.push(self.pending.drain(..index + END.len()).collect());
                    self.in_sync = false;
                    self.since = None;
                    continue;
                }
                if self.pending.len() > MAX_BYTES
                    || self
                        .since
                        .is_some_and(|since| now.duration_since(since) >= MAX_WAIT)
                {
                    self.disabled = true;
                    self.in_sync = false;
                    self.since = None;
                    ready.push(std::mem::take(&mut self.pending));
                }
                break;
            }
            if let Some(index) = find(&self.pending, START) {
                if index > 0 {
                    ready.push(self.pending.drain(..index).collect());
                }
                self.in_sync = true;
                self.since = Some(now);
                continue;
            }
            let retained = longest_start_prefix(&self.pending);
            let emit = self.pending.len() - retained;
            if emit > 0 {
                ready.push(self.pending.drain(..emit).collect());
            }
            self.since = (retained > 0).then_some(now);
            break;
        }
        ready
    }

    pub fn expire(&mut self, now: Instant) -> Option<Vec<u8>> {
        let wait = if self.in_sync { MAX_WAIT } else { PREFIX_WAIT };
        if self.pending.is_empty()
            || self
                .since
                .is_none_or(|since| now.duration_since(since) < wait)
        {
            return None;
        }
        if self.in_sync {
            self.disabled = true;
        }
        self.in_sync = false;
        self.since = None;
        Some(std::mem::take(&mut self.pending))
    }

    pub const fn in_sync(&self) -> bool {
        self.in_sync
    }

    /// Flush exact child bytes on exit. Close a stranded synchronized update
    /// so the parent terminal does not remain frozen after the child is gone.
    pub fn finish(mut self) -> Option<Vec<u8>> {
        if self.pending.is_empty() {
            return None;
        }
        if self.in_sync {
            self.pending.extend_from_slice(END);
        }
        Some(self.pending)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn longest_start_prefix(bytes: &[u8]) -> usize {
    (1..START.len())
        .rev()
        .find(|&length| bytes.ends_with(&START[..length]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_is_one_packet_at_every_split_with_bytes_preserved() {
        let raw = b"prefix\x1b[?2026h\x1b[1;1H\x1b[Jnew composer\x1b[?2026lsuffix";
        for split in 0..=raw.len() {
            let now = Instant::now();
            let mut batcher = SyncBatcher::default();
            let mut packets = batcher.feed(&raw[..split], now);
            packets.extend(batcher.feed(&raw[split..], now));
            if let Some(tail) = batcher.finish() {
                packets.push(tail);
            }
            assert_eq!(packets.concat(), raw);
            assert_eq!(
                packets
                    .iter()
                    .filter(|part| part.starts_with(START))
                    .count(),
                1
            );
            assert!(
                packets
                    .iter()
                    .any(|part| part.starts_with(START) && part.ends_with(END))
            );
        }
    }

    #[test]
    fn stalled_redraw_is_bounded_and_passes_through() {
        let now = Instant::now();
        let mut batcher = SyncBatcher::default();
        assert!(batcher.feed(START, now).is_empty());
        assert!(batcher.in_sync());
        assert_eq!(batcher.expire(now + MAX_WAIT), Some(START.to_vec()));
        assert!(!batcher.in_sync());
        assert_eq!(
            batcher.feed(b"tail", now + MAX_WAIT),
            vec![b"tail".to_vec()]
        );
    }

    #[test]
    fn incomplete_escape_prefix_is_released_quickly() {
        let now = Instant::now();
        let mut batcher = SyncBatcher::default();
        assert!(batcher.feed(b"\x1b[?202", now).is_empty());
        assert_eq!(
            batcher.expire(now + PREFIX_WAIT),
            Some(b"\x1b[?202".to_vec())
        );
    }
}
