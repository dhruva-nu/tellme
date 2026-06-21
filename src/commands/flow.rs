//! `tellme flow <file> --var|--function` — data-flow analysis (#34, #35).

use std::path::Path;

use serde_json::json;

use super::Ctx;
use crate::analysis::{self, FunctionFlow, VarFlow};
use crate::config::OutputFormat;
use crate::error::{Error, Result};

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
        return Err(Error::NotImplemented {
            command: "flow --history",
            phase: "Phase 8: History",
        });
    }

    let src = std::fs::read_to_string(file)
        .map_err(|_| Error::Other(format!("cannot read {}", file.display())))?;
    let analyzer = analysis::for_path(file)?;

    match (var, function) {
        (Some(v), None) => {
            let flow = analyzer.var_flow(&src, v)?;
            match (graph, ctx.format) {
                (true, OutputFormat::Text) => println!("{}", crate::graph::var_graph(&flow)),
                (_, OutputFormat::Json) => print_var_json(&flow),
                (false, OutputFormat::Text) => print_var_text(&flow),
            }
        }
        (None, Some(f)) => {
            let flow = analyzer.function_flow(&src, f)?;
            match (graph, ctx.format) {
                (true, OutputFormat::Text) => println!("{}", crate::graph::function_graph(&flow)),
                (_, OutputFormat::Json) => print_fn_json(&flow),
                (false, OutputFormat::Text) => print_fn_text(&flow),
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
