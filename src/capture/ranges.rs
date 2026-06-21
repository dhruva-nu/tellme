//! Extract the line range(s) a tool event touched (#26).
//!
//! Given a tool name, its `tool_input`, and the file content *after* the edit,
//! return the one-based inclusive line spans that changed. When an exact span
//! can't be determined we fall back to a file-level range, so capture never
//! silently drops an edit.

use serde_json::Value;

/// A one-based inclusive line span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// First line (inclusive).
    pub start: usize,
    /// Last line (inclusive).
    pub end: usize,
}

impl LineRange {
    fn new(start: usize, end: usize) -> Self {
        LineRange { start, end }
    }
}

/// Number of lines in `s` (at least 1 for non-empty content).
fn line_count(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.lines().count().max(1)
    }
}

/// The whole-file range for `content` (1..=lines), or `None` if empty.
fn whole_file(content: &str) -> Option<LineRange> {
    match line_count(content) {
        0 => None,
        n => Some(LineRange::new(1, n)),
    }
}

/// Locate `needle` in `content` and return the line span it occupies.
fn span_of(content: &str, needle: &str) -> Option<LineRange> {
    if needle.is_empty() {
        return None;
    }
    let idx = content.find(needle)?;
    let start = content[..idx].bytes().filter(|&b| b == b'\n').count() + 1;
    // Lines added by the needle itself (a single-line needle spans one line).
    let extra = needle.bytes().filter(|&b| b == b'\n').count();
    Some(LineRange::new(start, start + extra))
}

/// Extract line ranges for a `PostToolUse` event.
///
/// `content` is the file's content after the edit (read from disk by the
/// caller). Returns an empty vec for tools that don't touch a file.
pub fn edit_ranges(tool_name: &str, tool_input: &Value, content: &str) -> Vec<LineRange> {
    match tool_name {
        "Write" => whole_file(content).into_iter().collect(),
        "Edit" => {
            let new_string = tool_input.get("new_string").and_then(Value::as_str);
            match new_string.and_then(|n| span_of(content, n)) {
                Some(r) => vec![r],
                None => whole_file(content).into_iter().collect(),
            }
        }
        "MultiEdit" => {
            let edits = tool_input.get("edits").and_then(Value::as_array);
            let mut out: Vec<LineRange> = Vec::new();
            if let Some(edits) = edits {
                for e in edits {
                    if let Some(r) = e
                        .get("new_string")
                        .and_then(Value::as_str)
                        .and_then(|n| span_of(content, n))
                    {
                        if !out.contains(&r) {
                            out.push(r);
                        }
                    }
                }
            }
            if out.is_empty() {
                whole_file(content).into_iter().collect()
            } else {
                out
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FILE: &str = "line1\nline2\nTARGET\nline4\n";

    #[test]
    fn write_is_whole_file() {
        let r = edit_ranges("Write", &json!({"content": FILE}), FILE);
        assert_eq!(r, vec![LineRange::new(1, 4)]);
    }

    #[test]
    fn edit_locates_new_string_span() {
        let input = json!({"old_string": "x", "new_string": "TARGET"});
        let r = edit_ranges("Edit", &input, FILE);
        assert_eq!(r, vec![LineRange::new(3, 3)]);
    }

    #[test]
    fn edit_multiline_new_string_spans_multiple_lines() {
        let content = "a\nNEW1\nNEW2\nb\n";
        let input = json!({"new_string": "NEW1\nNEW2"});
        let r = edit_ranges("Edit", &input, content);
        assert_eq!(r, vec![LineRange::new(2, 3)]);
    }

    #[test]
    fn edit_falls_back_to_whole_file_when_not_found() {
        let input = json!({"new_string": "NOT PRESENT"});
        let r = edit_ranges("Edit", &input, FILE);
        assert_eq!(r, vec![LineRange::new(1, 4)]);
    }

    #[test]
    fn multiedit_collects_distinct_spans() {
        let content = "AAA\nb\nCCC\n";
        let input = json!({"edits": [
            {"new_string": "AAA"},
            {"new_string": "CCC"},
        ]});
        let r = edit_ranges("MultiEdit", &input, content);
        assert_eq!(r, vec![LineRange::new(1, 1), LineRange::new(3, 3)]);
    }

    #[test]
    fn unknown_tool_yields_nothing() {
        assert!(edit_ranges("Bash", &json!({}), FILE).is_empty());
    }
}
