//! `tellme hook install|uninstall` — wire up Claude Code + git hooks (#28).
//!
//! Install adds `UserPromptSubmit` and `PostToolUse` hooks to the project's
//! `.claude/settings.json` (merging, not clobbering) and a `post-commit` git
//! hook running `tellme reconcile`. Uninstall reverses both. Both are
//! idempotent and support `--dry-run`.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::Ctx;
use crate::error::{Error, Result};
use crate::git::Repo;

const POST_TOOL_MATCHER: &str = "Edit|MultiEdit|Write";

/// The hooks we manage, as `(event, optional matcher)`.
fn managed_events() -> [(&'static str, Option<&'static str>); 2] {
    [
        ("UserPromptSubmit", None),
        ("PostToolUse", Some(POST_TOOL_MATCHER)),
    ]
}

/// Path to the binary the hooks should invoke (this executable, or `tellme`).
fn exe() -> String {
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tellme".into())
}

/// `tellme hook install`.
pub fn install(ctx: &Ctx, dry_run: bool) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let exe = exe();

    let settings_path = root.join(".claude/settings.json");
    let existing = read_opt(&settings_path)?;
    let new_settings = merge_settings(existing.as_deref(), &format!("{exe} capture"))?;
    let settings_changed = existing.as_deref() != Some(new_settings.as_str());

    let hook_path = repo.git_dir().join("hooks/post-commit");
    let existing_hook = read_opt(&hook_path)?;
    let new_hook = add_reconcile(existing_hook.as_deref(), &format!("{exe} reconcile"));

    if dry_run {
        ctx.emit(
            "dry-run",
            &format!(
                "Would {} {}\nWould {} {}",
                if settings_changed { "update" } else { "leave" },
                settings_path.display(),
                if new_hook.is_some() {
                    "update"
                } else {
                    "leave"
                },
                hook_path.display(),
            ),
        );
        return Ok(());
    }

    if settings_changed {
        write_all(&settings_path, &new_settings)?;
    }
    if let Some(script) = new_hook {
        write_all(&hook_path, &script)?;
        make_executable(&hook_path)?;
    }
    ctx.emit(
        "installed",
        &format!(
            "Capture hooks installed (settings: {}, post-commit: {}).",
            yes_no(settings_changed),
            yes_no(true),
        ),
    );
    Ok(())
}

/// `tellme hook uninstall`.
pub fn uninstall(ctx: &Ctx, dry_run: bool) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let exe = exe();

    let settings_path = root.join(".claude/settings.json");
    let mut settings_changed = false;
    let new_settings = match read_opt(&settings_path)? {
        Some(s) => {
            let stripped = strip_settings(&s, &format!("{exe} capture"))?;
            settings_changed = stripped != s;
            Some(stripped)
        }
        None => None,
    };

    let hook_path = repo.git_dir().join("hooks/post-commit");
    let new_hook = match read_opt(&hook_path)? {
        Some(s) => remove_reconcile(&s),
        None => None,
    };

    if dry_run {
        ctx.emit(
            "dry-run",
            &format!(
                "Would {} {}\nWould {} {}",
                if settings_changed { "update" } else { "leave" },
                settings_path.display(),
                if new_hook.is_some() {
                    "update"
                } else {
                    "leave"
                },
                hook_path.display(),
            ),
        );
        return Ok(());
    }

    if let (true, Some(s)) = (settings_changed, &new_settings) {
        write_all(&settings_path, s)?;
    }
    if let Some(script) = new_hook {
        write_all(&hook_path, &script)?;
    }
    ctx.emit("uninstalled", "Capture hooks removed.");
    Ok(())
}

// ---- pure helpers (unit-tested) -------------------------------------------

/// Add our capture hooks to a `.claude/settings.json` body, idempotently.
/// Returns the pretty-printed result.
fn merge_settings(existing: Option<&str>, command: &str) -> Result<String> {
    let mut root: Value = match existing {
        Some(s) if !s.trim().is_empty() => {
            serde_json::from_str(s).map_err(|e| Error::Config(format!("settings.json: {e}")))?
        }
        _ => json!({}),
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| Error::Config("settings.json is not an object".into()))?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| Error::Config("settings.hooks is not an object".into()))?;

    for (event, matcher) in managed_events() {
        let arr = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| Error::Config(format!("settings.hooks.{event} is not an array")))?;
        if !contains_command(arr, command) {
            let inner = json!({ "hooks": [{ "type": "command", "command": command }] });
            let entry = match matcher {
                Some(m) => {
                    let mut e = inner;
                    e.as_object_mut()
                        .unwrap()
                        .insert("matcher".into(), json!(m));
                    e
                }
                None => inner,
            };
            arr.push(entry);
        }
    }
    Ok(serde_json::to_string_pretty(&root).unwrap() + "\n")
}

