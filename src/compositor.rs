use crate::{
    animation::{Animation, Pose},
    input::InputClassifier,
    profile::{self, Anchor, Profile},
    screen::ProtocolGuard,
};
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub struct Compositor {
    parser: vt100::Parser,
    guard: ProtocolGuard,
    input: InputClassifier,
    animation: Animation,
    pending_hit: Option<Instant>,
    pending_cursor: Option<(u16, u16)>,
    profile: Profile,
    drawn: Option<Anchor>,
    drawn_pose: Option<Pose>,
    last_frame: Option<Instant>,
    awaiting_clear: bool,
    frames: Option<[Vec<String>; 3]>,
    kitty: Option<crate::kitty::Kitty>,
}
impl Compositor {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), 0),
            guard: ProtocolGuard::default(),
            input: InputClassifier::default(),
            animation: Animation::new(),
            pending_hit: None,
            pending_cursor: None,
            profile: Profile::Unconfirmed,
            drawn: None,
            drawn_pose: None,
            last_frame: None,
            awaiting_clear: true,
            frames: load_frames(),
            kitty: None,
        }
    }
    pub fn use_kitty(&mut self, kitty: crate::kitty::Kitty) {
        self.kitty = Some(kitty);
    }
    pub fn hide_graphics(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(kitty) = &mut self.kitty {
            kitty.hide(out)?;
        }
        Ok(())
    }
    pub fn confirm_profile(&mut self, profile: Profile) {
        self.profile = profile;
    }
    pub fn input(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        if self.composing() {
            self.pending_hit = None;
            self.pending_cursor = None;
            self.input.observe(bytes, || self.animation.strike(now));
        } else if self.profile == Profile::HermesTui
            && !self.awaiting_clear
            && self.guard.ready()
            && profile::hermes_empty_composer(self.parser.screen(), self.frame_rows())
        {
            // Hermes hides its cursor for an empty draft. Keep only a bounded
            // hit category until its echo confirms a safe composing layout.
            self.input.observe(bytes, || self.pending_hit = Some(now));
        } else if self.profile == Profile::CodexTui
            && !self.awaiting_clear
            && self.guard.pending_safe()
            && profile::codex_pending_composer(self.parser.screen(), self.frame_rows())
        {
            // A synchronized Codex redraw may briefly hide the cursor while
            // the composer remains structurally intact. Require its next echo
            // to move the cursor before committing this short-lived hit.
            let cursor = self.parser.screen().cursor_position();
            self.input.observe(bytes, || {
                self.pending_hit = Some(now);
                self.pending_cursor = Some(cursor);
            });
        } else {
            self.pending_hit = None;
            self.pending_cursor = None;
            self.input = InputClassifier::default();
        }
    }
    fn frame_rows(&self) -> u16 {
        self.frames
            .as_ref()
            .and_then(|frames| u16::try_from(frames[0].len()).ok())
            .unwrap_or(0)
    }
    pub fn composing(&self) -> bool {
        !self.awaiting_clear
            && self.guard.ready()
            && profile::anchor(self.parser.screen(), self.profile, self.frame_rows()).is_some()
    }
    pub fn output(&mut self, bytes: &[u8], out: &mut impl Write) -> io::Result<()> {
        if !non_cell_output(bytes) {
            self.erase(out)?;
        }
        out.write_all(bytes)?;
        let clear_generation = self.guard.clear_generation;
        self.guard.observe(bytes);
        if self.guard.clear_generation != clear_generation {
            self.awaiting_clear = false;
        }
        self.parser.process(bytes);
        if self.profile == Profile::CodexTui
            && self.pending_hit.is_some()
            && !profile::codex_pending_composer(self.parser.screen(), self.frame_rows())
        {
            self.pending_hit = None;
            self.pending_cursor = None;
        }
        Ok(())
    }
    /// Call finish before changing terminal geometry. The fresh model hides until the child redraws confirmed chrome.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.drawn = None;
        self.drawn_pose = None;
        self.last_frame = None;
        self.awaiting_clear = true;
        self.pending_hit = None;
        self.pending_cursor = None;
        self.input = InputClassifier::default();
        let alternate_screen = self.parser.screen().alternate_screen();
        let hide_cursor = self.parser.screen().hide_cursor();
        let attributes = self.parser.screen().attributes_formatted();
        self.parser = vt100::Parser::new(rows.max(1), cols.max(1), 0);
        // Resize changes cells, not terminal modes. Seed only modeled state;
        // these bytes never reach the physical terminal or satisfy awaiting_clear.
        if alternate_screen {
            self.parser.process(b"\x1b[?1049h");
        }
        if hide_cursor {
            self.parser.process(b"\x1b[?25l");
        }
        self.parser.process(&attributes);
    }
    pub fn tick(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.tick_at(out, Instant::now())
    }
    fn tick_at(&mut self, out: &mut impl Write, now: Instant) -> io::Result<()> {
        let render_ready =
            self.guard.ready() || (self.profile == Profile::CodexTui && self.guard.pending_safe());
        if self.awaiting_clear || !render_ready {
            return Ok(());
        }
        let Some(anchor) = profile::anchor(self.parser.screen(), self.profile, self.frame_rows())
        else {
            return Ok(());
        };
        let pending_cursor = self.pending_cursor.take();
        if let Some(hit) = self.pending_hit.take()
            && now.duration_since(hit) < Duration::from_millis(110)
            && (self.profile != Profile::CodexTui
                || pending_cursor.is_some_and(|old| old != self.parser.screen().cursor_position()))
        {
            self.animation.strike(hit);
        }
        let requested_pose = self.animation.pose(now);
        if self.drawn == Some(anchor) && self.drawn_pose == Some(requested_pose) {
            return Ok(());
        }
        let throttled = self
            .last_frame
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(34));
        if throttled && self.drawn.is_some() {
            return Ok(());
        }
        // Restoring cells erased for child output is not an animation advance.
        // Keep the previous pose during its frame budget instead of leaving a hole.
        let pose = if throttled {
            self.drawn_pose.unwrap_or(requested_pose)
        } else {
            requested_pose
        };
        let advance_frame = self.drawn_pose != Some(pose) || self.last_frame.is_none();
        self.erase(out)?;
        if let Some(kitty) = &mut self.kitty {
            let frame = crate::kitty::Placement {
                anchor,
                columns: 16,
                pose,
                cursor: self.parser.screen().cursor_position(),
            };
            if kitty.draw(frame, out).is_ok() {
                self.drawn = Some(anchor);
                self.drawn_pose = Some(pose);
                if advance_frame {
                    self.last_frame = Some(now);
                }
                draw_owned_bar(anchor, out)?;
                self.restore_cursor(out)?;
                return Ok(());
            }
            kitty.finish(out)?;
            self.kitty = None;
        }
        let index = match pose {
            Pose::Idle => 0,
            Pose::Left => 1,
            Pose::Right => 2,
        };
        let Some(frames) = &self.frames else {
            return Ok(());
        };
        out.write_all(b"\x1b[0m")?;
        // Record ownership before writing so a partial writer failure still permits cleanup.
        self.drawn = Some(anchor);
        self.drawn_pose = Some(pose);
        if advance_frame {
            self.last_frame = Some(now);
        }
        for (offset, line) in (0_u16..).zip(&frames[index]) {
            write!(
                out,
                "\x1b[{};{}H{}",
                anchor.row + offset + 1,
                anchor.col + 1,
                line
            )?;
        }
        draw_owned_bar(anchor, out)?;
        self.restore_cursor(out)
    }
    pub fn suspend(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.erase(out)
    }
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(kitty) = &mut self.kitty {
            kitty.finish(out)?;
            if let Some(a) = self.drawn {
                erase_owned_bar(a, self.parser.screen(), out)?;
                self.restore_cursor(out)?;
            }
            self.drawn = None;
            return Ok(());
        }
        self.erase(out)
    }
    fn erase(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(kitty) = &mut self.kitty {
            kitty.hide(out)?;
            if let Some(a) = self.drawn {
                erase_owned_bar(a, self.parser.screen(), out)?;
                self.restore_cursor(out)?;
            }
            self.drawn = None;
            return Ok(());
        }
        if let Some(a) = self.drawn {
            out.write_all(b"\x1b[0m")?;
            for row in a.row..a.row + a.height {
                for col in a.col..a.col + 16 {
                    write!(out, "\x1b[{};{}H", row + 1, col + 1)?;
                    if self
                        .parser
                        .screen()
                        .cell(row, col)
                        .is_some_and(|cell| cell.contents() == " ")
                    {
                        out.write_all(b" ")?;
                    } else {
                        out.write_all(b"\x1b[X")?;
                    }
                }
            }
            erase_owned_bar(a, self.parser.screen(), out)?;
            self.restore_cursor(out)?;
            self.drawn = None;
        }
        Ok(())
    }
    fn restore_cursor(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(&self.parser.screen().cursor_state_formatted())?;
        out.write_all(&self.parser.screen().attributes_formatted())
    }
}

