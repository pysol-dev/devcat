use super::*;
#[test]
fn first_hit_waits_for_safe_echo_of_hidden_empty_composer() -> io::Result<()> {
    // Given stock empty-draft geometry: hidden cursor, with an independent composer marker.
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::HermesTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;1H\x1b[?25l"
            .as_bytes(),
        &mut out,
    )?;
    cat.input(b"a");
    assert!(!cat.composing());
    // When the stock echo exposes its composer cursor.
    cat.output(b"\x1b[40;4Ha\x1b[?25h", &mut out)?;
    cat.tick(&mut out)?;
    // Then the first hit is rendered only after the geometry is safe.
    assert_eq!(cat.drawn_pose, Some(Pose::Left));
    Ok(())
}

#[test]
fn modal_dismissal_never_queues_a_hit_for_reappearing_composer() -> io::Result<()> {
    // Given a modal that retains the status but removes the composer marker.
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::HermesTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;1H\x1b[?25l".as_bytes(),
        &mut out,
    )?;
    cat.input(b"q");
    // When a dismiss key is followed immediately by a valid composer.
    cat.output("\x1b[40;2H❯\x1b[40;4H\x1b[?25h".as_bytes(), &mut out)?;
    cat.tick(&mut out)?;
    // Then only idle is drawn, never a queued modal keystroke.
    assert_eq!(cat.drawn_pose, Some(Pose::Idle));
    Ok(())
}

#[test]
fn alternate_screen_resize_recovers_only_after_fresh_safe_redraw() -> io::Result<()> {
    // Given an active stock alternate-screen composer before a resize.
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::HermesTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;4H"
            .as_bytes(),
        &mut out,
    )?;
    cat.tick(&mut out)?;
    assert!(cat.drawn.is_some());
    cat.resize(24, 80);
    cat.output(
        "\x1b[2J\x1b[23;2H─ ready │ model │ time │ voice\x1b[24;2H❯\x1b[24;4H".as_bytes(),
        &mut out,
    )?;
    assert!(!cat.composing());
    cat.resize(40, 120);
    let before = out.len();
    cat.tick(&mut out)?;
    assert_eq!(out.len(), before, "resize must not erase stale coordinates");
    cat.output(
        "\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;4H".as_bytes(),
        &mut out,
    )?;
    assert!(!cat.composing(), "unobserved blank cells need a new clear");
    // When Hermes redraws without re-entering its already active alternate screen.
    cat.output(
        "\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;4H".as_bytes(),
        &mut out,
    )?;
    cat.input(b"a");
    cat.tick(&mut out)?;
    // Then safe composition and its next strike resume.
    assert!(cat.composing());
    assert_eq!(cat.drawn_pose, Some(Pose::Left));
    Ok(())
}

#[test]
fn hidden_cursor_stays_hidden_across_resize_until_observed_show() -> io::Result<()> {
    // Given the child explicitly hid the cursor in its alternate screen.
    let mut cat = Compositor::new(24, 80);
    cat.confirm_profile(Profile::HermesTui);
    let mut out = Vec::new();
    cat.output(b"\x1b[?1049h\x1b[?25l", &mut out)?;
    cat.resize(40, 120);
    cat.output(
        "\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;4H".as_bytes(),
        &mut out,
    )?;
    assert!(
        !cat.composing(),
        "fresh geometry does not prove a hidden cursor visible"
    );
    // When the child explicitly shows its cursor again.
    cat.output(b"\x1b[?25h", &mut out)?;
    // Then the independently safe composer becomes eligible.
    assert!(cat.composing());
    Ok(())
}

#[test]
fn safe_child_redraw_restores_last_pose_inside_animation_throttle() -> io::Result<()> {
    // Given a visible idle cat and a new strike still inside its 34ms frame budget.
    let mut cat = Compositor::new(40, 120);
    cat.confirm_profile(Profile::HermesTui);
    let mut out = Vec::new();
    cat.output(
        "\x1b[?1049h\x1b[2J\x1b[39;2H─ ready │ model │ time │ voice\x1b[40;2H❯\x1b[40;4H"
            .as_bytes(),
        &mut out,
    )?;
    let now = Instant::now();
    cat.tick_at(&mut out, now)?;
    cat.input(b"a");
    // When a harmless child update erases the old overlay during that budget.
    cat.output(b"\x1b]2;synthetic title\x07", &mut out)?;
    cat.tick_at(&mut out, now + Duration::from_millis(1))?;
    // Then restore idle immediately, but do not advance the animation early.
    assert!(cat.drawn.is_some());
    assert_eq!(cat.drawn_pose, Some(Pose::Idle));
    assert_eq!(cat.last_frame, Some(now));
    Ok(())
}
