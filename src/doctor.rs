use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;

pub(crate) fn report(host: &OsStr, renderer: crate::cli::Renderer, no_cat: bool) -> Result<()> {
    let path = Path::new(host);
    let executable = if path.components().count() > 1 {
        Some(path.to_path_buf()).filter(|path| is_executable(path))
    } else {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|path| path.join(host))
                .find(|path| is_executable(path))
        })
    };
    let mut output = std::io::stdout().lock();
    writeln!(output, "host: {}", host.to_string_lossy())?;
    match executable {
        Some(path) => writeln!(output, "executable: {}", path.display())?,
        None => writeln!(output, "executable: unavailable")?,
    }
    for key in ["TERM", "COLORTERM", "TERM_PROGRAM", "TMUX", "STY", "ZELLIJ"] {
        let value = std::env::var_os(key);
        writeln!(
            output,
            "{key}: {}",
            value
                .as_deref()
                .unwrap_or(OsStr::new("unset"))
                .to_string_lossy()
        )?;
    }
    match terminal_size::terminal_size() {
        Some((width, height)) => writeln!(output, "dimensions: {}x{}", width.0, height.0)?,
        None => writeln!(output, "dimensions: unavailable (not a terminal)")?,
    }
    let reason = if no_cat {
        "passthrough (cat disabled)"
    } else {
        match renderer {
            crate::cli::Renderer::Cells => "cells (explicit selection)",
            crate::cli::Renderer::Auto | crate::cli::Renderer::Kitty => {
                "cells (not probed: doctor does not consume terminal input; launcher negotiates before child spawn)"
            }
        }
    };
    writeln!(output, "renderer: {reason}")?;
    writeln!(
        output,
        "validation: transport only; host UI, authentication, graphics and animation are unvalidated"
    )?;
    Ok(())
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}
