//! Crate-wide error type and `Result` alias (#16).
//!
//! Internal code returns [`Result`]; the binary boundary in `main.rs` maps an
//! [`Error`] to a user-readable message and a process exit code.

use std::path::PathBuf;
use thiserror::Error;

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong inside `tellme`.
#[derive(Debug, Error)]
pub enum Error {
    /// No git repository was found at or above the start path.
    #[error("not inside a git repository (searched from {0})")]
    NotAGitRepo(PathBuf),

    /// The `.tellme/` store does not exist yet.
    #[error("tellme is not initialized here — run `tellme init` first (looked in {0})")]
    StoreNotFound(PathBuf),

    /// A feature whose command exists but is not implemented yet.
    #[error("`{command}` is not implemented yet (planned for {phase})")]
    NotImplemented {
        /// The user-facing command name.
        command: &'static str,
        /// The roadmap phase that delivers it.
        phase: &'static str,
    },

    /// Bad configuration on disk or invalid value.
    #[error("config error: {0}")]
    Config(String),

    /// Wraps a libgit2 error.
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    /// Wraps a SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Wraps a filesystem error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// Catch-all for contextual messages.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Process exit code for this error.
    ///
    /// `2` mirrors clap's usage-error convention; everything else is `1`.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotImplemented { .. } => 2,
            _ => 1,
        }
    }
}