fn non_cell_output(mut bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    while !bytes.is_empty() {
        if bytes.starts_with(b"\x1b[") {
            let Some(end) = bytes[2..]
                .iter()
                .position(|&byte| (0x40..=0x7e).contains(&byte))
            else {
                return false;
            };
            if !non_cell_csi(&bytes[2..end + 3]) {
                return false;
            }
            bytes = &bytes[end + 3..];
        } else if bytes.starts_with(b"\x1b]") {
            let terminator = (2..bytes.len()).find_map(|index| {
                (bytes[index] == 7).then_some((index, 1)).or_else(|| {
                    (bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\'))
                        .then_some((index, 2))
                })
            });
            let Some((end, width)) = terminator else {
                return false;
            };
            if end > 1026 || !crate::screen::supported_osc(&bytes[2..end]) {
                return false;
            }
            bytes = &bytes[end + width..];
        } else if bytes.starts_with(b"\x1b7") || bytes.starts_with(b"\x1b8") {
            bytes = &bytes[2..];
        } else {
            return false;
        }
    }
    true
}

fn non_cell_csi(sequence: &[u8]) -> bool {
    match sequence.split_last() {
        Some((&(b'H' | b'f' | b'G' | b'A' | b'B' | b'C' | b'D'), params)) => params
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b';'),
        Some((&b'm', _)) => {
            crate::screen::supported_sgr(sequence)
                || [b">4m".as_slice(), b">4;0m"].contains(&sequence)
        }
        _ => [
            b"?25h".as_slice(),
            b"?25l",
            b"?2026h",
            b"?2026l",
            b"6n",
            b"c",
            b"?u",
            b">7u",
            b"0 q",
        ]
        .contains(&sequence),
    }
}

