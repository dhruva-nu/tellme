//! Python source analysis via tree-sitter (#34, #35).

use std::collections::HashSet;

use tree_sitter::{Node, Parser, Tree};

use super::{Analyzer, Call, FunctionFlow, VarEvent, VarEventKind, VarFlow};
use crate::error::{Error, Result};

/// A tree-sitter-backed Python analyzer.
pub struct Python;

impl Python {
    /// Construct an analyzer.
    pub fn new() -> Result<Self> {
        Ok(Python)
    }

    fn parse(&self, src: &str) -> Result<Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::language())
            .map_err(|e| Error::Other(format!("tree-sitter init failed: {e}")))?;
        parser
            .parse(src, None)
            .ok_or_else(|| Error::Other("failed to parse source".into()))
    }
}

fn node_text<'a>(n: Node, src: &'a str) -> &'a str {
    n.utf8_text(src.as_bytes()).unwrap_or("")
}

fn line_of(n: Node) -> usize {
    n.start_position().row + 1
}

fn source_line(src: &str, line: usize) -> String {
    src.lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Visit every node depth-first.
fn walk(node: Node, f: &mut dyn FnMut(Node)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, f);
    }
}

/// Find the `function_definition` named `name`, if any (lifetime-preserving).
fn find_function<'t>(node: Node<'t>, name: &str, src: &str) -> Option<Node<'t>> {
    if node.kind() == "function_definition"
        && node
            .child_by_field_name("name")
            .map(|id| node_text(id, src) == name)
            .unwrap_or(false)
    {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function(child, name, src) {
            return Some(found);
        }
    }
    None
}

/// The name of the function enclosing `node`, if any.
fn enclosing_function(node: Node, src: &str) -> Option<String> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "function_definition" {
            return n
                .child_by_field_name("name")
                .map(|id| node_text(id, src).to_string());
        }
        cur = n.parent();
    }
    None
}

/// The callee name for a `call` node (identifier or dotted attribute).
fn call_name(call: Node, src: &str) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" | "attribute" => Some(node_text(func, src).to_string()),
        _ => None,
    }
}

impl Analyzer for Python {
    fn var_flow(&self, src: &str, var: &str) -> Result<VarFlow> {
        let tree = self.parse(src)?;
        let root = tree.root_node();

        let mut assign_targets: Vec<(usize, usize)> = Vec::new(); // (line, node_id)
        let mut augmented: Vec<usize> = Vec::new(); // lines
        let mut target_ids: HashSet<usize> = HashSet::new();
        let mut occurrences: Vec<(usize, usize)> = Vec::new(); // (line, node_id)
        let mut function: Option<String> = None;

        walk(root, &mut |n| match n.kind() {
            "assignment" | "augmented_assignment" => {
                let target = n
                    .child_by_field_name("left")
                    .filter(|l| l.kind() == "identifier" && node_text(*l, src) == var);
                if let Some(left) = target {
                    target_ids.insert(left.id());
                    if n.kind() == "assignment" {
                        assign_targets.push((line_of(left), left.id()));
                    } else {
                        augmented.push(line_of(left));
                    }
                }
            }
            "identifier" if node_text(n, src) == var => {
                occurrences.push((line_of(n), n.id()));
                if function.is_none() {
                    function = enclosing_function(n, src);
                }
            }
            _ => {}
        });

        if occurrences.is_empty() {
            return Err(Error::Other(format!(
                "variable `{var}` not found in this file"
            )));
        }

        let mut events: Vec<VarEvent> = Vec::new();
        assign_targets.sort_by_key(|(line, _)| *line);
        for (i, (line, _)) in assign_targets.iter().enumerate() {
            let kind = if i == 0 {
                VarEventKind::Initialized
            } else {
                VarEventKind::Modified
            };
            events.push(VarEvent {
                kind,
                line: *line,
                text: source_line(src, *line),
            });
        }
        for line in &augmented {
            events.push(VarEvent {
                kind: VarEventKind::Modified,
                line: *line,
                text: source_line(src, *line),
            });
        }
        // Reads: occurrences that aren't assignment targets, one per line.
        let mut used_lines: Vec<usize> = occurrences
            .iter()
            .filter(|(_, id)| !target_ids.contains(id))
            .map(|(line, _)| *line)
            .collect();
        used_lines.sort_unstable();
        used_lines.dedup();
        for line in used_lines {
            events.push(VarEvent {
                kind: VarEventKind::Used,
                line,
                text: source_line(src, line),
            });
        }

        events.sort_by_key(|e| e.line);
        // Lifecycle end at the last referenced line.
        if let Some(last) = events.iter().map(|e| e.line).max() {
            events.push(VarEvent {
                kind: VarEventKind::LifecycleEnd,
                line: last,
                text: source_line(src, last),
            });
        }

        Ok(VarFlow {
            name: var.to_string(),
            function,
            events,
        })
    }

