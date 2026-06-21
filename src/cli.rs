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
    /// Manually record a prompt.
    Add {
        /// Prompt text; if omitted, read from stdin.
        text: Option<String>,
    },
    /// List recorded prompts.
    List,
}
