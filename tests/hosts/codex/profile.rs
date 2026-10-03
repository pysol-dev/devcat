use super::*;

fn screen() -> vt100::Parser {
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› \x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[40;96H⚠ warnings\x1b[37;3H"
            .as_bytes(),
    );
    parser
}

#[test]
fn codex_anchor_requires_independent_composer_and_footer() {
    let parser = screen();
    assert_eq!(
        anchor(parser.screen(), Profile::CodexTui, 4),
        Some(Anchor {
            row: 34,
            col: 102,
            height: 4,
            bar_row: Some(38),
        })
    );
}

#[test]
fn codex_hides_for_modal_overflow_style_or_narrow_layout() {
    for bytes in [
        b"\x1b[37;1H  \x1b[37;3H".as_slice(),
        b"\x1b[37;103Hlong draft\x1b[37;4H",
        b"\x1b[35;103Hmodal\x1b[37;4H",
        b"\x1b[35;103H\x1b[41m \x1b[0m\x1b[37;4H",
        b"\x1b[39;1H\x1b[2K\x1b[37;4H",
        b"\x1b[?25l",
        b"\x1b[?1049l",
    ] {
        let mut parser = screen();
        parser.process(bytes);
        assert_eq!(anchor(parser.screen(), Profile::CodexTui, 4), None);
    }
    let mut narrow = vt100::Parser::new(24, 80, 0);
    narrow.process(
        b"\x1b[?1049h\x1b[2J\x1b[21;1H\xe2\x80\xba \x1b[23;1H  model \xc2\xb7 /tmp\x1b[21;3H",
    );
    assert_eq!(anchor(narrow.screen(), Profile::CodexTui, 4), None);
}
