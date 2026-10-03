#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub row: u16,
    pub col: u16,
    pub height: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Unconfirmed,
    ConfirmedFixture,
    HermesTui,
}

pub fn anchor(screen: &vt100::Screen, profile: Profile, height: u16) -> Option<Anchor> {
    match profile {
        Profile::Unconfirmed => return None,
        Profile::ConfirmedFixture => {}
        Profile::HermesTui => return hermes_anchor(screen, height),
    }
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();
    if height == 0
        || height > 10
        || rows < 12
        || cols < 24
        || screen.alternate_screen()
        || screen.hide_cursor()
        || cursor_col >= cols - 1
    {
        return None;
    }
    let lines: Vec<String> = screen.rows(0, cols).collect();
    let candidates: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, s)| (s.starts_with("─ ") && s.contains(" │ ")).then_some(i))
        .collect();
    let [bar] = candidates.as_slice() else {
        return None;
    };
    let bar = u16::try_from(*bar).ok()?;
    // A separately visible lower rule and cursor in the composer are required.
    if bar < height || cursor_row <= bar || cursor_row >= rows - 1 {
        return None;
    }
    let bottom = lines
        .iter()
        .enumerate()
        .skip(usize::from(cursor_row) + 1)
        .find(|(_, s)| {
            s.chars().filter(|c| *c == '─').count() >= usize::from(cols.saturating_sub(2))
                && s.chars().all(|c| c == '─' || c == ' ')
        })?;
    if bottom.0 != usize::from(rows - 2) {
        return None;
    }
    let a = Anchor {
        row: bar - height,
        col: cols - 18,
        height,
    };
    for r in a.row..bar {
        for c in a.col..a.col + 16 {
            let cell = screen.cell(r, c)?;
            if !cell.contents().trim_matches(' ').is_empty()
                || cell.is_wide_continuation()
                || cell.fgcolor() != vt100::Color::Default
                || cell.bold()
                || cell.dim()
                || cell.italic()
                || cell.bgcolor() != vt100::Color::Default
                || cell.inverse()
                || cell.underline()
            {
                return None;
            }
        }
    }
    Some(a)
}

pub fn hermes_empty_composer(screen: &vt100::Screen, height: u16) -> bool {
    screen.hide_cursor()
        && screen.cursor_position() == (screen.size().0.saturating_sub(1), 0)
        && hermes_geometry(screen, height, true).is_some()
}

fn hermes_anchor(screen: &vt100::Screen, height: u16) -> Option<Anchor> {
    hermes_geometry(screen, height, false)
}

fn hermes_geometry(screen: &vt100::Screen, height: u16, empty_cursor: bool) -> Option<Anchor> {
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();
    if !(1..=10).contains(&height)
        || rows < 40
        || cols < 80
        || !screen.alternate_screen()
        || (!empty_cursor && (screen.hide_cursor() || cursor_col < 3 || cursor_col >= cols - 1))
        || cursor_row != rows - 1
    {
        return None;
    }
    let bar = rows - 2;
    let status = screen.rows(0, cols).nth(usize::from(bar))?;
    if !status.starts_with(" ─ ")
        || status.matches(" │ ").count() < 3
        || !screen
            .cell(rows - 1, 0)?
            .contents()
            .trim_matches(' ')
            .is_empty()
        || screen.cell(rows - 1, 1)?.contents() != "❯"
        || !screen
            .cell(rows - 1, 2)?
            .contents()
            .trim_matches(' ')
            .is_empty()
    {
        return None;
    }
    let anchor = Anchor {
        row: bar - height,
        col: cols - 18,
        height,
    };
    for row in anchor.row..bar {
        for col in anchor.col..anchor.col + 16 {
            let cell = screen.cell(row, col)?;
            if !cell.contents().trim_matches(' ').is_empty()
                || cell.is_wide_continuation()
                || cell.fgcolor() != vt100::Color::Default
                || cell.bgcolor() != vt100::Color::Default
                || cell.bold()
                || cell.dim()
                || cell.italic()
                || cell.inverse()
                || cell.underline()
            {
                return None;
            }
        }
    }
    Some(anchor)
}

#[cfg(test)]
#[path = "../tests/hosts/hermes/profile.rs"]
mod hermes_tests;
