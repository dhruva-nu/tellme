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

use crate::error::{Error, Result};

/// Convert a path (relative to `cwd` or absolute) into a clean, repo-relative
/// path string for blame and anchor lookup.
///
/// Resolves against `cwd`, then strips `repo_root`. Both are canonicalized so
/// symlinks and `..` don't defeat the prefix match. Errors if the resulting
/// path lies outside the repository.
pub fn repo_relative(repo_root: &Path, cwd: &Path, path: &Path) -> Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let abs = abs
        .canonicalize()
        .map_err(|_| Error::Other(format!("no such file: {}", path.display())))?;
    let root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let rel = abs
        .strip_prefix(&root)
        .map_err(|_| Error::Other(format!("{} is outside the repository", path.display())))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

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
