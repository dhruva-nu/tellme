//! tellme — git blame, but for prompts and decisions.
//!
//! This is the scaffolding entry point (issue #9). The full subcommand surface
//! (`why`, `flow`, `journey`, `decision`, `prompt`, `init`) is built in #10.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Top-level CLI definition.
#[derive(Debug, Parser)]
#[command(
    name = "tellme",
    version,
    about = "Git blame, but for prompts and decisions.",
    long_about = None
)]
struct Cli {
    /// Increase log verbosity (-v debug, -vv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    tracing::debug!("tellme v{} starting", env!("CARGO_PKG_VERSION"));
    println!(
        "tellme v{} — scaffolding in place. Commands land in #10.",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        // Panics at test time if the clap derive is misconfigured.
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_verbose_flags() {
        let cli = Cli::try_parse_from(["tellme", "-vv"]).unwrap();
        assert_eq!(cli.verbose, 2);
    }
}
