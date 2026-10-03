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
