use super::*;
fn screen(rows: u16) -> vt100::Parser {
    let mut parser = vt100::Parser::new(rows, 120, 0);
    parser.process(
        format!(
            "\x1b[?1049h\x1b[2J\x1b[{};2H─ ready │ model │ time │ voice\x1b[{};2H❯\x1b[{};4H",
            rows - 1,
            rows,
            rows
        )
        .as_bytes(),
    );
    parser
}
#[test]
fn anchor_requires_independent_composer_and_safe_rectangle() {
    // Given the observed stock geometry and deliberately erased spacer cells.
    let parser = screen(40);
    // When the structural profile evaluates that screen.
    let actual = anchor(parser.screen(), Profile::HermesTui, 7);
    // Then it positions the full rectangle immediately above the status row.
    assert_eq!(
        actual,
        Some(Anchor {
            row: 31,
            col: 102,
            height: 7,
            bar_row: None,
        })
    );
}
#[test]
fn hidden_when_composer_missing_occupied_styled_or_short() {
    // Given each independently unsafe layout, without inspecting draft text.
    for bytes in [
        b"\x1b[40;2H \x1b[40;4H".as_slice(),
        b"\x1b[32;103HX\x1b[40;4H",
        b"\x1b[32;103H\x1b[41m \x1b[0m\x1b[40;4H",
        b"\x1b[39;4H",
        b"\x1b[?25l",
        b"\x1b[?1049l",
    ] {
        let mut parser = screen(40);
        parser.process(bytes);
        // When evaluated, then no anchor may be returned.
        assert_eq!(anchor(parser.screen(), Profile::HermesTui, 7), None);
    }
    assert_eq!(anchor(screen(24).screen(), Profile::HermesTui, 7), None);
}
