//! `tellme prompt add` / `tellme prompt list` — the manual capture path (#24).

use std::io::Read;
use std::path::Path;

use serde_json::json;

use super::Ctx;
use crate::config::OutputFormat;
use crate::error::{Error, Result};
use crate::git::Repo;
use crate::lineref::parse_selector;
use crate::paths::{self, Layout};
use crate::store::Store;

/// Open the repo + store for a command, erroring clearly when uninitialized.
fn open(ctx: &Ctx) -> Result<(Repo, Layout, Store)> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let layout = Layout::new(&repo.workdir()?);
    let store = Store::open(&layout)?;
    Ok((repo, layout, store))
}

/// `tellme prompt add <file> --line N[-M] [--session L] [-m TEXT]`.
pub fn add(
    ctx: &Ctx,
    file: &Path,
    line: &str,
    session: Option<&str>,
    message: Option<String>,
) -> Result<()> {
    let (repo, layout, store) = open(ctx)?;
    let (line_start, line_end) = parse_selector(line)?;
    let rel = paths::repo_relative(layout.repo_root(), &ctx.start_dir, file)?;

    // Anchor at the line's current blame commit so query-time blame matches.
    let blamed = repo
        .blame_line(Path::new(&rel), line_start as usize)
        .map_err(|_| {
            Error::Other(format!(
                "{rel}:{line_start} is not committed yet — commit it first, \
                 or let the capture hook record it"
            ))
        })?;

    let text = match message {
        Some(m) => m,
        None => read_stdin()?,
    };
    if text.trim().is_empty() {
        return Err(Error::Other("prompt text is empty".into()));
    }

    let label = session.unwrap_or("manual");
    let sess = store.find_or_create_session(label, Some(label))?;
    let ordinal = store.prompt_count_for_session(sess.id)?;
    let prompt = store.create_prompt(sess.id, ordinal, &text)?;
    let anchor = store.create_anchor(&rel, line_start, line_end, &blamed.commit.id)?;
    store.create_edit(prompt.id, anchor.id)?;

    ctx.emit(
        "recorded",
        &format!("Recorded prompt for {rel}:{line_start}-{line_end} (session \"{label}\")"),
    );
    Ok(())
}

/// `tellme prompt list [--file PATH] [--full]`.
pub fn list(ctx: &Ctx, file: Option<&Path>, full: bool) -> Result<()> {
    let (_repo, layout, store) = open(ctx)?;
    let filter = match file {
        Some(f) => Some(paths::repo_relative(layout.repo_root(), &ctx.start_dir, f)?),
        None => None,
    };

    let prompts = store.recent_prompts(100)?;
    let mut rows = Vec::new();
    for p in prompts {
        let anchors = store.anchors_for_prompt(p.id)?;
        let pending = store.pending_edits_for_prompt(p.id)?;
        if let Some(want) = &filter {
            let hit =
                anchors.iter().any(|a| &a.file == want) || pending.iter().any(|e| &e.file == want);
            if !hit {
                continue;
            }
        }
        // A prompt is "committed" once its edit is anchored; until then it is
        // pending (captured but waiting on a commit), like git status.
        let committed = !anchors.is_empty();
        let location = if committed {
            anchors
                .first()
                .map(|a| format!("{}:{}-{}", a.file, a.line_start, a.line_end))
                .unwrap()
        } else if let Some(e) = pending.first() {
            format!("{}:{}-{}", e.file, e.line_start, e.line_end)
        } else {
            "(no location yet)".into()
        };
        let session = store
            .session_by_id(p.session_id)?
            .and_then(|s| s.label.or(s.external_id))
            .unwrap_or_else(|| "?".into());
        let text = store.read_text(&p.blob_hash).unwrap_or_default();
        rows.push((committed, session, location, text));
    }

    match ctx.format {
        OutputFormat::Json => {
            // JSON always carries the complete prompt text.
            let arr: Vec<_> = rows
                .iter()
                .map(|(committed, s, l, t)| {
                    json!({
                        "status": if *committed { "committed" } else { "pending" },
                        "session": s,
                        "location": l,
                        "text": t,
                    })
                })
                .collect();
            println!("{}", serde_json::Value::Array(arr));
        }
        _ => render_status_list(&rows, full),
    }
    Ok(())
}

/// Render the prompt list git-status-style: green = committed, red = pending.
/// With `full`, print the complete (multi-line) prompt text.
fn render_status_list(rows: &[(bool, String, String, String)], full: bool) {
    if rows.is_empty() {
        println!("No prompts recorded yet.");
        return;
    }
    let (committed, pending): (Vec<_>, Vec<_>) = rows.iter().partition(|(c, ..)| *c);

    let render = |marker: &str, s: &str, l: &str, t: &str, code: &str| {
        if full {
            println!("{}", paint(&format!("{marker} {l}  [{s}]"), code));
            for line in t.lines() {
                println!("      {line}");
            }
            println!();
        } else {
            println!(
                "{}",
                paint(&format!("{marker} {l}  [{s}]  {}", snippet(t)), code)
            );
        }
    };

    if !committed.is_empty() {
        println!("Committed prompts:");
        for (_, s, l, t) in &committed {
            render("  ●", s, l, t, GREEN);
        }
    }
    if !pending.is_empty() {
        if !committed.is_empty() {
            println!();
        }
        println!("Pending prompts (not yet committed — commit to anchor):");
        for (_, s, l, t) in &pending {
            render("  ○", s, l, t, RED);
        }
    }
    println!();
    println!("{} committed, {} pending", committed.len(), pending.len());
}

const RED: &str = "31";
const GREEN: &str = "32";

/// Wrap `s` in an ANSI color when stdout is a color-capable terminal.
fn paint(s: &str, code: &str) -> String {
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// First line of `text`, truncated for a one-line listing.
fn snippet(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    if first.chars().count() > 80 {
        let mut s: String = first.chars().take(77).collect();
        s.push('…');
        s
    } else {
        first.to_string()
    }
}

fn read_stdin() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_truncates_long_first_line() {
        let s = snippet(&"x".repeat(200));
        assert!(s.chars().count() <= 78);
        assert!(s.ends_with('…'));
    }
}
