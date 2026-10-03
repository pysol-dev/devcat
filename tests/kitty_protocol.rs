//! Wire evidence only: these tests do not certify any native terminal renderer.
#![expect(
    clippy::unwrap_used,
    reason = "assert exact protocol invariants in tests"
)]
#[path = "../src/animation.rs"]
mod animation;
#[path = "../src/kitty.rs"]
mod kitty;
#[expect(
    dead_code,
    reason = "isolated backend harness imports production anchor types"
)]
#[path = "../src/profile.rs"]
mod profile;
use animation::Pose;
use base64::{Engine, engine::general_purpose::STANDARD};
use kitty::{Allocation, Capability, Kitty, Placement, Probe, ProbeStatus};
use std::{io, num::NonZeroU32, time::Instant};

fn capability() -> Capability {
    let now = Instant::now();
    let mut probe = Probe::start(&mut Vec::new(), now).unwrap();
    let mut passthrough = Vec::new();
    probe.feed(b"\x1b_Gi=31;OK\x1b\\", now, &mut passthrough);
    probe.finish(&mut passthrough).unwrap()
}
fn backend(out: &mut Vec<u8>) -> Kitty {
    let mut allocation =
        Allocation::start(capability(), NonZeroU32::new(73).unwrap(), out).unwrap();
    allocation
        .feed(
            b"\x1b_Gi=123456,I=73;OK\x1b\\",
            Instant::now(),
            &mut Vec::new(),
        )
        .unwrap()
}
fn frame(pose: Pose) -> Placement {
    Placement {
        anchor: profile::Anchor {
            row: 8,
            col: 20,
            height: 5,
            bar_row: None,
        },
        columns: 16,
        pose,
        cursor: (17, 4),
    }
}