/// Remove our capture hooks from a settings body, returning the result.
fn strip_settings(existing: &str, command: &str) -> Result<String> {
    let mut root: Value =
        serde_json::from_str(existing).map_err(|e| Error::Config(format!("settings.json: {e}")))?;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        for (event, _) in managed_events() {
            if let Some(arr) = hooks.get_mut(event).and_then(Value::as_array_mut) {
                arr.retain(|entry| !entry_has_command(entry, command));
            }
        }
        hooks.retain(|_, v| v.as_array().map(|a| !a.is_empty()).unwrap_or(true));
    }
    Ok(serde_json::to_string_pretty(&root).unwrap() + "\n")
}

fn contains_command(arr: &[Value], command: &str) -> bool {
    arr.iter().any(|e| entry_has_command(e, command))
}

fn entry_has_command(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hs| {
            hs.iter()
                .any(|h| h.get("command").and_then(Value::as_str) == Some(command))
        })
        .unwrap_or(false)
}

/// Return an updated post-commit script that runs reconcile, or `None` if it
/// already does.
fn add_reconcile(existing: Option<&str>, reconcile_cmd: &str) -> Option<String> {
    let line = format!("{reconcile_cmd} >/dev/null 2>&1 || true");
    match existing {
        Some(s) if s.contains(reconcile_cmd) => None,
        Some(s) => {
            let mut out = s.to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("# added by tellme\n");
            out.push_str(&line);
            out.push('\n');
            Some(out)
        }
        None => Some(format!("#!/bin/sh\n# added by tellme\n{line}\n")),
    }
}

/// Strip our reconcile line from a post-commit script, or `None` if absent.
fn remove_reconcile(existing: &str) -> Option<String> {
    if !existing.contains("reconcile") {
        return None;
    }
    let kept: Vec<&str> = existing
        .lines()
        .filter(|l| !l.contains("reconcile") && l.trim() != "# added by tellme")
        .collect();
    // If only a shebang (or nothing) remains, drop the file entirely.
    let meaningful = kept
        .iter()
        .any(|l| !l.trim().is_empty() && !l.starts_with("#!"));
    if meaningful {
        Some(kept.join("\n") + "\n")
    } else {
        Some(String::new())
    }
}

// ---- io helpers -----------------------------------------------------------

fn read_opt(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
    }
}

fn write_all(path: &Path, contents: &str) -> Result<()> {
    if contents.is_empty() {
        // An empty result means "remove".
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "updated"
    } else {
        "unchanged"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_adds_both_events_and_is_idempotent() {
        let once = merge_settings(None, "tellme capture").unwrap();
        let v: Value = serde_json::from_str(&once).unwrap();
        assert!(contains_command(
            v["hooks"]["UserPromptSubmit"].as_array().unwrap(),
            "tellme capture"
        ));
        assert!(contains_command(
            v["hooks"]["PostToolUse"].as_array().unwrap(),
            "tellme capture"
        ));
        assert_eq!(v["hooks"]["PostToolUse"][0]["matcher"], POST_TOOL_MATCHER);

        // Running again must not duplicate entries.
        let twice = merge_settings(Some(&once), "tellme capture").unwrap();
        let v2: Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(v2["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_preserves_unrelated_settings() {
        let existing = r#"{"model":"opus","hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"other"}]}]}}"#;
        let merged = merge_settings(Some(existing), "tellme capture").unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn strip_removes_only_our_hooks() {
        let existing = r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"other"}]},{"hooks":[{"type":"command","command":"tellme capture"}]}]}}"#;
        let stripped = strip_settings(existing, "tellme capture").unwrap();
        let v: Value = serde_json::from_str(&stripped).unwrap();
        let arr = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(contains_command(arr, "other"));
    }

    #[test]
    fn post_commit_create_append_remove() {
        // Create fresh.
        let created = add_reconcile(None, "tellme reconcile").unwrap();
        assert!(created.starts_with("#!/bin/sh"));
        assert!(created.contains("tellme reconcile"));
        // Idempotent.
        assert!(add_reconcile(Some(&created), "tellme reconcile").is_none());
        // Append to an existing hook.
        let appended = add_reconcile(Some("#!/bin/sh\necho hi\n"), "tellme reconcile").unwrap();
        assert!(appended.contains("echo hi"));
        assert!(appended.contains("tellme reconcile"));
        // Remove.
        let removed = remove_reconcile(&appended).unwrap();
        assert!(removed.contains("echo hi"));
        assert!(!removed.contains("reconcile"));
    }

    #[test]
    fn remove_reconcile_drops_file_when_only_ours() {
        let only_ours = add_reconcile(None, "tellme reconcile").unwrap();
        assert_eq!(remove_reconcile(&only_ours), Some(String::new()));
    }
}
