//! Controlled composer acceptance in a real PTY.
#[path = "fixtures/composer_session.rs"]
mod composer_session;

use anyhow::{Result, ensure};
use composer_session::Session;
use portable_pty::PtySize;

fn blank_safe_area(screen: &vt100::Screen) -> bool {
    let (rows, cols) = screen.size();
    (rows - 12..rows - 7).all(|row| {
        (cols - 18..cols - 2).all(|col| {
            screen.cell(row, col).is_some_and(|cell| {
                cell.contents().trim().is_empty()
                    && cell.fgcolor() == vt100::Color::Default
                    && cell.bgcolor() == vt100::Color::Default
                    && !cell.bold()
                    && !cell.inverse()
            })
        })
    })
}

#[test]
fn composer_transitions_when_driven_through_a_real_pty() -> Result<()> {
    // Given a synthetic composer with an actual 80x24 PTY.
    let mut session = Session::start()?;
    session.frame("01-initial", |screen| {
        screen.cursor_position() == (19, 2)
            && screen.contents().contains("─ fixture │ model │ context")
    })?;
    ensure!(
        blank_safe_area(session.parser.screen()),
        "initial safe area must be blank default style"
    );
    ensure!(
        session.parser.screen().rows(0, 80).nth(22) == Some("─".repeat(80)),
        "bottom rule must occupy row23"
    );
    // When typing, deleting, pasting, opening a modal, resizing, toggling screens, then exiting.
    for byte in b"abcdef" {
        session.send(&[*byte])?;
        let expected = char::from(*byte).to_string();
        session.frame(&format!("02-key-{expected}"), |screen| {
            screen
                .rows(0, 80)
                .nth(19)
                .is_some_and(|line| line.ends_with(&expected))
        })?;
    }
    session.send(b"\x7f")?;
    session.frame("03-backspace", |screen| {
        screen.rows(0, 80).nth(19) == Some("> abcde".to_owned())
    })?;
    session.send(b"\x1b[200~ paste \xf0\x9f\x90\xb1\x1b[201~")?;
    session.frame("04-paste", |screen| {
        screen.contents().contains("> abcde paste 🐱")
    })?;
    session.send(b"\x0f")?;
    session.frame("05-modal", |screen| {
        screen.contents().contains("MODAL OPEN")
    })?;
    ensure!(
        !blank_safe_area(session.parser.screen()),
        "modal must occupy safe area"
    );
    session.send(b"\x0f")?;
    session.frame("06-modal-closed", |screen| {
        !screen.contents().contains("MODAL OPEN")
    })?;
    session.parser.screen_mut().set_size(40, 120);
    session.master.resize(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    session.frame("07-resize", |screen| {
        screen.contents().contains("120x40") && screen.cursor_position().0 == 35
    })?;
    ensure!(
        blank_safe_area(session.parser.screen()),
        "resized safe area must be blank"
    );
    ensure!(
        session.parser.screen().rows(0, 120).nth(38) == Some("─".repeat(120)),
        "resized bottom rule must occupy row39"
    );
    session.send(b"\x14")?;
    session.frame("08-alternate", vt100::Screen::alternate_screen)?;
    session.send(b"\x14")?;
    session.frame("09-primary", |screen| !screen.alternate_screen())?;
    session.send(b"\x04")?;
    // Then the complete draft and exact synthetic input survive every transition.
    let receipt = session.finish()?;
    ensure!(
        receipt["draft"] == "abcde paste 🐱",
        "draft must survive transitions"
    );
    let expected: &[u8] =
        b"abcdef\x7f\x1b[200~ paste \xf0\x9f\x90\xb1\x1b[201~\x0f\x0f\x14\x14\x04";
    ensure!(
        receipt["bytes"] == serde_json::json!(expected),
        "exact input bytes preserved"
    );
    Ok(())
}
