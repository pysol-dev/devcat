//! Real PTY scenarios with a controlled, non-sensitive fixture.
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const INPUT: &[u8] = b"abcdef\x7f\x1b[200~paste \xf0\x9f\x90\xb1 e\xcc\x81\n\x1b[201~\x1b[A\x1b[I\x1b[<0;5;3M\x1b[12;34R\x00\x03\x04";

fn evidence_dir() -> Result<PathBuf> {
    let path = std::env::var_os("BONGO_EVIDENCE_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".omo/evidence/transport"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[derive(Clone, Copy)]
enum Environment {
    Present,
    Unset,
}

fn scenario(
    wrapper: Option<&Path>,
    name: &str,
    environment: Environment,
) -> Result<serde_json::Value> {
    let dir = evidence_dir()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = Path::new(env!("CARGO_BIN_EXE_bongo"))
        .parent()
        .context("binary directory")?
        .join("examples/transport-fixture");
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = CommandBuilder::new("/bin/sh");
    let supervisor = match environment {
        Environment::Present => {
            "before=$(stty -g); env _=fixture-environment \"$@\"; code=$?; after=$(stty -g); printf 'CLEANUP:%s|%s\\n' \"$before\" \"$after\"; exit \"$code\""
        }
        Environment::Unset => {
            "before=$(stty -g); env -u TERM -u COLORTERM -u SHELL _=fixture-environment \"$@\"; code=$?; after=$(stty -g); printf 'CLEANUP:%s|%s\\n' \"$before\" \"$after\"; exit \"$code\""
        }
    };
    command.args(["-c", supervisor, "fixture-shell"]);
    if let Some(binary) = wrapper {
        command.arg(binary);
        command.args(["--no-cat", "--"]);
    }
    command.arg(fixture);
    command.args(["--tui", "argument with spaces", "", "猫"]);
    command.cwd(root);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "fixture-truecolor");
    command.env("BONGO_FIXTURE_ENV", "inherited value");
    command.env("BONGO_FIXTURE_BYTES", INPUT.len().to_string());
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader()?;
    let (send, receive) = mpsc::sync_channel(8);
    let reader_thread = std::thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    let mut transcript = String::new();
    let ready = receive.recv_timeout(Duration::from_secs(10))??;
    transcript.push_str(&ready);
    transcript.push('\n');
    ensure!(ready == "READY", "fixture readiness: {ready}");
    pair.master.resize(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut writer = pair.master.take_writer()?;
    writer.write_all(INPUT)?;
    writer.flush()?;
    let receipt_line = receive.recv_timeout(Duration::from_secs(10))??;
    transcript.push_str(&receipt_line);
    transcript.push('\n');
    let cleanup = receive.recv_timeout(Duration::from_secs(10))??;
    transcript.push_str(&cleanup);
    transcript.push('\n');
    let status = child.wait()?;
    reader_thread
        .join()
        .map_err(|_| anyhow::anyhow!("fixture reader panicked"))?;
    std::fs::write(dir.join(format!("{name}.txt")), transcript)?;
    ensure!(status.exit_code() == 37, "exit status must be 37");
    let modes = cleanup
        .strip_prefix("CLEANUP:")
        .context("missing cleanup receipt")?;
    let (before, after) = modes
        .split_once('|')
        .context("missing terminal snapshots")?;
    ensure!(
        normalize_modes(before)? == normalize_modes(after)?,
        "terminal must be restored: {cleanup}"
    );
    let receipt: serde_json::Value = serde_json::from_str(&receipt_line)?;
    std::fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    ensure!(
        receipt["bytes"] == serde_json::json!(INPUT),
        "input bytes changed"
    );
    ensure!(
        receipt["args"] == serde_json::json!(["--tui", "argument with spaces", "", "猫"]),
        "arguments changed"
    );
    ensure!(
        receipt["size"] == serde_json::json!([120, 40]),
        "resize missing"
    );
    Ok(receipt)
}

fn normalize_modes(snapshot: &str) -> Result<String> {
    // macOS sets PENDIN when canonical input resumes. It is kernel queue state,
    // not a user terminal mode; retain every other bit in the comparison.
    snapshot
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
}

#[test]
fn direct_fixture_records_baseline() -> Result<()> {
    // Given a stock fixture in a real PTY, when exercised, then retain its oracle receipt.
    scenario(None, "direct", Environment::Present).map(|_| ())
}

#[test]
fn wrapper_preserves_transport_when_input_contains_terminal_protocols() -> Result<()> {
    // Given the direct oracle, when run through the launcher, then all receipts agree.
    let direct = scenario(None, "parity-direct", Environment::Present)?;
    let binary = Path::new(env!("CARGO_BIN_EXE_bongo"));
    let wrapped =
        scenario(Some(binary), "wrapped", Environment::Present).context("wrapped fixture")?;
    ensure!(direct == wrapped, "wrapper changed the fixture receipt");
    std::fs::write(
        evidence_dir()?.join("parity.txt"),
        "PASS: exact input bytes, args, cwd, environment, TERM/COLORTERM, 120x40 resize, exit 37, original termios\n",
    )?;
    Ok(())
}

#[test]
fn wrapper_keeps_environment_absent_when_terminal_variables_are_unset() -> Result<()> {
    // Given absent terminal and shell variables, when wrapped, then none are invented.
    let direct = scenario(None, "unset-direct", Environment::Unset)?;
    let binary = Path::new(env!("CARGO_BIN_EXE_bongo"));
    let wrapped = scenario(Some(binary), "unset-wrapped", Environment::Unset)?;
    ensure!(
        direct == wrapped,
        "wrapper injected an environment variable"
    );
    ensure!(
        wrapped["term"].is_null() && wrapped["colorterm"].is_null(),
        "terminal variables were set"
    );
    Ok(())
}
