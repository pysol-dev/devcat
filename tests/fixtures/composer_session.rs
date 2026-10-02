//! Drives the synthetic composer through a real PTY; this proves no stock-host gate.
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

pub(super) struct Session {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Option<std::thread::JoinHandle<()>>,
    receive: Receiver<std::io::Result<Vec<u8>>>,
    writer: Box<dyn Write + Send>,
    pub(super) master: Box<dyn portable_pty::MasterPty + Send>,
    pub(super) parser: vt100::Parser,
    raw: Vec<u8>,
    directory: PathBuf,
    finished: bool,
}

impl Session {
    pub(super) fn start() -> Result<Self> {
        let directory = std::env::var_os("BONGO_EVIDENCE_DIR").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".omo/evidence/composer-fixture"),
            PathBuf::from,
        );
        std::fs::create_dir_all(&directory)?;
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/debug/examples/composer-fixture");
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "before=$(stty -g); \"$@\"; code=$?; after=$(stty -g); printf 'CLEANUP:%s|%s\\n' \"$before\" \"$after\"; exit \"$code\"", "fixture-shell"]);
        command.arg(fixture);
        command.args(["--exit-code", "23"]);
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let mut input = pair.master.try_clone_reader()?;
        let (send, receive) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut bytes = [0; 4096];
            loop {
                let result = match input.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => Ok(bytes[..count].to_vec()),
                    Err(error) => Err(error),
                };
                let failed = result.is_err();
                if send.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            reader: Some(reader),
            receive,
            writer: pair.master.take_writer()?,
            master: pair.master,
            parser: vt100::Parser::new(24, 80, 0),
            raw: Vec::new(),
            directory,
            finished: false,
        })
    }

    pub(super) fn frame(
        &mut self,
        name: &str,
        ready: impl Fn(&vt100::Screen) -> bool,
    ) -> Result<()> {
        loop {
            let chunk = self.receive.recv_timeout(Duration::from_secs(5))??;
            self.raw.extend_from_slice(&chunk);
            std::fs::write(self.directory.join("terminal.raw"), &self.raw)?;
            self.parser.process(&chunk);
            let screen = self.parser.screen();
            if !screen.hide_cursor() && ready(screen) {
                std::fs::write(
                    self.directory.join(format!("{name}.txt")),
                    screen.contents(),
                )?;
                std::fs::write(
                    self.directory.join(format!("{name}.ansi")),
                    screen.state_formatted(),
                )?;
                return Ok(());
            }
        }
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.child.kill() {
                eprintln!("fixture kill: {error}");
            }
            if let Err(error) = self.child.wait() {
                eprintln!("fixture reap: {error}");
            }
        }
        if let Some(reader) = self.reader.take() {
            if reader.join().is_err() {
                eprintln!("fixture reader panicked");
            }
        }
    }
}

impl Session {
    pub(super) fn finish(&mut self) -> Result<serde_json::Value> {
        while let Ok(chunk) = self.receive.recv_timeout(Duration::from_secs(5)) {
            self.raw.extend_from_slice(&chunk?);
        }
        std::fs::write(self.directory.join("terminal.raw"), &self.raw)?;
        let transcript = String::from_utf8(self.raw.clone())?;
        std::fs::write(
            self.directory.join("terminal-escaped.txt"),
            format!("{transcript:?}\n"),
        )?;
        // Then the draft, original bytes, chosen exit status and terminal modes are observable.
        let receipt_line = transcript
            .split("FIXTURE_RECEIPT:")
            .nth(1)
            .context("exit receipt")?
            .lines()
            .next()
            .context("receipt JSON")?;
        let receipt: serde_json::Value = serde_json::from_str(receipt_line)?;
        ensure!(receipt["exit_code"] == 23, "configured receipt exit");
        ensure!(
            self.child.wait()?.exit_code() == 23,
            "configured actual exit"
        );
        std::fs::write(
            self.directory.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        let cleanup = transcript
            .split("CLEANUP:")
            .nth(1)
            .context("cleanup")?
            .trim();
        let (before, after) = cleanup.split_once('|').context("before/after termios")?;
        let normalize = |value: &str| -> Result<Vec<String>> {
            value
                .split(':')
                .map(|field| match field.strip_prefix("lflag=") {
                    Some(hex) if cfg!(target_os = "macos") => Ok(format!(
                        "lflag={:x}",
                        u64::from_str_radix(hex, 16)? & !0x2000_0000
                    )),
                    _ => Ok(field.to_owned()),
                })
                .collect()
        };
        ensure!(
            normalize(before)? == normalize(after)?,
            "original termios restored"
        );
        ensure!(
            transcript.contains("\x1b[?2004l\x1b[?1049l"),
            "display modes reset"
        );
        std::fs::write(
            self.directory.join("cleanup.txt"),
            format!(
                "PASS exit=23, child reaped, original termios restored (macOS PENDIN masked)\n{cleanup}\n"
            ),
        )?;
        self.finished = true;
        Ok(receipt)
    }
}
