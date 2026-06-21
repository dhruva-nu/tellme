//! ASCII flow-graph rendering over a petgraph model (#37, #38).
//!
//! `flow --graph` visualizes the structures from [`crate::analysis`]:
//! a function as `callers → function → callees` (EXAMPLES.md §4), and a
//! variable as a vertical lifecycle chain.

use petgraph::dot::{Config, Dot};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;

use crate::analysis::{FunctionFlow, VarFlow};

/// Center `s` within `width` columns.
fn center(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let left = (width - len) / 2;
    let right = width - len - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

/// A boxed label: `┌──┐ / │ x │ / └──┘`, sized to the widest line.
fn boxed(lines: &[&str]) -> Vec<String> {
    let inner = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) + 2;
    let mut out = vec![format!("┌{}┐", "─".repeat(inner))];
    for l in lines {
        out.push(format!("│ {} │", center(l, inner - 2)));
    }
    out.push(format!("└{}┘", "─".repeat(inner)));
    out
}

/// Width of the widest line in a block.
fn block_width(lines: &[String]) -> usize {
    lines.iter().map(|l| l.chars().count()).max().unwrap_or(0)
}

/// Render a block of lines centered to `width` into `out`.
fn push_centered(out: &mut Vec<String>, lines: &[String], width: usize) {
    for l in lines {
        out.push(center(l, width));
    }
}

/// Render the function graph: callers above, the function box, callees below.
pub fn function_graph(f: &FunctionFlow) -> String {
    // Build the petgraph model: callers → fn → callees.
    let mut g: DiGraph<String, &str> = DiGraph::new();
    let sig = format!(
        "({}) → {}",
        f.params.join(", "),
        f.returns.as_deref().unwrap_or("?")
    );
    let fn_node = g.add_node(format!("{}\n{}", f.name, sig));
    for c in &f.callers {
        let n = g.add_node(c.name.clone());
        g.add_edge(n, fn_node, "calls");
    }
    for c in &f.callees {
        let n = g.add_node(c.name.clone());
        g.add_edge(fn_node, n, "uses");
    }

    // Read the bands back out of the graph.
    let callers: Vec<String> = neighbors(&g, fn_node, Direction::Incoming);
    let callees: Vec<String> = neighbors(&g, fn_node, Direction::Outgoing);

    let box_lines = boxed(&[&f.name, &sig]);
    let callers_row = row(&callers, "(no callers)");
    let callees_row = row(&callees, "(no callees)");

    let width = [
        block_width(&box_lines),
        callers_row.chars().count(),
        callees_row.chars().count(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    let mut out = Vec::new();
    out.push(center(&callers_row, width));
    out.push(center("│", width));
    push_centered(&mut out, &box_lines, width);
    out.push(center("│ uses", width));
    out.push(center(&callees_row, width));
    out.join("\n")
}

/// Render a variable's lifecycle as a vertical chain of small boxes.
pub fn var_graph(v: &VarFlow) -> String {
    let scope = v
        .function
        .as_deref()
        .map(|f| format!("  (in {f})"))
        .unwrap_or_default();
    let mut out = vec![format!("Variable: {}{}", v.name, scope), String::new()];
    let width = v
        .events
        .iter()
        .map(|e| e.kind.label().len() + e.text.len() + 12)
        .max()
        .unwrap_or(20);
    for (i, e) in v.events.iter().enumerate() {
        let label = format!("{} (line {})", e.kind.label(), e.line);
        for l in boxed(&[&label, &e.text]) {
            out.push(center(&l, width));
        }
        if i + 1 < v.events.len() {
            out.push(center("│", width));
        }
    }
    out.join("\n")
}

/// Graphviz DOT for the function graph (pipe to `dot -Tsvg`).
pub fn function_dot(f: &FunctionFlow) -> String {
    let mut g: DiGraph<String, &str> = DiGraph::new();
    let sig = format!(
        "{}({}) -> {}",
        f.name,
        f.params.join(", "),
        f.returns.as_deref().unwrap_or("?")
    );
    let fnode = g.add_node(sig);
    for c in &f.callers {
        let n = g.add_node(c.name.clone());
        g.add_edge(n, fnode, "calls");
    }
    for c in &f.callees {
        let n = g.add_node(c.name.clone());
        g.add_edge(fnode, n, "uses");
    }
    format!("{}", Dot::with_config(&g, &[Config::EdgeNoLabel]))
}

/// Graphviz DOT for a variable's lifecycle chain.
pub fn var_dot(v: &VarFlow) -> String {
    let mut g: DiGraph<String, &str> = DiGraph::new();
    let mut prev: Option<NodeIndex> = None;
    for e in &v.events {
        let n = g.add_node(format!("{} (line {})", e.kind.label(), e.line));
        if let Some(p) = prev {
            g.add_edge(p, n, "");
        }
        prev = Some(n);
    }
    format!("{}", Dot::with_config(&g, &[Config::EdgeNoLabel]))
}

fn neighbors(g: &DiGraph<String, &str>, node: NodeIndex, dir: Direction) -> Vec<String> {
    g.neighbors_directed(node, dir)
        .map(|n| g[n].clone())
        .collect()
}

/// A horizontal row of names joined by spaces, or a placeholder if empty.
fn row(names: &[String], empty: &str) -> String {
    if names.is_empty() {
        empty.to_string()
    } else {
        names.join("        ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Call, VarEvent, VarEventKind};

    fn sample_fn() -> FunctionFlow {
        FunctionFlow {
            name: "calculate_total".into(),
            line: 1,
            params: vec!["cart".into(), "user".into()],
            returns: None,
            callers: vec![Call {
                name: "checkout".into(),
                line: 10,
            }],
            callees: vec![
                Call {
                    name: "sum".into(),
                    line: 2,
                },
                Call {
                    name: "round".into(),
                    line: 7,
                },
            ],
        }
    }

    #[test]
    fn function_graph_contains_node_and_neighbors() {
        let g = function_graph(&sample_fn());
        assert!(g.contains("calculate_total"));
        assert!(g.contains("checkout"));
        assert!(g.contains("sum"));
        assert!(g.contains("round"));
        assert!(g.contains("uses"));
    }

    #[test]
    fn var_graph_lists_events_in_order() {
        let v = VarFlow {
            name: "total".into(),
            function: Some("calculate_total".into()),
            events: vec![
                VarEvent {
                    kind: VarEventKind::Initialized,
                    line: 4,
                    text: "total = a".into(),
                },
                VarEvent {
                    kind: VarEventKind::Used,
                    line: 7,
                    text: "return total".into(),
                },
            ],
        };
        let out = var_graph(&v);
        let init = out.find("INITIALIZED").unwrap();
        let used = out.find("USED").unwrap();
        assert!(init < used);
        assert!(out.contains("total"));
    }
}
