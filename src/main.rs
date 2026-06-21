//! tellme — git blame, but for prompts and decisions.
//!
//! Thin binary entry point: parse the CLI, set up logging, dispatch to a
//! command handler, and map any [`tellme::error::Error`] to a user-readable
//! message + process exit code. The real logic lives in the `tellme` library.

use std::process::ExitCode;

use clap::Parser;
use tellme::cli::Cli;
use tellme::commands;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    tracing::debug!("tellme v{} starting", env!("CARGO_PKG_VERSION"));
    match commands::dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Configure `tracing` from the `-v` count, honouring `RUST_LOG` if set.
fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("tellme={default}")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
