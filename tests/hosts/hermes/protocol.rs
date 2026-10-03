use super::*;
#[test]
fn observed_setup_is_non_cell_effect_and_split_safe() {
    // Given exact stock setup queries and input-mode changes.
    let bytes = b"\x1b[?1003l\x1b[?9l\x1b[?1001l\x1b[?1005l\x1b[?1015l\x1b[?1016l\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1004h\x1b[?1006h\x1b[?2029l\x1b[0'z\x1b[0'{\x1b[?1000$p\x1b[c\x1b[>0q\x1b[<u\x1b[>4m\x1b]0;synthetic title\x07\x1b]10;?\x1b\\\x1b]11;?\x07";
    let mut guard = ProtocolGuard::default();
    // When bytes arrive across every possible sequence boundary.
    for byte in bytes {
        guard.observe(&[*byte]);
    }
    // Then the exact setup leaves cell composition available.
    assert!(guard.ready());
}
#[test]
fn synchronized_redraw_waits_for_end() {
    // Given a synchronized child redraw.
    let mut guard = ProtocolGuard::default();
    guard.observe(b"\x1b[?2026h\x1b[2J");
    assert!(!guard.ready());
    // When the child completes it, then composition becomes ready.
    guard.observe(b"\x1b[?2026l");
    assert!(guard.ready());
}
#[test]
fn unknown_neighbors_stay_blocked() {
    // Given unsupported relatives of accepted commands.
    for bytes in [
        b"\x1b[?2029h".as_slice(),
        b"\x1b[1'z",
        b"\x1b[>5m",
        b"\x1b]11;#ffffff\x07",
        b"\x1b]8;;url\x07",
    ] {
        let mut guard = ProtocolGuard::default();
        // When received, then even a later clear does not enable composition.
        guard.observe(bytes);
        guard.observe(b"\x1b[2J");
        assert!(!guard.ready());
    }
}
