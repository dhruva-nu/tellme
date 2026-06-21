//! Filesystem layout of the committed `.tellme/` sidecar store.
//!
//! ```text
//! <repo>/.tellme/
//!   ├── index.db        SQLite index           (committed)
//!   ├── blobs/          content-addressed text  (committed)
//!   ├── config.json     local configuration     (committed)
//!   └── cache/          derived/scratch data     (gitignored)
//! ```

use std::path::{Path, PathBuf};

/// Resolved paths for a repository's `.tellme/` store.
#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
    tellme: PathBuf,
}

impl Layout {
    /// Build a layout rooted at a repository working directory.
    pub fn new(repo_root: &Path) -> Self {
        let root = repo_root.to_path_buf();
        let tellme = root.join(".tellme");
        Layout { root, tellme }
    }

    /// The repository working directory.
    pub fn repo_root(&self) -> &Path {
        &self.root
    }

    /// The `.tellme/` directory itself.
    pub fn tellme_dir(&self) -> &Path {
        &self.tellme
    }

    /// SQLite index database (committed).
    pub fn index_db(&self) -> PathBuf {
        self.tellme.join("index.db")
    }

    /// Content-addressed blob directory (committed).
    pub fn blobs_dir(&self) -> PathBuf {
        self.tellme.join("blobs")
    }

    /// Configuration file (committed).
    pub fn config_path(&self) -> PathBuf {
        self.tellme.join("config.json")
    }

    /// Derived/scratch cache (gitignored).
    pub fn cache_dir(&self) -> PathBuf {
        self.tellme.join("cache")
    }

    /// The `.gitignore` path at the repo root.
    pub fn gitignore_path(&self) -> PathBuf {
        self.root.join(".gitignore")
    }

    /// Whether the store has already been initialized (index db present).
    pub fn is_initialized(&self) -> bool {
        self.index_db().exists()
    }
}
