//! Controlled transport oracle; only synthetic test input is printed here.
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    let stdin = std::io::stdin();
    let saved = nix::sys::termios::tcgetattr(&stdin)?;
    let mut raw = saved.clone();
    nix::sys::termios::cfmakeraw(&mut raw);
    nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSANOW, &raw)?;
    let mut output = std::io::stdout().lock();
    writeln!(output, "READY")?;
    output.flush()?;
    let count: usize = std::env::var("BONGO_FIXTURE_BYTES")?.parse()?;
    let mut bytes = vec![0; count];
    stdin.lock().read_exact(&mut bytes)?;
    let size = terminal_size::terminal_size();
    let mut environment: Vec<_> = std::env::vars_os().collect();
    environment.sort();
    let mut digest = std::collections::hash_map::DefaultHasher::new();
    environment.hash(&mut digest);
    let receipt = serde_json::json!({
        "args": std::env::args().skip(1).collect::<Vec<_>>(),
        "cwd": std::env::current_dir()?,
        "term": std::env::var_os("TERM"),
        "colorterm": std::env::var_os("COLORTERM"),
        "env": std::env::var_os("BONGO_FIXTURE_ENV"),
        "environment_digest": digest.finish(),
        "bytes": bytes,
        "size": size.map(|(w, h)| (w.0, h.0)),
    });
    writeln!(output, "{receipt}")?;
    output.flush()?;
    nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSANOW, &saved)?;
    Ok(ExitCode::from(37))
}