    fn function_flow(&self, src: &str, func: &str) -> Result<FunctionFlow> {
        let tree = self.parse(src)?;
        let root = tree.root_node();
        let def = find_function(root, func, src)
            .ok_or_else(|| Error::Other(format!("function `{func}` not found in this file")))?;

        // Parameters (text as written) and return annotation.
        let mut params = Vec::new();
        if let Some(p) = def.child_by_field_name("parameters") {
            let mut cursor = p.walk();
            for child in p.named_children(&mut cursor) {
                params.push(node_text(child, src).to_string());
            }
        }
        let returns = def
            .child_by_field_name("return_type")
            .map(|t| node_text(t, src).to_string());

        // Callees: calls within the body.
        let mut callees = Vec::new();
        if let Some(body) = def.child_by_field_name("body") {
            let mut seen = HashSet::new();
            walk(body, &mut |n| {
                if n.kind() == "call" {
                    if let Some(name) = call_name(n, src) {
                        let line = line_of(n);
                        if seen.insert((name.clone(), line)) {
                            callees.push(Call { name, line });
                        }
                    }
                }
            });
        }

        // Callers: call sites of `func` elsewhere in the file.
        let mut callers = Vec::new();
        walk(root, &mut |n| {
            if n.kind() == "call" {
                if let Some(name) = call_name(n, src) {
                    let short = name.rsplit('.').next().unwrap_or(&name);
                    if short == func {
                        callers.push(Call {
                            name: enclosing_function(n, src).unwrap_or_else(|| "<module>".into()),
                            line: line_of(n),
                        });
                    }
                }
            }
        });

        Ok(FunctionFlow {
            name: func.to_string(),
            line: line_of(def),
            params,
            returns,
            callers,
            callees,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
def calculate_total(cart, user):
    subtotal = sum(item.price for item in cart.items)
    tax = subtotal * 0.08
    total = subtotal + tax
    if user.is_premium:
        total = total * 0.9
    return round(total, 2)

def checkout():
    return calculate_total(cart, user)
";

    #[test]
    fn var_flow_tracks_total() {
        let py = Python::new().unwrap();
        let flow = py.var_flow(SRC, "total").unwrap();
        assert_eq!(flow.function.as_deref(), Some("calculate_total"));
        let kinds: Vec<_> = flow.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds.first(), Some(&VarEventKind::Initialized));
        assert!(kinds.contains(&VarEventKind::Modified));
        assert!(kinds.contains(&VarEventKind::Used));
        assert_eq!(kinds.last(), Some(&VarEventKind::LifecycleEnd));
        // Initialized on line 4.
        assert_eq!(flow.events[0].line, 4);
    }

    #[test]
    fn function_flow_signature_callers_callees() {
        let py = Python::new().unwrap();
        let f = py.function_flow(SRC, "calculate_total").unwrap();
        assert_eq!(f.line, 1);
        assert_eq!(f.params, vec!["cart", "user"]);
        // Callees include round() and sum().
        let callee_names: Vec<_> = f.callees.iter().map(|c| c.name.as_str()).collect();
        assert!(callee_names.contains(&"round"));
        assert!(callee_names.contains(&"sum"));
        // Called by checkout().
        assert_eq!(f.callers.len(), 1);
        assert_eq!(f.callers[0].name, "checkout");
    }

    #[test]
    fn missing_symbols_error() {
        let py = Python::new().unwrap();
        assert!(py.var_flow(SRC, "nope").is_err());
        assert!(py.function_flow(SRC, "nope").is_err());
    }
}
