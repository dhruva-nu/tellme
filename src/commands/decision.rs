//! `tellme decision add` — attach a written "why" to a line/variable (#43).

use std::path::Path;
use std::process::Command;

use super::Ctx;
use crate::analysis;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git::Repo;
use crate::paths::{self, Layout};
use crate::store::{Anchor, Store};

/// Run `tellme decision add`.
pub fn add(
    ctx: &Ctx,
    file: &Path,
    var: Option<&str>,
    line: Option<usize>,
    message: Option<String>,
) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let layout = Layout::new(&root);
    let store = Store::open(&layout)?;
    let rel = paths::repo_relative(&root, &ctx.start_dir, file)?;

    // Resolve the target line: explicit --line, else the variable's first line.
    let line = match (line, var) {
        (Some(l), _) => l,
        (None, Some(v)) => var_line(&root, &rel, v)?,
        (None, None) => return Err(Error::Other("pass --line <N> or --var <name>".into())),
    };

    let blamed = repo.blame_line(Path::new(&rel), line).map_err(|_| {
        Error::Other(format!(
            "{rel}:{line} is not committed yet — commit it before recording a decision"
        ))
    })?;
    let anchor = resolve_anchor(&store, &rel, line as i64, &blamed.commit.id)?;

    let text = match message {
        Some(m) => m,
        None => open_editor(&layout, &rel, line)?,
    };
    if text.trim().is_empty() {
        return Err(Error::Other("aborted: empty decision".into()));
    }

    let author = repo.user_name();
    let prompt_id = store.prompts_for_anchor(anchor.id)?.first().map(|p| p.id);
    store.create_decision(anchor.id, text.trim(), author.as_deref(), prompt_id)?;

    ctx.emit(
        "saved",
        &format!("Decision saved and attached to {rel}:{line}"),
    );
    Ok(())
}

/// Reuse an anchor at the same file+commit overlapping `line`, else create one.
fn resolve_anchor(store: &Store, file: &str, line: i64, commit: &str) -> Result<Anchor> {
    if let Some(found) = store
        .anchors_for_file(file)?
        .into_iter()
        .find(|a| a.commit_id == commit && a.line_start <= line && a.line_end >= line)
    {
        return Ok(found);
    }
    store.create_anchor(file, line, line, commit)
}

/// First line of a variable's lifecycle, via the analyzer.
fn var_line(root: &Path, rel: &str, var: &str) -> Result<usize> {
    let src = std::fs::read_to_string(root.join(rel))
        .map_err(|_| Error::Other(format!("cannot read {rel}")))?;
    let analyzer = analysis::for_path(Path::new(rel))?;
    let flow = analyzer.var_flow(&src, var)?;
    flow.events
        .first()
        .map(|e| e.line)
        .ok_or_else(|| Error::Other(format!("no occurrences of `{var}`")))
}

/// Open `$EDITOR` on a template and return the entered decision text.
fn open_editor(layout: &Layout, rel: &str, line: usize) -> Result<String> {
    let editor = Config::load(&layout.config_path())?
        .editor
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vi".to_string());

    std::fs::create_dir_all(layout.cache_dir())?;
    let path = layout.cache_dir().join("DECISION_EDIT.md");
    std::fs::write(
        &path,
        format!("\n# Write the decision (why) for {rel}:{line}.\n# Lines starting with # are ignored; an empty file aborts.\n"),
    )?;

    // Support editors with args, e.g. `code --wait`.
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = Command::new(program)
        .args(parts)
        .arg(&path)
        .status()
        .map_err(|e| Error::Other(format!("failed to launch editor `{editor}`: {e}")))?;
    if !status.success() {
        return Err(Error::Other("editor exited with an error".into()));
    }

    let raw = std::fs::read_to_string(&path)?;
    Ok(raw
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Layout;

    #[test]
    fn resolve_anchor_creates_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::create(&Layout::new(dir.path())).unwrap();
        let a = resolve_anchor(&store, "checkout.py", 7, "abc").unwrap();
        let b = resolve_anchor(&store, "checkout.py", 7, "abc").unwrap();
        assert_eq!(a.id, b.id);
        // A different commit makes a new anchor.
        let c = resolve_anchor(&store, "checkout.py", 7, "def").unwrap();
        assert_ne!(a.id, c.id);
    }
}
