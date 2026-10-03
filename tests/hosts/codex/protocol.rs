use super::*;

#[test]
fn observed_codex_setup_is_bounded_and_keeps_unknown_modes_hidden() {
    let mut guard = ProtocolGuard::default();
    guard.observe(b"\x1b[?2004h\x1b[?1004h\x1b[6n\x1b[?u\x1b[c\x1b[?1049h\x1b[?1007l\x1b[>7u\x1b[>4;0m\x1b[0 q\x1b[J");
    assert!(guard.ready());
    assert_eq!(guard.clear_generation, 1);
    for sequence in [b"\x1b[?1007h".as_slice(), b"\x1b[>8u", b"\x1b[?6h"] {
        let mut guard = ProtocolGuard::default();
        guard.observe(sequence);
        assert!(!guard.ready());
    }
}

#[test]
fn codex_home_then_erase_to_end_is_a_full_redraw_even_when_split() {
    let mut guard = ProtocolGuard::default();
    guard.observe(b"\x1b[?1049h");
    let initial = guard.clear_generation;
    guard.observe(b"\x1b[1;1H\x1b[");
    assert_eq!(guard.clear_generation, initial);
    guard.observe(b"J");
    assert_eq!(guard.clear_generation, initial + 1);
    guard.observe(b"\x1b[5;1H\x1b[J");
    guard.observe(b"\x1b[1;1Htext\x1b[J");
    assert_eq!(guard.clear_generation, initial + 1);
}