#[test]
fn fragmented_probe_preserves_unrelated_bytes_at_every_split() {
    // Given a query and a response surrounded by input and unrelated terminal replies.
    let bytes = b"a\x1b[?1;2c\x1b_Gi=99;OK\x1b\\\x1b_Gi=31;OK\x1b\\z";
    for split in 0..=bytes.len() {
        let now = Instant::now();
        let mut wire = Vec::new();
        let mut p = Probe::start(&mut wire, now).unwrap();
        let mut forwarded = Vec::new();
        // When the response is fragmented at any byte boundary.
        p.feed(&bytes[..split], now, &mut forwarded);
        p.feed(&bytes[split..], now, &mut forwarded);
        // Then only our reply is consumed and the exact query was emitted.
        assert_eq!(p.status(), &ProbeStatus::Supported);
        assert_eq!(forwarded, b"a\x1b[?1;2c\x1b_Gi=99;OK\x1b\\z");
        assert_eq!(wire, kitty::PROBE_QUERY);
        if split == 0 {
            if let Ok(dir) = std::env::var("KITTY_EVIDENCE_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(format!("{dir}/probe.protocol.bin"), &wire).unwrap();
            }
        }
    }
}
#[test]
fn timeout_releases_partial_response_and_fails_closed() {
    // Given an incomplete query reply.
    let now = Instant::now();
    let mut p = Probe::start(&mut Vec::new(), now).unwrap();
    let mut forwarded = Vec::new();
    p.feed(b"\x1b_Gi=31;O", now, &mut forwarded);
    // When the exact deadline expires.
    p.expire(now + kitty::PROBE_TIMEOUT, &mut forwarded);
    // Then bytes are returned unchanged and no capability is issued.
    assert_eq!(p.status(), &ProbeStatus::Unsupported);
    assert!(p.finish(&mut forwarded).is_none());
    assert_eq!(forwarded, b"\x1b_Gi=31;O");
}
#[test]
fn negative_and_oversized_probe_replies_cannot_enable_backend() {
    // Given rejection and an overlong unterminated reply.
    for input in [
        b"\x1b_Gi=31;ENOTSUP\x1b\\".to_vec(),
        [b"\x1b_Gi=31;".as_slice(), &vec![b'a'; 4096]].concat(),
    ] {
        let now = Instant::now();
        let mut p = Probe::start(&mut Vec::new(), now).unwrap();
        let mut forwarded = Vec::new();
        // When all bytes are observed then negotiation ends.
        p.feed(&input, now, &mut forwarded);
        // Then neither response grants a capability.
        assert!(p.finish(&mut forwarded).is_none());
        if input.len() > 256 {
            assert_eq!(forwarded, input);
        }
    }
}
#[test]
fn allocation_matches_number_and_preserves_other_replies() {
    // Given terminal allocation with our number 73.
    let mut wire = Vec::new();
    let mut a = Allocation::start(capability(), NonZeroU32::new(73).unwrap(), &mut wire).unwrap();
    let mut forwarded = Vec::new();
    // When unrelated replies precede a byte-fragmented matching reply.
    assert!(
        a.feed(b"\x1b_Gi=456,I=72;OK\x1b\\", Instant::now(), &mut forwarded)
            .is_none()
    );
    let mut result = None;
    for b in b"\x1b_Gi=123456,I=73;OK\x1b\\" {
        if let Some(backend) = a.feed(&[*b], Instant::now(), &mut forwarded) {
            result = Some(backend);
        }
    }
    // Then only the allocated ID is owned and cancellation cannot delete it.
    let mut backend = result.unwrap();
    let mut deletion = Vec::new();
    backend.finish(&mut deletion).unwrap();
    assert_eq!(deletion, b"\x1b\\\x1b_Ga=d,d=I,i=123456,q=2\x1b\\");
    assert_eq!(forwarded, b"\x1b_Gi=456,I=72;OK\x1b\\");
    a.cancel(&mut Vec::new(), &mut forwarded).unwrap();
}
#[test]
fn cancelled_allocation_deletes_only_its_number() {
    // Given an allocation without a complete reply.
    let mut a =
        Allocation::start(capability(), NonZeroU32::new(73).unwrap(), &mut Vec::new()).unwrap();
    let mut forwarded = Vec::new();
    a.feed(b"\x1b_Gi=12", Instant::now(), &mut forwarded);
    assert!(a.expired(Instant::now() + kitty::PROBE_TIMEOUT));
    let mut wire = Vec::new();
    // When cancelled on timeout or shutdown.
    a.cancel(&mut wire, &mut forwarded).unwrap();
    // Then partial input is preserved and cancellation is scoped.
    assert_eq!(forwarded, b"\x1b_Gi=12");
    assert_eq!(wire, b"\x1b\\\x1b_Ga=d,d=N,I=73,q=2\x1b\\");
}
#[test]
fn original_png_payloads_and_placement_survive_pose_changes() {
    // Given a terminal-allocated backend and the three original transparent PNGs.
    let mut wire = Vec::new();
    let mut b = backend(&mut wire);
    let original = [
        include_bytes!("../assets/generated/idle.png").as_slice(),
        include_bytes!("../assets/generated/left.png").as_slice(),
        include_bytes!("../assets/generated/right.png").as_slice(),
    ];
    // When idle, left and right are drawn then hidden.
    for pose in [Pose::Idle, Pose::Left, Pose::Right] {
        b.draw(frame(pose), &mut wire).unwrap();
    }
    b.hide(&mut wire).unwrap();
    b.finish(&mut wire).unwrap();
    // Then every decoded upload exactly equals original bytes and chunks obey the protocol.
    let text = std::str::from_utf8(&wire).unwrap();
    let mut images = Vec::new();
    let mut encoded = String::new();
    for command in text.split("\x1b_G").skip(1) {
        let command = command.split("\x1b\\").next().unwrap();
        if let Some((metadata, payload)) = command.split_once(';') {
            assert!(payload.len() <= 4096);
            assert_eq!(payload.len() % 4, 0);
            encoded.push_str(payload);
            if metadata.contains("m=0") {
                images.push(STANDARD.decode(&encoded).unwrap());
                encoded.clear();
            }
        }
    }
    assert_eq!(images.len(), 3);
    for (decoded, expected) in images.iter().zip(original) {
        assert_eq!(decoded, expected);
        assert_eq!(decoded[25], 6);
    }
    assert_eq!(text.matches("p=1,C=1,z=-1,c=16,r=5,q=2").count(), 3);
    assert_eq!(text.matches("\x1b[9;21H").count(), 3);
    assert_eq!(text.matches("\x1b[18;5H").count(), 3);
    assert_eq!(text.matches("a=d,d=I,i=123456").count(), 1);
    assert!(!text.contains("\x1b7"));
    if let Ok(dir) = std::env::var("KITTY_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}/poses.protocol.bin"), &wire).unwrap();
        for (name, data) in ["idle", "left", "right"].into_iter().zip(images) {
            std::fs::write(format!("{dir}/{name}.decoded.png"), data).unwrap();
        }
    }
}
#[test]
fn placement_is_redrawn_after_clear_or_resize() {
    // Given an already drawn image whose placement the terminal may have erased.
    let mut b = backend(&mut Vec::new());
    b.draw(frame(Pose::Idle), &mut Vec::new()).unwrap();
    let mut wire = Vec::new();
    // When the caller draws the fresh confirmed anchor again.
    b.draw(frame(Pose::Idle), &mut wire).unwrap();
    // Then placement and cursor restore are emitted even for unchanged pose/anchor.
    assert_eq!(
        wire,
        b"\x1b[9;21H\x1b_Ga=p,i=123456,p=1,C=1,z=-1,c=16,r=5,q=2\x1b\\\x1b[18;5H"
    );
}
#[test]
fn cleanup_remains_retryable_after_writer_failure() {
    // Given an owned image and a writer that fails.
    struct Broken;
    impl io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut b = backend(&mut Vec::new());
    assert!(b.finish(&mut Broken).is_err());
    let mut wire = Vec::new();
    // When cleanup is retried through a working writer.
    b.finish(&mut wire).unwrap();
    // Then ownership was retained until deletion was written.
    assert_eq!(wire, b"\x1b\\\x1b_Ga=d,d=I,i=123456,q=2\x1b\\");
}

