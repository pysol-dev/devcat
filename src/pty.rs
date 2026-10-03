use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{Signal, killpg, raise};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, native_pty_system};
use signal_hook::consts::signal::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGTSTP, SIGWINCH};
use signal_hook::iterator::Signals;

use crate::terminal::{self, Terminal};
use crate::{compositor::Compositor, profile::Profile};

#[expect(
    clippy::too_many_lines,
    reason = "Keep PTY ownership and unconditional compositor cleanup in one lexical scope"
)]
pub(crate) fn run(args: &[OsString], cat_enabled: bool) -> Result<ExitCode> {
    let command = command(args)?;
    let terminal = Terminal::enter()?;
    let mut signals = Signals::new([SIGWINCH, SIGINT, SIGTERM, SIGHUP, SIGQUIT, SIGTSTP, SIGCONT])?;
    let size = terminal::size()?;
    let mut compositor = cat_enabled.then(|| Compositor::new(size.rows, size.cols));
    if let Some(cat) = &mut compositor {
        if fixture_profile(args) {
            cat.confirm_profile(Profile::ConfirmedFixture);
        } else if args.first().is_some_and(|p| {
            std::path::Path::new(p).file_name() == Some(std::ffi::OsStr::new("hermes"))
        }) && args.iter().skip(1).any(|arg| arg == "--tui")
        {
            cat.confirm_profile(Profile::HermesTui);
        }
    }
    let pair = native_pty_system().openpty(size)?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .context("spawn stock executable")?;
    drop(pair.slave);
    let pid = Pid::from_raw(i32::try_from(
        child.process_id().context("child PID unavailable")?,
    )?);
    let mut writer = Some(pair.master.take_writer()?);
    let mut reader = pair.master.try_clone_reader()?;
    let (send, receive) = mpsc::sync_channel(16);
    let output_thread = std::thread::spawn(move || read_output(reader.as_mut(), &send));
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout().lock();
    let mut buffer = [0; 8192];
    let mut composed_output = Vec::with_capacity(16384);
    let transport_result: Result<()> = (|| {
        loop {
            for signal in signals.pending() {
                match signal {
                    SIGWINCH => {
                        let size = terminal::size()?;
                        // SIGWINCH arrives after terminal reflow: discard old coordinates instead of erasing child cells.
                        if let Some(cat) = &mut compositor {
                            cat.resize(size.rows, size.cols);
                        }
                        pair.master.resize(size)?;
                    }
                    SIGTSTP => {
                        if let Some(cat) = &mut compositor {
                            cat.finish(&mut output)?;
                        }
                        output.flush()?;
                        terminal.restore()?;
                        forward(pid, Signal::SIGTSTP)?;
                        raise(Signal::SIGSTOP)?;
                        terminal.resume()?;
                        let size = terminal::size()?;
                        if let Some(cat) = &mut compositor {
                            cat.resize(size.rows, size.cols);
                        }
                        pair.master.resize(size)?;
                        forward(pid, Signal::SIGCONT)?;
                    }
                    SIGCONT => {
                        terminal.resume()?;
                        let size = terminal::size()?;
                        if let Some(cat) = &mut compositor {
                            cat.resize(size.rows, size.cols);
                        }
                        pair.master.resize(size)?;
                        forward(pid, Signal::SIGCONT)?;
                    }
                    other => forward(pid, Signal::try_from(other)?)?,
                }
            }
            if let Some(destination) = writer.as_mut() {
                let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
                match poll(&mut fds, PollTimeout::ZERO) {
                    Ok(count) if count > 0 => {
                        let count = match input.read(&mut buffer) {
                            Ok(count) => count,
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                continue;
                            }
                            Err(error) => return Err(error.into()),
                        };
                        if count == 0 {
                            writer.take();
                        } else {
                            destination.write_all(&buffer[..count])?;
                            destination.flush()?;
                            if let Some(cat) = &mut compositor {
                                cat.input(&buffer[..count]);
                            }
                        }
                    }
                    Ok(_) | Err(nix::errno::Errno::EINTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            match receive.recv_timeout(Duration::from_millis(2)) {
                Ok(Ok(bytes)) => {
                    match &mut compositor {
                        Some(cat) => {
                            composed_output.clear();
                            cat.output(&bytes, &mut composed_output)?;
                            cat.tick(&mut composed_output)?;
                            output.write_all(&composed_output)?;
                        }
                        None => output.write_all(&bytes)?,
                    }
                    output.flush()?;
                }
                Ok(Err(error)) if error.raw_os_error() == Some(nix::libc::EIO) => break,
                Ok(Err(error)) => return Err(error.into()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if let Some(cat) = &mut compositor {
                cat.tick(&mut output)?;
                output.flush()?;
            }
        }
        Ok(())
    })();
    if let Some(cat) = &mut compositor {
        cat.finish(&mut output)?;
        output.flush()?;
    }
    transport_result?;
    output_thread
        .join()
        .map_err(|_| anyhow::anyhow!("PTY output reader panicked"))?;
    // portable-pty's ExitStatus erases the numeric Unix signal. Keep the stock
    // process status so shells can distinguish signal death from exit(1).
    let child_handle: &mut dyn portable_pty::Child = child.as_mut();
    let status = child_handle
        .downcast_mut::<std::process::Child>()
        .context("Unix process handle unavailable")?
        .wait()?;
    terminal.restore()?;
    match (status.code(), status.signal()) {
        (Some(code), _) => Ok(ExitCode::from(u8::try_from(code)?)),
        (None, Some(signal)) => {
            drop(terminal);
            drop(signals);
            signal_hook::low_level::emulate_default_handler(signal)?;
            anyhow::bail!("signal default handler returned unexpectedly")
        }
        (None, None) => anyhow::bail!("child has no final process status"),
    }
}

fn command(args: &[OsString]) -> Result<CommandBuilder> {
    let program = args.first().context("missing executable")?;
    let mut command = if std::env::var_os("SHELL").is_some() {
        let mut command = CommandBuilder::new(program);
        command.args(args.iter().skip(1));
        command
    } else {
        // portable-pty injects SHELL unconditionally on Unix. env replaces
        // itself with the stock executable after removing that injected value.
        let mut command = CommandBuilder::new("/usr/bin/env");
        command.args(["-u", "SHELL", "--"]);
        command.args(args);
        command
    };
    command.cwd(std::env::current_dir()?);
    command.env_clear();
    for (key, value) in std::env::vars_os() {
        command.env(key, value);
    }
    Ok(command)
}

fn read_output(reader: &mut dyn Read, send: &mpsc::SyncSender<std::io::Result<Vec<u8>>>) {
    loop {
        let mut buffer = [0; 8192];
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if send.send(Ok(buffer[..count].to_vec())).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                if send.send(Err(error)).is_err() {
                    break;
                }
                break;
            }
        }
    }
}

fn forward(pid: Pid, signal: Signal) -> Result<()> {
    match killpg(pid, signal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn fixture_profile(args: &[OsString]) -> bool {
    if std::env::var_os("BONGO_TEST_PROFILE").as_deref() != Some(std::ffi::OsStr::new("fixture")) {
        return false;
    }
    let Some(program) = args.first() else {
        return false;
    };
    let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug/examples/composer-fixture");
    match (
        std::fs::canonicalize(program),
        std::fs::canonicalize(expected),
    ) {
        (Ok(actual), Ok(expected)) => actual == expected,
        _ => false,
    }
}
