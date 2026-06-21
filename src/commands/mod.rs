//! Command handlers and the shared execution context.
//!
//! `init` is implemented (#15); the feature commands are stubs that return
//! [`Error::NotImplemented`] until their phase lands.
//!
//! [`Error::NotImplemented`]: crate::error::Error::NotImplemented

mod capture;
mod hook;
mod init;
mod prompt;
mod reconcile;
mod why;

use std::env;
use std::path::PathBuf;

use crate::cli::{Cli, Command, DecisionCommand, HookCommand, PromptCommand};
use crate::config::OutputFormat;
use crate::error::{Error, Result};

/// Per-invocation context shared by all handlers.
pub struct Ctx {
    /// Directory to start repository discovery from.
    pub start_dir: PathBuf,
    /// How to render output.
    pub format: OutputFormat,
}

impl Ctx {
    /// Build a context from parsed CLI flags.
    fn from_cli(cli: &Cli) -> Result<Self> {
        let start_dir = match &cli.repo {
            Some(p) => p.clone(),
            None => env::current_dir()?,
        };
        Ok(Ctx {
            start_dir,
            format: cli.format.unwrap_or_default(),
        })
    }

    /// Emit a status + message in the active output format.
    pub fn emit(&self, status: &str, message: &str) {
        match self.format {
            OutputFormat::Text => println!("{message}"),
            OutputFormat::Json => {
                let obj = serde_json::json!({ "status": status, "message": message });
                println!("{obj}");
            }
        }
    }
}

/// Dispatch a parsed CLI to the right handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    let ctx = Ctx::from_cli(&cli)?;
    match cli.command {
        Command::Init => init::run(&ctx),
        Command::Why { file, target } => why::run(&ctx, &file, target.as_deref()),
        Command::Flow { .. } => not_implemented("flow", "Phase 4: Data Flow Analysis"),
        Command::Journey { .. } => not_implemented("journey", "Phase 6: Cross-Layer Journey"),
        Command::Decision { command } => match command {
            DecisionCommand::Add { .. } => {
                not_implemented("decision add", "Phase 7: Decision Editor")
            }
        },
        Command::Prompt { command } => match command {
            PromptCommand::Add {
                file,
                line,
                session,
                message,
            } => prompt::add(&ctx, &file, &line, session.as_deref(), message),
            PromptCommand::List { file } => prompt::list(&ctx, file.as_deref()),
        },
        Command::Capture => capture::run(&ctx),
        Command::Reconcile => reconcile::run(&ctx),
        Command::Hook { command } => match command {
            HookCommand::Install { dry_run } => hook::install(&ctx, dry_run),
            HookCommand::Uninstall { dry_run } => hook::uninstall(&ctx, dry_run),
        },
    }
}

fn not_implemented(command: &'static str, phase: &'static str) -> Result<()> {
    Err(Error::NotImplemented { command, phase })
}
