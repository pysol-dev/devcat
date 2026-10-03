//! Compares real wrapped and direct PTYs using only synthetic input.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

struct Capture {
    parser: vt100::Parser,
    raw: Vec<u8>,
    receive: Receiver<std::io::Result<Vec<u8>>>,
}
impl Capture {
    fn drain(&mut self, duration: Duration) -> Result<()> {
        let end = Instant::now() + duration;
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            match self.receive.recv_timeout(remaining) {
                Ok(Ok(bytes)) => {
                    self.parser.process(&bytes);
                    self.raw.extend_from_slice(&bytes);
                }
                Ok(Err(error)) if error.raw_os_error() == Some(nix::libc::EIO) => break,
                Ok(Err(error)) => return Err(error.into()),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }
}
struct Scenario {
    screens: Vec<(String, vt100::Screen)>,
    receipt: serde_json::Value,
}
fn dimensions(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}
fn evidence_dir() -> PathBuf {
    std::env::var_os("BONGO_EVIDENCE_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join(".omo/evidence/compositor-transport"),
        PathBuf::from,
    )
}
#[expect(
    clippy::too_many_lines,
    reason = "One ordered real-PTY scenario with an owned reader and cleanup"
)]
fn scenario(wrapped: bool, evidence: &Path) -> Result<Scenario> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("target/debug/examples/composer-fixture");
    let pair = native_pty_system().openpty(dimensions(24, 80))?;
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "before=$(stty -g); \"$@\"; code=$?; after=$(stty -g); printf '\\r\\nCLEANUP:%s|%s\\n' \"$before\" \"$after\"; exit \"$code\"", "fixture-shell"]);
    if wrapped {
        command.arg(env!("CARGO_BIN_EXE_bongo"));
        command.args(["--renderer", "cells", "--"]);
    }
    command.arg(&fixture);
    command.cwd(root);
    command.env("BONGO_TEST_PROFILE", "fixture");
    command.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let (send, receive) = mpsc::channel();
    let reading = std::thread::spawn(move || {
        loop {
            let mut bytes = [0; 8192];
            match reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    if send.send(Ok(bytes[..count].to_vec())).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    if let Err(send_error) = send.send(Err(error)) {
                        eprintln!("capture receiver closed: {send_error}");
                    }
                    break;
                }
            }
        }
    });
    let mut capture = Capture {
        parser: vt100::Parser::new(24, 80, 0),
        raw: vec![],
        receive,
    };
    let startup_deadline = Instant::now() + Duration::from_secs(5);
    while !capture
        .parser
        .screen()
        .contents()
        .contains("controlled composer fixture")
    {
        ensure!(
            Instant::now() < startup_deadline,
            "fixture readiness timeout"
        );
        capture.drain(Duration::from_millis(20))?;
    }
    capture.drain(Duration::from_millis(150))?;
    let mut screens = vec![("idle".into(), capture.parser.screen().clone())];
    for (name, input) in [
        ("left-a", b"a".as_slice()),
        ("right-b", b"b"),
        ("left-c", b"c"),
        ("right-d", b"d"),
        ("left-e", b"e"),
        ("right-f", b"f"),
        ("left-backspace", b"\x7f"),
        ("paste", b"\x1b[200~ paste\x1b[201~"),
        ("modal", b"\x0f"),
        ("restored", b"\x0f"),
    ] {
        writer.write_all(input)?;
        writer.flush()?;
        capture.drain(Duration::from_millis(55))?;
        screens.push((name.into(), capture.parser.screen().clone()));
        capture.drain(Duration::from_millis(130))?;
    }
    pair.master.resize(dimensions(40, 120))?;
    capture.parser.screen_mut().set_size(40, 120);
    capture.drain(Duration::from_millis(250))?;
    screens.push(("resized".into(), capture.parser.screen().clone()));
    writer.write_all(b"\x04")?;
    writer.flush()?;
    capture.drain(Duration::from_secs(5))?;
    let status = child.wait()?;
    reading
        .join()
        .map_err(|_| anyhow::anyhow!("reader panicked"))?;
    let label = if wrapped { "wrapped" } else { "direct" };
    std::fs::write(evidence.join(format!("{label}.ansi")), &capture.raw)?;
    let text = String::from_utf8_lossy(&capture.raw);
    let receipt_text = text
        .split("FIXTURE_RECEIPT:")
        .nth(1)
        .context("fixture receipt")?
        .lines()
        .next()
        .context("receipt line")?
        .trim();
    let receipt: serde_json::Value = serde_json::from_str(receipt_text)?;
    let cleanup = text
        .split("CLEANUP:")
        .nth(1)
        .context("cleanup receipt")?
        .lines()
        .next()
        .context("cleanup line")?
        .trim();
    let (before, after) = cleanup.split_once('|').context("cleanup states")?;
    let normalize = |state: &str| -> Result<String> {
        state
            .split(':')
            .map(|field| match field.strip_prefix("lflag=") {
                Some(hex) if cfg!(target_os = "macos") => Ok(format!(
                    "lflag={:x}",
                    u64::from_str_radix(hex, 16)? & !0x2000_0000
                )),
                Some(_) | None => Ok(field.to_owned()),
            })
            .collect::<Result<Vec<_>>>()
            .map(|fields| fields.join(":"))
    };
    ensure!(
        normalize(before)? == normalize(after)?,
        "terminal attributes not restored"
    );
    ensure!(status.exit_code() == 37, "child exit changed");
    std::fs::write(
        evidence.join(format!("{label}-receipt.json")),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    std::fs::write(
        evidence.join(format!("{label}-cleanup.txt")),
        format!("exit=37\ntermios_equal=true\n{cleanup}\n"),
    )?;
    Ok(Scenario { screens, receipt })
}
#[test]
fn wrapped_composer_matches_direct_outside_cat_and_preserves_draft() -> Result<()> {
    let evidence = evidence_dir();
    std::fs::create_dir_all(&evidence)?;
    let direct = scenario(false, &evidence)?;
    let wrapped = scenario(true, &evidence)?;
    ensure!(
        direct.receipt == wrapped.receipt,
        "draft/input receipt differs"
    );
    let art: serde_json::Value =
        serde_json::from_str(include_str!("../assets/generated/compact-cells.json"))?;
    let mut checks = vec![];
    for ((name, expected), (actual_name, actual)) in direct.screens.iter().zip(&wrapped.screens) {
        ensure!(name == actual_name, "scenario order changed");
        let (rows, cols) = expected.size();
        let height = u16::try_from(art["rows"].as_u64().context("art rows")?)?;
        let top = rows - 7 - height;
        let left = cols - 18;
        for row in 0..rows {
            for col in 0..cols {
                if name != "modal"
                    && (top..top + height).contains(&row)
                    && (left..left + 16).contains(&col)
                {
                    continue;
                }
                ensure!(
                    expected.cell(row, col) == actual.cell(row, col),
                    "{name}: cell {row},{col} changed outside cat"
                );
            }
        }
        ensure!(
            expected.cursor_position() == actual.cursor_position(),
            "{name}: cursor changed"
        );
        if name != "modal" {
            let pose = if name.starts_with("left") {
                "left"
            } else if name.starts_with("right") || name == "paste" {
                "right"
            } else {
                "idle"
            };
            for row in 0..height {
                let actual_row: String = (left..left + 16)
                    .map(|col| {
                        actual.cell(top + row, col).map_or_else(
                            || " ",
                            |cell| {
                                let text = cell.contents();
                                if text.is_empty() { " " } else { text }
                            },
                        )
                    })
                    .collect();
                ensure!(
                    Some(actual_row.as_str())
                        == art["frames"][pose]["monochrome_rows"][usize::from(row)].as_str(),
                    "{name}: wrong {pose} pose row {row}"
                );
            }
        }
        checks.push(serde_json::json!({"scenario":name,"outside_cells_equal":true,"cursor_equal":true,"pose_verified":name!="modal"}));
    }
    std::fs::write(
        evidence.join("parity.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"pass":true,"receipt_equal":true,"checks":checks}),
        )?,
    )?;
    Ok(())
}
