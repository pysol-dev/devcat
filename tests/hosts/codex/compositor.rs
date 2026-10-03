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
        String::from_utf8_lossy(&out[before..]).contains("\x1b[39;103H\x1b[X"),
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
