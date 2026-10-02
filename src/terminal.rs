use std::io::{IsTerminal, Stdin};

use anyhow::{Result, ensure};
use nix::sys::termios::{SetArg, Termios, cfmakeraw, tcgetattr, tcsetattr};
use portable_pty::PtySize;

pub(crate) struct Terminal {
    input: Stdin,
    saved: Termios,
}

impl Terminal {
    pub(crate) fn enter() -> Result<Self> {
        let input = std::io::stdin();
        ensure!(
            input.is_terminal() && std::io::stdout().is_terminal(),
            "launch requires a terminal on stdin and stdout"
        );
        let saved = tcgetattr(&input)?;
        let terminal = Self { input, saved };
        terminal.resume()?;
        Ok(terminal)
    }

    pub(crate) fn restore(&self) -> Result<()> {
        tcsetattr(&self.input, SetArg::TCSANOW, &self.saved)?;
        Ok(())
    }

    pub(crate) fn resume(&self) -> Result<()> {
        let mut raw = self.saved.clone();
        cfmakeraw(&mut raw);
        tcsetattr(&self.input, SetArg::TCSANOW, &raw)?;
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("bongo: terminal restore failed: {error:#}");
        }
    }
}

pub(crate) fn size() -> Result<PtySize> {
    let (width, height) = terminal_size::terminal_size()
        .ok_or_else(|| anyhow::anyhow!("terminal dimensions unavailable"))?;
    Ok(PtySize {
        rows: height.0,
        cols: width.0,
        pixel_width: 0,
        pixel_height: 0,
    })
}
