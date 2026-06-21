//! `tellme journey <file> --endpoint x` — cross-layer data journey (#41).

use std::path::Path;

use serde_json::json;

use super::Ctx;
use crate::config::OutputFormat;
use crate::error::Result;
use crate::git::Repo;
use crate::journey::{journey, Journey};

/// Run `tellme journey`.
pub fn run(ctx: &Ctx, _file: &Path, endpoint: &str) -> Result<()> {
    // The endpoint is resolved project-wide, so the file argument only anchors
    // the repository; discovery walks all Python files under the repo root.
    let root = match Repo::discover(&ctx.start_dir) {
        Ok(repo) => repo.workdir()?,
        Err(_) => ctx.start_dir.clone(),
    };
    let result = journey(&root, endpoint)?;
    match ctx.format {
        OutputFormat::Json => render_json(&result),
        _ if ctx.interactive && !result.stages.is_empty() => browse(&result),
        _ => render_text(&result),
    }
    Ok(())
}

/// Interactive browser for a cross-layer data journey.
fn browse(j: &Journey) {
    use crate::tui::{Item, ItemStyle};
    let mut items: Vec<_> = j
        .stages
        .iter()
        .map(|s| {
            let file = s.file.as_deref().unwrap_or("");
            let label = format!("▾ {} — {}", s.layer.to_uppercase(), s.label);
            let mut detail = format!("{}   {}\n\n{}", s.layer.to_uppercase(), s.label, s.note);
            if !file.is_empty() {
                detail.push_str(&format!("\n\n[{file}]"));
            }
            Item::new(label, detail).styled(ItemStyle::Accent)
        })
        .collect();

    if !j.transformations.is_empty() {
        let detail = j
            .transformations
            .iter()
            .map(|t| format!("• {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        items.push(Item::new("⚙ transformations along the way", detail));
    }

    let browser = crate::tui::Browser::new(format!("journey: {}()", j.endpoint), items);
    if let Err(e) = crate::tui::run(&browser) {
        eprintln!("error: {e}");
    }
}

fn render_text(j: &Journey) {
    println!("Data journey for endpoint: {}()", j.endpoint);
    println!();
    for (i, s) in j.stages.iter().enumerate() {
        let file = s.file.as_deref().unwrap_or("");
        let suffix = if file.is_empty() {
            String::new()
        } else {
            format!("  [{file}]")
        };
        println!("  ┌─ {} ─ {}{}", s.layer.to_uppercase(), s.label, suffix);
        println!("  │    {}", s.note);
        if i + 1 < j.stages.len() {
            println!("  ▼");
        }
    }
    if !j.transformations.is_empty() {
        println!();
        println!("Transformations along the way:");
        for t in &j.transformations {
            println!("  • {t}");
        }
    }
}

fn render_json(j: &Journey) {
    let stages: Vec<_> = j
        .stages
        .iter()
        .map(|s| json!({ "layer": s.layer, "label": s.label, "file": s.file, "note": s.note }))
        .collect();
    println!(
        "{}",
        json!({ "endpoint": j.endpoint, "stages": stages, "transformations": j.transformations })
    );
}
