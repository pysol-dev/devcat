use super::*;

#[test]
fn codex_contact_bar_is_owned_and_restored_on_modal() -> io::Result<()> {
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› \x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;3H"
            .as_bytes(),
        &mut out,
    )?;
    cat.input(b"a");
    cat.tick(&mut out)?;
    assert_eq!(cat.drawn_pose, Some(Pose::Left));
    assert!(String::from_utf8_lossy(&out).contains("▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔"));
    let before = out.len();
    cat.output(b"\x1b[37;1H\x1b[2K\x1b[39;1H\x1b[2K", &mut out)?;
    cat.tick(&mut out)?;
    assert!(cat.drawn.is_none());
    assert!(
        String::from_utf8_lossy(&out[before..]).contains("\x1b[39;103H\x1b[16X"),
        "the owned footer cells must be erased before the child's modal appears"
    );
    Ok(())
}

#[test]
fn first_codex_key_survives_synchronized_cursor_hide_only_after_composer_echo() -> io::Result<()> {
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› \x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;3H"
            .as_bytes(),
        &mut out,
    )?;
    cat.tick(&mut out)?;
    cat.output(b"\x1b[?2026h\x1b[?25l", &mut out)?;
    cat.input(b"a");
    assert!(
        cat.pending_hit.is_some(),
        "the safe composer should buffer this first key"
    );
    cat.output(b"\x1b[37;3Ha\x1b[37;4H\x1b[?25h\x1b[?2026l", &mut out)?;
    assert!(
        cat.pending_hit.is_some(),
        "safe echo must retain the pending hit"
    );
    assert!(cat.composing(), "the echoed composer should be eligible");
    cat.tick_at(&mut out, Instant::now() + Duration::from_millis(40))?;
    assert_eq!(cat.drawn_pose, Some(Pose::Left));
    Ok(())
}

#[test]
fn codex_modal_transition_cannot_turn_a_navigation_key_into_a_strike() -> io::Result<()> {
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› \x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;3H"
            .as_bytes(),
        &mut out,
    )?;
    cat.output(b"\x1b[?2026h\x1b[?25l", &mut out)?;
    cat.input(b"q");
    cat.output(
        b"\x1b[37;1H\x1b[2K\x1b[35;1Hmodel picker\x1b[?2026l",
        &mut out,
    )?;
    cat.output(b"\x1b[2J\x1b[37;1H\xe2\x80\xba \x1b[39;1H  GPT-6-Sol high \xc2\xb7 /private/tmp/qa\x1b[37;3H\x1b[?25h", &mut out)?;
    cat.tick(&mut out)?;
    assert_eq!(cat.drawn_pose, Some(Pose::Idle));
    Ok(())
}

#[test]
fn codex_keeps_cat_when_synchronized_update_has_not_changed_safe_chrome() -> io::Result<()> {
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› a\x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;4H"
            .as_bytes(),
        &mut out,
    )?;
    cat.tick(&mut out)?;
    assert!(cat.drawn.is_some());
    cat.output(b"\x1b[?2026h", &mut out)?;
    cat.tick(&mut out)?;
    assert!(
        cat.drawn.is_some(),
        "a synchronization marker alone must not leave the cat absent"
    );
    cat.output(
        b"\x1b[37;1H\x1b[2K\x1b[35;1Hmodel picker\x1b[?2026l",
        &mut out,
    )?;
    cat.tick(&mut out)?;
    assert!(cat.drawn.is_none(), "the modal must still hide the cat");
    Ok(())
}

#[test]
fn mode_only_output_keeps_cat_until_a_cell_mutation() -> io::Result<()> {
    assert!(non_cell_output(b"\x1b[?25l\x1b[1;1H\x1b[?2026h"));
    assert!(non_cell_output(b"\x1b]2;qa title\x07\x1b[0m\x1b[37;4H"));
    assert!(!non_cell_output(b"\x1b[1;1H\x1b[J"));
    assert!(!non_cell_output(b"\x1b[?1049h"));
    assert!(!non_cell_output(b"\x1b]8;;https://example.invalid\x07"));
    assert!(!non_cell_output(b"\x1b[?25"));

    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› a\x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;4H"
            .as_bytes(),
        &mut out,
    )?;
    cat.tick(&mut out)?;
    cat.output(b"\x1b[?25l", &mut out)?;
    assert!(cat.drawn.is_some());
    cat.output(b"\x1b[37;1H\x1b[2K", &mut out)?;
    cat.tick(&mut out)?;
    assert!(cat.drawn.is_none());
    Ok(())
}

#[test]
fn erasing_owned_blank_rows_fits_before_the_next_pty_chunk() -> io::Result<()> {
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::CodexTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[37;1H› a\x1b[39;1H  GPT-6-Sol high · /private/tmp/qa\x1b[37;4H"
            .as_bytes(),
        &mut out,
    )?;
    cat.tick(&mut out)?;
    out.clear();
    cat.output(b"\x1b[37;1H\x1b[2K", &mut out)?;
    assert!(
        out.len() < 250,
        "erasing the five owned rows must be compact"
    );
    for row in 35..=39 {
        let expected = format!("\x1b[{row};103H\x1b[16X");
        assert!(
            out.windows(expected.len())
                .any(|bytes| bytes == expected.as_bytes())
        );
    }
    Ok(())
}
