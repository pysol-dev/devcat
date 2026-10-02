use std::ffi::OsString;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, arg_required_else_help = true)]
pub(crate) struct Cli {
    /// Run transparent PTY transport without a cat.
    #[arg(long)]
    pub(crate) no_cat: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Inspect executable availability and terminal capabilities.
    Doctor {
        #[arg(long, default_value = "hermes")]
        host: OsString,
    },
    #[command(external_subcommand)]
    Launch(Vec<OsString>),
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn host_arguments_are_preserved_when_launcher_options_end() -> anyhow::Result<()> {
        // Given stock-host and explicit-command forms, when parsed, then retain all child arguments.
        for (input, expected) in [
            (
                vec!["bongo", "--no-cat", "hermes", "--tui"],
                vec!["hermes", "--tui"],
            ),
            (vec!["bongo", "--no-cat", "codex"], vec!["codex"]),
            (
                vec!["bongo", "--no-cat", "--", "custom", "--no-cat", ""],
                vec!["custom", "--no-cat", ""],
            ),
        ] {
            let parsed = Cli::try_parse_from(input)?;
            match parsed.command {
                Command::Launch(args) => assert_eq!(args, expected),
                Command::Doctor { .. } => anyhow::bail!("launch parsed as doctor"),
            }
        }
        Ok(())
    }
}
