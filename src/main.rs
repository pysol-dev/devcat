//! Same-window launcher for unmodified terminal applications.
mod cli;
mod doctor;
mod pty;
mod terminal;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

fn run() -> Result<ExitCode> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Doctor { host } => {
            doctor::report(&host)?;
            Ok(ExitCode::SUCCESS)
        }
        cli::Command::Launch(command) => pty::run(&command),
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
