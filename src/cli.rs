//! The full clap command surface (#10).
//!
//! Every planned subcommand is defined here so the UX shape is fixed early;
//! unimplemented ones dispatch to handlers that return [`Error::NotImplemented`].
//!
//! [`Error::NotImplemented`]: crate::error::Error::NotImplemented

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::OutputFormat;

/// Top-level CLI: global flags plus a subcommand.
#[derive(Debug, Parser)]
#[command(
    name = "tellme",
    version,
    about = "Git blame, but for prompts and decisions.",
    long_about = None
)]
pub struct Cli {
    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Operate on the repository at this path instead of the current directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// Output format.
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// All tellme subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize the `.tellme/` store in the current repository.
    Init,

    /// Show the prompts that shaped a line (the signature feature).
    Why {
        /// Source file.
        file: PathBuf,
        /// Line selector, e.g. `#line4` or `4`.
        target: Option<String>,
    },

    /// Trace data flow of a variable or function within a file.
    Flow {
        /// Source file.
        file: PathBuf,
        /// Trace a variable's lifecycle.
        #[arg(long, value_name = "NAME")]
        var: Option<String>,
        /// Trace a function's callers/callees.
        #[arg(long, value_name = "NAME")]
        function: Option<String>,
        /// Render a graph instead of a list.
        #[arg(long)]
        graph: bool,
        /// Include the timeline of changes.
        #[arg(long)]
        history: bool,
    },

    /// Trace a piece of data across architectural layers (DB → API).
    Journey {
        /// Source file containing the endpoint.
        file: PathBuf,
        /// Endpoint/controller name.
        #[arg(long, value_name = "NAME")]
        endpoint: String,
    },

    /// Record or inspect decisions ("why").
    Decision {
        /// Decision subcommand.
        #[command(subcommand)]
        command: DecisionCommand,
    },

    /// Record or inspect captured prompts.
    Prompt {
        /// Prompt subcommand.
        #[command(subcommand)]
        command: PromptCommand,
    },

    /// Ingest a Claude Code hook event from stdin (used by installed hooks).
    #[command(hide = true)]
    Capture,

    /// Promote captured edits to anchors for newly committed code.
    Reconcile,

    /// Install or remove the capture hooks.
    Hook {
        /// Hook subcommand.
        #[command(subcommand)]
        command: HookCommand,
    },
}

/// `tellme decision ...`
#[derive(Debug, Subcommand)]
pub enum DecisionCommand {
    /// Attach a written decision to a line or variable.
    Add {
        /// Source file.
        file: PathBuf,
        /// Attach to a variable.
        #[arg(long, value_name = "NAME")]
        var: Option<String>,
        /// Attach to a line number.
        #[arg(long, value_name = "N")]
        line: Option<usize>,
    },
}

/// `tellme prompt ...`
#[derive(Debug, Subcommand)]
pub enum PromptCommand {
    /// Manually record a prompt against a committed line.
    Add {
        /// Source file.
        file: PathBuf,
        /// Line or range, e.g. `7` or `4-7`.
        #[arg(long, value_name = "N|N-M")]
        line: String,
        /// Session label to group the prompt under.
        #[arg(long, value_name = "LABEL")]
        session: Option<String>,
        /// Prompt text; if omitted, read from stdin.
        #[arg(short = 'm', long, value_name = "TEXT")]
        message: Option<String>,
    },
    /// List recorded prompts.
    List {
        /// Only prompts touching this file.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },
}

/// `tellme hook ...`
#[derive(Debug, Subcommand)]
pub enum HookCommand {
    /// Install Claude Code + git hooks that capture prompts and edits.
    Install {
        /// Print planned changes without writing them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the hooks installed by `install`.
    Uninstall {
        /// Print planned changes without writing them.
        #[arg(long)]
        dry_run: bool,
    },
}
