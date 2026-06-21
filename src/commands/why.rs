//! `tellme why <file> #lineN` — render a line's prompt history (#31).

use std::path::Path;

use serde_json::json;

use super::Ctx;
use crate::blame::{why, Entry, WhyResult};
use crate::config::OutputFormat;
use crate::error::Result;
use crate::git::Repo;
use crate::lineref::parse_selector;
use crate::paths::{self, Layout};
use crate::store::Store;

/// Run `tellme why`.
pub fn run(ctx: &Ctx, file: &Path, target: Option<&str>) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let store = Store::open(&Layout::new(&root))?;

    let selector = target.unwrap_or("1");
    let (line_start, line_end) = parse_selector(selector)?;
    let rel = paths::repo_relative(&root, &ctx.start_dir, file)?;

    let result = why(&store, &repo, &root, &rel, line_start, line_end)?;
    match ctx.format {
        OutputFormat::Json => render_json(&result),
        _ => render_text(&result),
    }
    Ok(())
}

fn render_text(r: &WhyResult) {
    let loc = if r.line_start == r.line_end {
        format!("{}:{}", r.file, r.line_start)
    } else {
        format!("{}:{}-{}", r.file, r.line_start, r.line_end)
    };
    match &r.code {
        Some(code) => println!("{loc}  {code}"),
        None => println!("{loc}"),
    }
    println!();

    if !r.committed {
        println!("  This line isn't committed yet — no prompt history to show.");
        return;
    }
    if r.entries.is_empty() {
        println!("  No recorded prompts for this line.");
        println!("  (record one with `tellme prompt add` or install capture hooks)");
        return;
    }

    println!("Prompt history for this line:");
    println!();
    for entry in &r.entries {
        render_entry(entry);
    }
}

fn render_entry(e: &Entry) {
    let short = &e.commit_id[..e.commit_id.len().min(8)];
    match &e.prompt {
        Some(prompt) => {
            let session = e.session.as_deref().unwrap_or("(unknown session)");
            println!("  ● {short}  {}  — session \"{session}\"", e.commit_summary);
            println!("    YOU: {prompt}");
        }
        None => {
            println!("  ● {short}  {}  — decision only", e.commit_summary);
        }
    }
    for d in &e.decisions {
        println!("    ▸ decision: {d}");
    }
    println!();
}

fn render_json(r: &WhyResult) {
    let entries: Vec<_> = r
        .entries
        .iter()
        .map(|e| {
            json!({
                "commit": e.commit_id,
                "summary": e.commit_summary,
                "author": e.author,
                "time": e.time,
                "session": e.session,
                "prompt": e.prompt,
                "decisions": e.decisions,
            })
        })
        .collect();
    let obj = json!({
        "file": r.file,
        "line_start": r.line_start,
        "line_end": r.line_end,
        "code": r.code,
        "committed": r.committed,
        "entries": entries,
    });
    println!("{obj}");
}
