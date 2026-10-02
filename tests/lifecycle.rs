//! Process lifecycle checks on real PTYs.
use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

fn capture(command: CommandBuilder) -> Result<(std::process::ExitStatus, String)> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let (send, receive) = mpsc::sync_channel(1);
    let reading = std::thread::spawn(move || {
        let mut output = String::new();
        let result = reader.read_to_string(&mut output).map(|_| output);
        if let Err(error) = send.send(result) {
            eprintln!("test receiver stopped: {error}");
        }
    });
    let output = receive.recv_timeout(Duration::from_secs(10))??;
    reading
        .join()
        .map_err(|_| anyhow::anyhow!("reader panicked"))?;
    let handle: &mut dyn portable_pty::Child = child.as_mut();
    let status = handle
        .downcast_mut::<std::process::Child>()
        .context("Unix child handle")?
        .wait()?;
    Ok((status, output))
}

fn record(name: &str, content: &str) -> Result<()> {
    let dir = std::env::var_os("BONGO_EVIDENCE_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".omo/evidence/transport"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), content)?;
    Ok(())
}

#[test]
fn wrapper_keeps_signal_status_when_child_terminates_by_signal() -> Result<()> {
    // Given a child that kills itself with TERM, when wrapped, then the launcher also dies by TERM.
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bongo"));
    command.args(["--no-cat", "--", "/bin/sh", "-c", "kill -TERM $$"]);
    let (status, output) = capture(command)?;
    record(
        "signal.txt",
        &format!(
            "command: bongo --no-cat -- /bin/sh -c 'kill -TERM $$'\nstatus: {status:?}\noutput: {output}\n"
        ),
    )?;
    ensure!(
        status.signal() == Some(nix::libc::SIGTERM),
        "termination signal changed: {status:?}"
    );
    Ok(())
}

#[test]
fn wrapper_restores_terminal_when_executable_is_missing() -> Result<()> {
    // Given a missing executable, when spawn fails, then cooked input and echo are restored.
    let mut command = CommandBuilder::new("/bin/sh");
    command.args([
        "-c",
        "\"$1\" --no-cat -- /bongo-test-no-such-executable; code=$?; stty -a; exit \"$code\"",
        "cleanup",
    ]);
    command.arg(env!("CARGO_BIN_EXE_bongo"));
    let (status, output) = capture(command)?;
    record(
        "spawn-failure.txt",
        &format!("status: {status:?}\n{output}"),
    )?;
    ensure!(status.code() == Some(1), "missing executable must fail");
    ensure!(
        !output.contains("-icanon") && !output.contains("-echo "),
        "terminal remained raw: {output}"
    );
    Ok(())
}

#[test]
fn doctor_reports_dimensions_when_attached_to_a_terminal() -> Result<()> {
    // Given an 80x24 PTY, when doctor inspects sh, then it reports the actual terminal size.
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_bongo"));
    command.args(["doctor", "--host", "/bin/sh"]);
    let (status, output) = capture(command)?;
    record("doctor-pty.txt", &format!("status: {status:?}\n{output}"))?;
    ensure!(status.success(), "doctor failed");
    ensure!(
        output.contains("dimensions: 80x24"),
        "doctor missed terminal dimensions"
    );
    ensure!(
        output.contains("executable: /bin/sh"),
        "doctor missed executable"
    );
    Ok(())
}
