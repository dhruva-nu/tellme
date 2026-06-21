//! Ingest Claude Code hook events into the store (#25).
//!
//! A hidden `tellme capture` command pipes a single hook event as JSON on
//! stdin. `UserPromptSubmit` appends a prompt to the session; `PostToolUse`
//! for an edit tool records pending edits linked to the session's latest
//! prompt. Capture must never fail the agent — the command layer swallows
//! errors and logs them; this module returns them for testability.

mod ranges;

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::Result;
use crate::paths;
use crate::store::Store;

pub use ranges::{edit_ranges, LineRange};

/// A Claude Code hook event (the fields we care about).
#[derive(Debug, Clone, Deserialize)]
pub struct HookEvent {
    /// Agent session id; becomes the store session's external id.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Which hook fired.
    pub hook_event_name: String,
    /// The submitted prompt text (`UserPromptSubmit`).
    #[serde(default)]
    pub prompt: Option<String>,
    /// The tool that ran (`PostToolUse`).
    #[serde(default)]
    pub tool_name: Option<String>,
    /// The tool's input payload (`PostToolUse`).
    #[serde(default)]
    pub tool_input: Option<Value>,
}

/// What an ingest call did, for logging.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A prompt was appended to a session.
    PromptRecorded {
        /// Session external id.
        session: String,
    },
    /// Pending edits were recorded for a file.
    EditsRecorded {
        /// Repo-relative file.
        file: String,
        /// How many ranges.
        count: usize,
    },
    /// Nothing actionable in this event.
    Ignored {
        /// Why it was ignored.
        reason: String,
    },
}

/// Parse a hook event from raw JSON.
pub fn parse_event(json: &str) -> Result<HookEvent> {
    serde_json::from_str(json)
        .map_err(|e| crate::error::Error::Other(format!("invalid hook event JSON: {e}")))
}

/// Ingest one hook event. `repo_root` is the git working directory; `cwd` is
/// where the agent was running (used to resolve relative tool paths).
pub fn ingest(event: &HookEvent, store: &Store, repo_root: &Path, cwd: &Path) -> Result<Outcome> {
    let session_id = match event.session_id.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return Ok(Outcome::Ignored {
                reason: "no session_id".into(),
            })
        }
    };

    match event.hook_event_name.as_str() {
        "UserPromptSubmit" => ingest_prompt(event, store, session_id),
        "PostToolUse" => ingest_tool_use(event, store, repo_root, cwd, session_id),
        other => Ok(Outcome::Ignored {
            reason: format!("unhandled event {other}"),
        }),
    }
}

fn ingest_prompt(event: &HookEvent, store: &Store, session_id: &str) -> Result<Outcome> {
    let text = match event.prompt.as_deref() {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return Ok(Outcome::Ignored {
                reason: "empty prompt".into(),
            })
        }
    };
    let session = store.find_or_create_session(session_id, None)?;
    let ordinal = store.prompt_count_for_session(session.id)?;
    store.create_prompt(session.id, ordinal, text)?;
    Ok(Outcome::PromptRecorded {
        session: session_id.to_string(),
    })
}

fn ingest_tool_use(
    event: &HookEvent,
    store: &Store,
    repo_root: &Path,
    cwd: &Path,
    session_id: &str,
) -> Result<Outcome> {
    let tool_name = event.tool_name.as_deref().unwrap_or("");
    let input = match &event.tool_input {
        Some(v) => v,
        None => {
            return Ok(Outcome::Ignored {
                reason: "no tool_input".into(),
            })
        }
    };
    let file_path = match input.get("file_path").and_then(Value::as_str) {
        Some(p) => p,
        None => {
            return Ok(Outcome::Ignored {
                reason: "no file_path".into(),
            })
        }
    };

    // The session must already have a prompt to attribute the edit to.
    let session = match store.session_by_external_id(session_id)? {
        Some(s) => s,
        None => {
            return Ok(Outcome::Ignored {
                reason: "no session for edit".into(),
            })
        }
    };
    let prompt = match store.latest_prompt_for_session(session.id)? {
        Some(p) => p,
        None => {
            return Ok(Outcome::Ignored {
                reason: "no prompt to attribute edit to".into(),
            })
        }
    };

    // Read the file after the edit to compute line spans.
    let content = std::fs::read_to_string(file_path).unwrap_or_default();
    let ranges = edit_ranges(tool_name, input, &content);
    if ranges.is_empty() {
        return Ok(Outcome::Ignored {
            reason: format!("no ranges for tool {tool_name}"),
        });
    }

    let rel = paths::repo_relative(repo_root, cwd, Path::new(file_path))?;
    for r in &ranges {
        store.create_pending_edit(prompt.id, &rel, r.start as i64, r.end as i64)?;
    }
    Ok(Outcome::EditsRecorded {
        file: rel,
        count: ranges.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Layout;
    use std::fs;

    fn store_in(dir: &Path) -> Store {
        Store::create(&Layout::new(dir)).unwrap()
    }

    #[test]
    fn prompt_then_edit_records_prompt_and_pending_edit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store_in(root);
        fs::write(root.join("checkout.py"), "a\nTARGET\nb\n").unwrap();

        let prompt_event = parse_event(
            r#"{"session_id":"s1","hook_event_name":"UserPromptSubmit","prompt":"add target"}"#,
        )
        .unwrap();
        assert_eq!(
            ingest(&prompt_event, &store, root, root).unwrap(),
            Outcome::PromptRecorded {
                session: "s1".into()
            }
        );

        let file = root.join("checkout.py");
        let edit_json = format!(
            r#"{{"session_id":"s1","hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{{"file_path":"{}","new_string":"TARGET"}}}}"#,
            file.display()
        );
        let edit_event = parse_event(&edit_json).unwrap();
        let outcome = ingest(&edit_event, &store, root, root).unwrap();
        assert_eq!(
            outcome,
            Outcome::EditsRecorded {
                file: "checkout.py".into(),
                count: 1
            }
        );

        let pending = store.pending_edits().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].file, "checkout.py");
        assert_eq!((pending[0].line_start, pending[0].line_end), (2, 2));
    }

    #[test]
    fn edit_without_prior_prompt_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store_in(root);
        fs::write(root.join("f.py"), "x\n").unwrap();
        let file = root.join("f.py");
        let edit_json = format!(
            r#"{{"session_id":"s9","hook_event_name":"PostToolUse","tool_name":"Write","tool_input":{{"file_path":"{}","content":"x\n"}}}}"#,
            file.display()
        );
        let event = parse_event(&edit_json).unwrap();
        let outcome = ingest(&event, &store, root, root).unwrap();
        assert!(matches!(outcome, Outcome::Ignored { .. }));
    }

    #[test]
    fn event_without_session_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        let event = parse_event(r#"{"hook_event_name":"UserPromptSubmit","prompt":"hi"}"#).unwrap();
        assert!(matches!(
            ingest(&event, &store, dir.path(), dir.path()).unwrap(),
            Outcome::Ignored { .. }
        ));
    }
}
