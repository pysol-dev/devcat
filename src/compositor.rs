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
    profile: Profile,
    drawn: Option<Anchor>,
    drawn_pose: Option<Pose>,
    last_frame: Option<Instant>,
    awaiting_clear: bool,
    frames: Option<[Vec<String>; 3]>,
}
impl Compositor {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), 0),
            guard: ProtocolGuard::default(),
            input: InputClassifier::default(),
            animation: Animation::new(),
            profile: Profile::Unconfirmed,
            drawn: None,
            drawn_pose: None,
            last_frame: None,
            awaiting_clear: true,
            frames: load_frames(),
        }
    }
    pub fn confirm_profile(&mut self, profile: Profile) {
        self.profile = profile;
    }
    pub fn input(&mut self, bytes: &[u8]) {
        if !self.composing() {
            self.input = InputClassifier::default();
            return;
        }
        let now = Instant::now();
        self.input.observe(bytes, || self.animation.strike(now));
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
    /// Remove our previous cells before forwarding child bytes, then model only child output.
    pub fn output(&mut self, bytes: &[u8], out: &mut impl Write) -> io::Result<()> {
        self.erase(out)?;
        out.write_all(bytes)?;
        let clear_generation = self.guard.clear_generation;
        self.guard.observe(bytes);
        if self.guard.clear_generation != clear_generation {
            self.awaiting_clear = false;
        }
        self.parser.process(bytes);
        Ok(())
    }
    /// Call finish before changing terminal geometry. The fresh model hides until the child redraws confirmed chrome.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.drawn = None;
        self.drawn_pose = None;
        self.last_frame = None;
        self.awaiting_clear = true;
        self.input = InputClassifier::default();
        self.parser = vt100::Parser::new(rows.max(1), cols.max(1), 0);
    }
    pub fn tick(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.tick_at(out, Instant::now())
    }
    fn tick_at(&mut self, out: &mut impl Write, now: Instant) -> io::Result<()> {
        if self.awaiting_clear || !self.guard.ready() {
            return Ok(());
        }
        let Some(anchor) = profile::anchor(self.parser.screen(), self.profile, self.frame_rows())
        else {
            return Ok(());
        };
        let pose = self.animation.pose(now);
        if self.drawn == Some(anchor) && self.drawn_pose == Some(pose) {
            return Ok(());
        }
        if self
            .last_frame
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(34))
        {
            return Ok(());
        }
        self.erase(out)?;
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
        self.last_frame = Some(now);
        for (offset, line) in (0_u16..).zip(&frames[index]) {
            write!(
                out,
                "\x1b[{};{}H{}",
                anchor.row + offset + 1,
                anchor.col + 1,
                line
            )?;
        }
        self.restore_cursor(out)
    }
    pub fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.erase(out)
    }
    fn erase(&mut self, out: &mut impl Write) -> io::Result<()> {
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
