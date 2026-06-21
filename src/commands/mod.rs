//! Command handlers and the shared execution context.
//!
//! `init` is implemented (#15); the feature commands are stubs that return
//! [`Error::NotImplemented`] until their phase lands.
//!
//! [`Error::NotImplemented`]: crate::error::Error::NotImplemented

mod capture;
mod decision;
mod flow;
mod hook;
mod init;
mod journey;
mod prompt;
mod reconcile;
mod why;

use std::env;
use std::path::PathBuf;

use crate::cli::{Cli, Command, DecisionCommand, HookCommand, PromptCommand};
use crate::config::OutputFormat;
use crate::error::Result;

/// Per-invocation context shared by all handlers.
pub struct Ctx {
    /// Directory to start repository discovery from.
    pub start_dir: PathBuf,
    /// How to render output.
    pub format: OutputFormat,
    /// Whether handlers may take over the terminal with an interactive UI.
    pub interactive: bool,
}

impl Ctx {
    /// Build a context from parsed CLI flags.
    fn from_cli(cli: &Cli) -> Result<Self> {
        use std::io::IsTerminal;

        let start_dir = match &cli.repo {
            Some(p) => p.clone(),
            None => env::current_dir()?,
        };
        let format = cli.format.unwrap_or_default();
        // Interactive only when the user hasn't opted out, stdout is a real
        // terminal, and the format is the human-readable text default.
        let interactive =
            !cli.plain && format == OutputFormat::Text && std::io::stdout().is_terminal();
        Ok(Ctx {
            start_dir,
            format,
            interactive,
        })
    }

    /// Emit a status + message in the active output format.
    pub fn emit(&self, status: &str, message: &str) {
        match self.format {
            OutputFormat::Json => {
                let obj = serde_json::json!({ "status": status, "message": message });
                println!("{obj}");
            }
            _ => println!("{message}"),
        }
    }
}

/// Dispatch a parsed CLI to the right handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    let ctx = Ctx::from_cli(&cli)?;
    match cli.command {
        Command::Init => init::run(&ctx),
        Command::Why { file, target } => why::run(&ctx, &file, target.as_deref()),
        Command::Flow {
            file,
            var,
            function,
            graph,
            history,
        } => flow::run(
            &ctx,
            &file,
            var.as_deref(),
            function.as_deref(),
            graph,
            history,
        ),
        Command::Journey { file, endpoint } => journey::run(&ctx, &file, &endpoint),
        Command::Decision { command } => match command {
            DecisionCommand::Add {
                file,
                var,
                line,
                message,
            } => decision::add(&ctx, &file, var.as_deref(), line, message),
        },
        Command::Prompt { command } => match command {
            PromptCommand::Add {
                file,
                line,
                session,
                message,
            } => prompt::add(&ctx, &file, &line, session.as_deref(), message),
            PromptCommand::List { file, full } => prompt::list(&ctx, file.as_deref(), full),
        },
        Command::Capture => capture::run(&ctx),
        Command::Reconcile => reconcile::run(&ctx),
        Command::Hook { command } => match command {
            HookCommand::Install { dry_run } => hook::install(&ctx, dry_run),
            HookCommand::Uninstall { dry_run } => hook::uninstall(&ctx, dry_run),
        },
    }
}
