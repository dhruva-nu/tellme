//! `tellme init` — create the `.tellme/` store end-to-end (#15).

use std::fs;
use std::io::Write;

use crate::config::Config;
use crate::error::Result;
use crate::git::Repo;
use crate::paths::Layout;
use crate::store::Store;

use super::Ctx;

/// Run `tellme init`.
///
/// Idempotent: re-running on an initialized repo prints a notice and succeeds.
/// Errors clearly when not inside a git repository.
pub fn run(ctx: &Ctx) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let workdir = repo.workdir()?;
    let layout = Layout::new(&workdir);

    if layout.is_initialized() {
        ctx.emit(
            "already-initialized",
            &format!(
                "tellme is already initialized at {}",
                layout.tellme_dir().display()
            ),
        );
        return Ok(());
    }

    Store::create(&layout)?;
    Config::default().save(&layout.config_path())?;
    ensure_gitignore(&layout)?;

    ctx.emit(
        "initialized",
        &format!(
            "Initialized tellme store at {}",
            layout.tellme_dir().display()
        ),
    );
    Ok(())
}

/// Ensure `.tellme/cache` is gitignored while the index/blobs stay committed.
fn ensure_gitignore(layout: &Layout) -> Result<()> {
    let entry = ".tellme/cache/";
    let path = layout.gitignore_path();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing
        .lines()
        .any(|l| l.trim() == entry.trim_end_matches('/') || l.trim() == entry)
    {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{entry}")?;
    Ok(())
}