#[test]
fn late_allocation_response_fails_closed() {
    // Given a pending allocation after its bounded deadline.
    let mut a =
        Allocation::start(capability(), NonZeroU32::new(73).unwrap(), &mut Vec::new()).unwrap();
    let late = Instant::now() + kitty::PROBE_TIMEOUT;
    let mut forwarded = Vec::new();
    // When a matching response arrives too late.
    let result = a.feed(b"\x1b_Gi=123456,I=73;OK\x1b\\", late, &mut forwarded);
    // Then no renderer is created and input remains intact for caller routing.
    assert!(result.is_none());
    assert_eq!(forwarded, b"\x1b_Gi=123456,I=73;OK\x1b\\");
    a.cancel(&mut Vec::new(), &mut forwarded).unwrap();
}

#[test]
fn hide_retains_image_and_finish_permanently_releases_it() {
    // Given a placed terminal-owned image.
    let mut b = backend(&mut Vec::new());
    b.draw(frame(Pose::Idle), &mut Vec::new()).unwrap();
    let mut wire = Vec::new();
    // When temporarily hidden, redrawn, then permanently finished.
    b.hide(&mut wire).unwrap();
    b.draw(frame(Pose::Idle), &mut wire).unwrap();
    b.finish(&mut wire).unwrap();
    b.finish(&mut wire).unwrap();
    let released_bytes = wire.len();
    let error = b.draw(frame(Pose::Left), &mut wire).unwrap_err();
    // Then hide deletes only the placement; finish deletes once; no ID reuse occurs.
    let text = std::str::from_utf8(&wire).unwrap();
    assert!(text.starts_with("\x1b\\\x1b_Ga=d,d=i,i=123456,p=1,q=2\x1b\\"));
    assert_eq!(text.matches("a=d,d=I,i=123456").count(), 1);
    assert_eq!(text.matches("a=p,i=123456").count(), 1);
    assert!(!text.contains("a=t"));
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(wire.len(), released_bytes);
}

#[test]
fn failed_hide_keeps_placement_owned_for_retry() {
    struct FailOnce {
        failed: bool,
        bytes: Vec<u8>,
    }
    impl io::Write for FailOnce {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.failed {
                self.failed = true;
                return Err(io::Error::other("synthetic write failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Given a placed image and a writer with one failed write.
    let mut b = backend(&mut Vec::new());
    b.draw(frame(Pose::Idle), &mut Vec::new()).unwrap();
    let mut out = FailOnce {
        failed: false,
        bytes: Vec::new(),
    };
    assert!(b.hide(&mut out).is_err());
    // When hide is retried and animation resumes.
    b.hide(&mut out).unwrap();
    b.draw(frame(Pose::Left), &mut out).unwrap();
    b.finish(&mut out).unwrap();
    // Then retry cleans our placement and the same still-owned ID is used.
    let text = std::str::from_utf8(&out.bytes).unwrap();
    assert_eq!(text.matches("a=d,d=i,i=123456,p=1").count(), 1);
    assert_eq!(text.matches("a=p,i=123456").count(), 1);
    assert_eq!(text.matches("a=d,d=I,i=123456").count(), 1);
}