fn draw_owned_bar(anchor: Anchor, out: &mut impl Write) -> io::Result<()> {
    if let Some(row) = anchor.bar_row {
        out.write_all(b"\x1b[0m")?;
        write!(
            out,
            "\x1b[{};{}H{}",
            row + 1,
            anchor.col + 1,
            "▔".repeat(16)
        )?;
    }
    Ok(())
}

fn erase_owned_bar(anchor: Anchor, screen: &vt100::Screen, out: &mut impl Write) -> io::Result<()> {
    if let Some(row) = anchor.bar_row {
        out.write_all(b"\x1b[0m")?;
        for col in anchor.col..anchor.col + 16 {
            write!(out, "\x1b[{};{}H", row + 1, col + 1)?;
            if screen
                .cell(row, col)
                .is_some_and(|cell| cell.contents() == " ")
            {
                out.write_all(b" ")?;
            } else {
                out.write_all(b"\x1b[X")?;
            }
        }
    }
    Ok(())
}
fn load_frames() -> Option<[Vec<String>; 3]> {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("../assets/generated/compact-cells.json")).ok()?;
    if doc["schema_version"] != 1 || doc["columns"] != 16 {
        return None;
    }
    let rows = usize::try_from(doc["rows"].as_u64()?).ok()?;
    if !(1..=10).contains(&rows) {
        return None;
    }
    let mut frames: [Vec<String>; 3] = std::array::from_fn(|_| Vec::new());
    for (i, key) in ["idle", "left", "right"].iter().enumerate() {
        for row in doc["frames"][key]["monochrome_rows"].as_array()? {
            let text = row.as_str()?;
            if text.chars().count() != 16
                || !text.chars().all(|c| matches!(c, ' ' | '▀' | '▄' | '█'))
            {
                return None;
            }
            frames[i].push(text.to_owned());
        }
        if frames[i].len() != rows {
            return None;
        }
    }
    Some(frames)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests assert infallible Vec writes and fixture cells"
)]
mod tests {
    use super::*;
    #[test]
    fn cells_continue_when_graphics_writer_fails_once() {
        struct FailOnce {
            failed: bool,
            bytes: Vec<u8>,
        }
        impl Write for FailOnce {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.failed {
                    self.failed = true;
                    return Err(io::Error::other("synthetic graphics failure"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        // Given a negotiated backend and a transient graphics writer failure.
        let now = Instant::now();
        let mut probe = crate::kitty::Probe::start(&mut Vec::new(), now).unwrap();
        probe.feed(b"\x1b_Gi=31;OK\x1b\\", now, &mut Vec::new());
        let mut allocation = crate::kitty::Allocation::start(
            probe.finish(&mut Vec::new()).unwrap(),
            std::num::NonZeroU32::MIN,
            &mut Vec::new(),
        )
        .unwrap();
        let kitty = allocation
            .feed(b"\x1b_Gi=90210,I=1;OK\x1b\\", now, &mut Vec::new())
            .unwrap();
        let mut cat = Compositor::new(24, 80);
        cat.confirm_profile(Profile::ConfirmedFixture);
        cat.use_kitty(kitty);
        cat.output(&fixture(), &mut Vec::new()).unwrap();
        let mut out = FailOnce {
            failed: false,
            bytes: Vec::new(),
        };
        // When a frame write fails, then release the owned image and draw cells.
        cat.tick(&mut out).unwrap();
        assert!(cat.kitty.is_none());
        assert!(cat.drawn.is_some());
        assert!(String::from_utf8_lossy(&out.bytes).contains("a=d,d=I,i=90210"));
        cat.input(b"a");
        cat.output(b"\x1b[20;3Ha", &mut out).unwrap();
        cat.finish(&mut out).unwrap();
        let evidence = std::env::var_os("BONGO_EVIDENCE_DIR").map_or_else(
            || std::path::PathBuf::from(".omo/evidence/kitty-launcher"),
            std::path::PathBuf::from,
        );
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("graphics-writer-fallback.bin"), out.bytes).unwrap();
    }
    fn fixture() -> Vec<u8> {
        format!(
            "\x1b[2J\x1b[1;1Hfixture\x1b[18;1H─ host │ model │ context\x1b[23;1H{}\x1b[20;3H",
            "─".repeat(80)
        )
        .into_bytes()
    }
    #[test]
    fn frames_are_coalesced_and_input_requires_confirmed_composer() {
        let mut c = Compositor::new(24, 80);
        let mut out = vec![];
        let now = Instant::now();
        c.input(b"ignored");
        assert_eq!(c.animation.pose(now), Pose::Idle);
        c.output(&fixture(), &mut out).unwrap();
        c.confirm_profile(Profile::ConfirmedFixture);
        c.tick_at(&mut out, now).unwrap();
        let size = out.len();
        for millis in 1..34 {
            c.tick_at(&mut out, now + Duration::from_millis(millis))
                .unwrap();
        }
        assert_eq!(size, out.len());
        c.input(b"a");
        c.tick_at(&mut out, now + Duration::from_millis(33))
            .unwrap();
        assert_eq!(size, out.len());
        c.tick_at(&mut out, now + Duration::from_millis(34))
            .unwrap();
        assert!(out.len() > size);
        assert_eq!(c.drawn_pose, Some(Pose::Left));
        c.tick_at(&mut out, now + Duration::from_millis(200))
            .unwrap();
        assert_eq!(c.drawn_pose, Some(Pose::Idle));
        let size = out.len();
        c.tick_at(&mut out, now + Duration::from_millis(400))
            .unwrap();
        assert_eq!(size, out.len());
    }
    #[test]
    fn unobserved_cells_after_resize_never_count_as_blank() {
        let mut c = Compositor::new(24, 80);
        let mut out = vec![];
        c.confirm_profile(Profile::ConfirmedFixture);
        c.resize(24, 80);
        c.output(&fixture()[4..], &mut out).unwrap();
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert_eq!(size, out.len());
        assert!(!c.composing());
    }
    #[test]
    fn passthrough_default_and_controlled_overlay() {
        let bytes = fixture();
        let mut c = Compositor::new(24, 80);
        let mut out = vec![];
        c.output(&bytes, &mut out).unwrap();
        c.tick(&mut out).unwrap();
        assert_eq!(out, bytes);
        c.confirm_profile(Profile::ConfirmedFixture);
        c.tick(&mut out).unwrap();
        assert!(out.windows(3).any(|b| b == "▀".as_bytes()));
        c.finish(&mut out).unwrap();
        let mut actual = vt100::Parser::new(24, 80, 0);
        actual.process(&out);
        let mut expected = vt100::Parser::new(24, 80, 0);
        expected.process(&bytes);
        assert_eq!(
            actual.screen().contents_formatted(),
            expected.screen().contents_formatted()
        );
        assert_eq!(
            actual.screen().cursor_position(),
            expected.screen().cursor_position()
        );
    }
    #[test]
    fn child_overwrites_old_cat_and_unknown_protocol_stays_hidden() {
        let mut c = Compositor::new(24, 80);
        c.confirm_profile(Profile::ConfirmedFixture);
        let mut out = vec![];
        c.output(&fixture(), &mut out).unwrap();
        c.tick(&mut out).unwrap();
        c.output(b"\x1b[14;63HX\x1b[20;3H", &mut out).unwrap();
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert_eq!(out.len(), size);
        let mut actual = vt100::Parser::new(24, 80, 0);
        actual.process(&out);
        assert_eq!(actual.screen().cell(13, 62).unwrap().contents(), "X");
        c.output(b"\x1b]1337;unsupported\x07", &mut out).unwrap();
        c.output(&fixture(), &mut out).unwrap();
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert_eq!(out.len(), size);
    }
    #[test]
    fn modal_alternate_resize_and_split_escape_hide() {
        for suffix in [b"\x1b[23;1H\x1b[2K".as_slice(), b"\x1b[?1049h", b"\x1b["] {
            let mut c = Compositor::new(24, 80);
            c.confirm_profile(Profile::ConfirmedFixture);
            let mut out = vec![];
            c.output(&fixture(), &mut out).unwrap();
            c.output(suffix, &mut out).unwrap();
            let size = out.len();
            c.tick(&mut out).unwrap();
            assert_eq!(out.len(), size);
        }
        let mut c = Compositor::new(24, 80);
        c.confirm_profile(Profile::ConfirmedFixture);
        let mut out = vec![];
        c.output(&fixture(), &mut out).unwrap();
        c.resize(40, 120);
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert_eq!(out.len(), size);
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests assert infallible Vec writes and fixture cells"
)]
mod resize_tests {
    use super::*;
    #[test]
    fn resize_hides_until_fresh_complete_chrome() {
        let mut c = Compositor::new(24, 80);
        c.confirm_profile(Profile::ConfirmedFixture);
        let mut out = vec![];
        c.resize(40, 120);
        c.output(b"\x1b[2J\x1b[1;1H", &mut out).unwrap();
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert_eq!(size, out.len());
        let redraw = format!(
            "\x1b[34;1H─ host │ model │ context\x1b[39;1H{}\x1b[36;3H",
            "─".repeat(120)
        );
        c.output(redraw.as_bytes(), &mut out).unwrap();
        let size = out.len();
        c.tick(&mut out).unwrap();
        assert!(out.len() > size);
        assert!(c.drawn.is_some());
    }
}

#[cfg(test)]
#[path = "../tests/hosts/hermes/compositor.rs"]
mod hermes_tests;

#[cfg(test)]
#[path = "../tests/hosts/codex/compositor.rs"]
mod codex_tests;
