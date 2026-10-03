//! Same-window launcher for unmodified terminal applications.
mod animation;
mod cli;
mod compositor;
mod doctor;
mod input;
mod kitty;
mod profile;
mod pty;
mod screen;
mod sync_batch;
mod terminal;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

fn run() -> Result<ExitCode> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Doctor { host } => {
            doctor::report(&host, args.renderer, args.no_cat)?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Launch(command) => pty::run(&command, !args.no_cat, args.renderer),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("bongo: {error:#}");
            ExitCode::FAILURE
        }
    }
}
