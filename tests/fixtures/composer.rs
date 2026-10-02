//! Synthetic stock-like composer for controlled PTY acceptance, never a real-host gate.
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use nix::poll::{PollFd, PollFlags, poll};
use nix::sys::termios::{SetArg, Termios, cfmakeraw, tcgetattr, tcsetattr};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 37)]
    exit_code: u8,
}

struct Terminal(Termios);
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(error) = std::io::stdout().write_all(b"\x1b[0m\x1b[?25h\x1b[?2004l\x1b[?1049l") {
            eprintln!("fixture display cleanup failed: {error}");
        }
        if let Err(error) = tcsetattr(std::io::stdin(), SetArg::TCSANOW, &self.0) {
            eprintln!("fixture termios cleanup failed: {error}");
        }
    }
}

#[derive(Default)]
struct Composer {
    draft: String,
    utf8: Vec<u8>,
    escape: Vec<u8>,
    paste: bool,
    modal: bool,
    alternate: bool,
    bytes: Vec<u8>,
}

impl Composer {
    fn input(&mut self, byte: u8) -> bool {
        self.bytes.push(byte);
        if byte == 0x1b || !self.escape.is_empty() {
            self.escape.push(byte);
            match self.escape.as_slice() {
                b"\x1b[200~" => self.paste = true,
                b"\x1b[201~" => self.paste = false,
                b"\x1b" | b"\x1b[" => return true,
                sequence if sequence.last().is_some_and(u8::is_ascii_digit) => return true,
                _ => {}
            }
            self.escape.clear();
            return true;
        }
        if self.paste {
            self.append(byte);
        } else {
            match byte {
                3 | 4 => return false,
                15 => self.modal = !self.modal,
                20 => self.alternate = !self.alternate,
                8 | 127 => {
                    self.draft.pop();
                }
                _ => self.append(byte),
            }
        }
        true
    }

    fn append(&mut self, byte: u8) {
        if byte < 32 && byte != b'\n' && byte != b'\r' && byte != b'\t' {
            return;
        }
        self.utf8.push(byte);
        match std::str::from_utf8(&self.utf8) {
            Ok(text) => {
                self.draft.push_str(text);
                self.utf8.clear();
            }
            Err(error) if error.error_len().is_some() => self.utf8.clear(),
            Err(_) => {}
        }
    }

    fn draw(&self, output: &mut impl Write, size: (u16, u16)) -> Result<()> {
        let (cols, rows) = size;
        let status = rows.saturating_sub(6).max(1);
        let composer = rows.saturating_sub(4).max(1);
        let bottom = rows.saturating_sub(1).max(1);
        let status_text = "─ fixture │ model │ context ";
        write!(
            output,
            "\x1b[?25l\x1b[0m\x1b[2J\x1b[Hcontrolled composer fixture {cols}x{rows}\r\nCtrl+O modal | Ctrl+T alternate | Ctrl+D exit"
        )?;
        write!(
            output,
            "\x1b[{status};1H{status_text}{}\x1b[{bottom};1H{}",
            "─".repeat(usize::from(cols).saturating_sub(status_text.chars().count())),
            "─".repeat(usize::from(cols))
        )?;
        // Keep arbitrary synthetic drafts within the composer viewport, including wide UTF-8.
        let mut viewport = vt100::Parser::new(2, cols.max(1), 0);
        viewport.process(b"> ");
        viewport.process(self.draft.replace('\n', "\r\n").as_bytes());
        for (offset, row) in viewport.screen().rows(0, cols).enumerate() {
            let row_number = composer.saturating_add(u16::try_from(offset)?);
            write!(output, "\x1b[{row_number};1H{row}")?;
        }
        if self.modal {
            let row = status.saturating_sub(3).max(1);
            let col = cols.saturating_sub(17).max(1);
            write!(output, "\x1b[{row};{col}H\x1b[7m MODAL OPEN \x1b[0m")?;
        }
        let (row, col) = viewport.screen().cursor_position();
        write!(
            output,
            "\x1b[{};{}H\x1b[?25h",
            composer.saturating_add(row),
            col.saturating_add(1)
        )?;
        output.flush()?;
        Ok(())
    }
}

fn size() -> Result<(u16, u16)> {
    let (width, height) = terminal_size::terminal_size().context("fixture requires a PTY")?;
    Ok((width.0, height.0))
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let input = std::io::stdin();
    let saved = Terminal(tcgetattr(&input)?);
    let mut raw = saved.0.clone();
    cfmakeraw(&mut raw);
    tcsetattr(&input, SetArg::TCSANOW, &raw)?;
    let mut output = std::io::stdout();
    output.write_all(b"\x1b[?2004h")?;
    let mut composer = Composer::default();
    let mut dimensions = size()?;
    composer.draw(&mut output, dimensions)?;
    let mut buffer = [0; 1024];
    loop {
        let mut descriptors = [PollFd::new(input.as_fd(), PollFlags::POLLIN)];
        match poll(&mut descriptors, 50u16) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(error) => return Err(error.into()),
        }
        let resized = size()?;
        let mut redraw = resized != dimensions;
        dimensions = resized;
        if descriptors[0]
            .revents()
            .is_some_and(|flags| flags.intersects(PollFlags::POLLIN | PollFlags::POLLHUP))
        {
            let count = input.lock().read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let mut running = true;
            for byte in &buffer[..count] {
                let previous = composer.alternate;
                running = composer.input(*byte);
                if previous != composer.alternate {
                    output.write_all(if composer.alternate {
                        b"\x1b[?1049h"
                    } else {
                        b"\x1b[?1049l"
                    })?;
                }
                if !running {
                    break;
                }
            }
            if !running {
                break;
            }
            redraw = true;
        }
        if redraw {
            composer.draw(&mut output, dimensions)?;
        }
    }
    drop(saved);
    let receipt = serde_json::json!({"bytes": composer.bytes, "draft": composer.draft, "size": dimensions, "exit_code": args.exit_code});
    writeln!(output, "\r\nFIXTURE_RECEIPT:{receipt}")?;
    output.flush()?;
    Ok(ExitCode::from(args.exit_code))
}
