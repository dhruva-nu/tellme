//! `tellme flow <file> --var|--function` — data-flow analysis (#34, #35).

use std::path::Path;

use serde_json::json;

use super::Ctx;
use crate::analysis::{self, FunctionFlow, VarFlow};
use crate::blame::{self, WhyResult};
use crate::config::OutputFormat;
use crate::error::{Error, Result};
use crate::git::Repo;
use crate::paths::{self, Layout};
use crate::store::Store;

/// Run `tellme flow`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    ctx: &Ctx,
    file: &Path,
    var: Option<&str>,
    function: Option<&str>,
    graph: bool,
    history: bool,
) -> Result<()> {
    if history {
        return run_history(ctx, file, var, function);
    }

    let src = std::fs::read_to_string(file)
        .map_err(|_| Error::Other(format!("cannot read {}", file.display())))?;
    let analyzer = analysis::for_path(file)?;

    match (var, function) {
        (Some(v), None) => {
            let flow = analyzer.var_flow(&src, v)?;
            match (graph, ctx.format) {
                (_, OutputFormat::Json) => print_var_json(&flow),
                (true, OutputFormat::Dot) => println!("{}", crate::graph::var_dot(&flow)),
                (true, _) => println!("{}", crate::graph::var_graph(&flow)),
                (false, _) => print_var_text(&flow),
            }
        }
        (None, Some(f)) => {
            let flow = analyzer.function_flow(&src, f)?;
            match (graph, ctx.format) {
                (_, OutputFormat::Json) => print_fn_json(&flow),
                (true, OutputFormat::Dot) => println!("{}", crate::graph::function_dot(&flow)),
                (true, _) => println!("{}", crate::graph::function_graph(&flow)),
                (false, _) => print_fn_text(&flow),
            }
        }
        (Some(_), Some(_)) => {
            return Err(Error::Other("pass only one of --var or --function".into()))
        }
        (None, None) => {
            return Err(Error::Other(
                "pass --var <name> or --function <name>".into(),
            ))
        }
    }
    Ok(())
}

fn print_var_text(flow: &VarFlow) {
    let scope = flow
        .function
        .as_deref()
        .map(|f| format!("function {f}"))
        .unwrap_or_else(|| "module scope".into());
    println!("Variable: {}   ({scope})", flow.name);
    println!();
    for e in &flow.events {
        println!("  ▸ {:<13} line {:<4} {}", e.kind.label(), e.line, e.text);
    }
}

fn print_var_json(flow: &VarFlow) {
    let events: Vec<_> = flow
        .events
        .iter()
        .map(|e| json!({ "kind": e.kind.label(), "line": e.line, "text": e.text }))
        .collect();
    println!(
        "{}",
        json!({ "variable": flow.name, "function": flow.function, "events": events })
    );
}

fn print_fn_text(f: &FunctionFlow) {
    println!("Function: {}   (line {})", f.name, f.line);
    println!();
    println!("  CALLED BY");
    if f.callers.is_empty() {
        println!("    (no callers in this file)");
    } else {
        for c in &f.callers {
            println!("    • {} (line {})", c.name, c.line);
        }
    }
    println!();
    println!("  CALLS");
    if f.callees.is_empty() {
        println!("    (none)");
    } else {
        for c in &f.callees {
            println!("    • {} (line {})", c.name, c.line);
        }
    }
    println!();
    println!("  SIGNATURE");
    println!("    in:  {}", f.params.join(", "));
    println!(
        "    out: {}",
        f.returns.as_deref().unwrap_or("(unannotated)")
    );
}

fn print_fn_json(f: &FunctionFlow) {
    let calls = |v: &[crate::analysis::Call]| {
        v.iter()
            .map(|c| json!({ "name": c.name, "line": c.line }))
            .collect::<Vec<_>>()
    };
    println!(
        "{}",
        json!({
            "function": f.name,
            "line": f.line,
            "params": f.params,
            "returns": f.returns,
            "callers": calls(&f.callers),
            "callees": calls(&f.callees),
        })
    );
}

/// `flow … --history`: the code+prompt+decision timeline for a symbol's lines.
fn run_history(ctx: &Ctx, file: &Path, var: Option<&str>, function: Option<&str>) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let store = Store::open(&Layout::new(&root))?;
    let rel = paths::repo_relative(&root, &ctx.start_dir, file)?;
    let src = std::fs::read_to_string(root.join(&rel))
        .map_err(|_| Error::Other(format!("cannot read {rel}")))?;
    let analyzer = analysis::for_path(file)?;

    let (label, start, end) = match (var, function) {
        (Some(v), None) => {
            let flow = analyzer.var_flow(&src, v)?;
            let min = flow.events.iter().map(|e| e.line).min().unwrap_or(1) as i64;
            let max = flow
                .events
                .iter()
                .map(|e| e.line)
                .max()
                .unwrap_or(min as usize) as i64;
            (format!("variable {v}"), min, max)
        }
        (None, Some(f)) => {
            let flow = analyzer.function_flow(&src, f)?;
            (format!("function {f}"), flow.line as i64, flow.line as i64)
        }
        (Some(_), Some(_)) => {
            return Err(Error::Other("pass only one of --var or --function".into()))
        }
        (None, None) => {
            return Err(Error::Other(
                "pass --var <name> or --function <name>".into(),
            ))
        }
    };

    let result = blame::why(&store, &repo, &root, &rel, start, end)?;
    match ctx.format {
        OutputFormat::Json => print_history_json(&label, &rel, &result),
        _ => print_history_text(&label, &rel, &result),
    }
    Ok(())
}

fn print_history_text(label: &str, rel: &str, r: &WhyResult) {
    println!("History: {label}   ({rel})");
    println!();
    if !r.committed {
        println!("  These lines aren't committed yet — no history.");
        return;
    }
    if r.entries.is_empty() {
        println!("  No recorded prompts or decisions for these lines.");
        return;
    }
    let last = r.entries.len() - 1;
    for (i, e) in r.entries.iter().enumerate() {
        let marker = if i == 0 {
            "CREATED      "
        } else if i == last {
            "LAST MODIFIED"
        } else {
            "MODIFIED     "
        };
        let short = &e.commit_id[..e.commit_id.len().min(8)];
        let session = e.session.as_deref().unwrap_or("-");
        println!("  ● {marker}  {short}  {}  [{session}]", e.commit_summary);
        if let Some(p) = &e.prompt {
            println!("    PROMPT:   {p}");
        }
        for d in &e.decisions {
            println!("    DECISION: {d}");
        }
    }
}

fn print_history_json(label: &str, rel: &str, r: &WhyResult) {
    let entries: Vec<_> = r
        .entries
        .iter()
        .map(|e| {
            json!({
                "commit": e.commit_id,
                "summary": e.commit_summary,
                "time": e.time,
                "session": e.session,
                "prompt": e.prompt,
                "decisions": e.decisions,
            })
        })
        .collect();
    println!(
        "{}",
        json!({ "subject": label, "file": rel, "committed": r.committed, "history": entries })
    );
}
