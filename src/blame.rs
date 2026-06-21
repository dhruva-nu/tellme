//! Prompt-blame query engine (#31): resolve a line to the prompts and
//! decisions that shaped it, git-derived per `docs/anchors.md`.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::git::Repo;
use crate::store::Store;

/// One prompt in a line's history, with any decisions on the same anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Commit the anchor was recorded against.
    pub commit_id: String,
    /// Commit summary line.
    pub commit_summary: String,
    /// Author display name.
    pub author: String,
    /// Author time (unix seconds).
    pub time: i64,
    /// Session label/external id, if known.
    pub session: Option<String>,
    /// The prompt text.
    pub prompt: String,
    /// Decisions attached to the same anchor.
    pub decisions: Vec<String>,
}

/// The result of `why` for a line (range).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhyResult {
    /// Repo-relative file.
    pub file: String,
    /// First line queried.
    pub line_start: i64,
    /// Last line queried.
    pub line_end: i64,
    /// Current text of the first queried line, if readable.
    pub code: Option<String>,
    /// Whether the line is committed (blame succeeded).
    pub committed: bool,
    /// Prompt history, oldest first.
    pub entries: Vec<Entry>,
}

/// Compute the prompt history for `file` lines `[line_start, line_end]`.
///
/// `repo_root` is the working directory; `file` is repo-relative.
pub fn why(
    store: &Store,
    repo: &Repo,
    repo_root: &Path,
    file: &str,
    line_start: i64,
    line_end: i64,
) -> Result<WhyResult> {
    let code = read_line(repo_root, file, line_start as usize);

    // Commits that touched the line range (git-derived). If blame fails the
    // line isn't committed yet.
    let commits =
        match repo.commits_touching(Path::new(file), line_start as usize, line_end as usize) {
            Ok(c) => c,
            Err(_) => {
                return Ok(WhyResult {
                    file: file.to_string(),
                    line_start,
                    line_end,
                    code,
                    committed: false,
                    entries: Vec::new(),
                })
            }
        };
    let commit_meta: HashMap<String, _> =
        commits.iter().map(|c| (c.id.clone(), c.clone())).collect();

    // Anchors for this file whose commit is in the touching set and whose
    // range overlaps the queried lines.
    let mut entries = Vec::new();
    for anchor in store.anchors_for_file(file)? {
        let overlaps = anchor.line_start <= line_end && anchor.line_end >= line_start;
        if !overlaps || !commit_meta.contains_key(&anchor.commit_id) {
            continue;
        }
        let meta = &commit_meta[&anchor.commit_id];
        let decisions: Vec<String> = store
            .decisions_for_anchor(anchor.id)?
            .iter()
            .filter_map(|d| store.read_text(&d.blob_hash).ok())
            .collect();
        for prompt in store.prompts_for_anchor(anchor.id)? {
            let session = store
                .session_by_id(prompt.session_id)?
                .and_then(|s| s.label.or(s.external_id));
            entries.push(Entry {
                commit_id: anchor.commit_id.clone(),
                commit_summary: meta.summary.clone(),
                author: meta.author_name.clone(),
                time: meta.time,
                session,
                prompt: store.read_text(&prompt.blob_hash).unwrap_or_default(),
                decisions: decisions.clone(),
            });
        }
    }
    entries.sort_by_key(|e| e.time);

    Ok(WhyResult {
        file: file.to_string(),
        line_start,
        line_end,
        code,
        committed: true,
        entries,
    })
}

/// Read one line (one-based) from the working tree, trimmed of the newline.
fn read_line(repo_root: &Path, file: &str, line: usize) -> Option<String> {
    let content = std::fs::read_to_string(repo_root.join(file)).ok()?;
    content
        .lines()
        .nth(line.saturating_sub(1))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Layout;
    use git2::Repository;
    use std::fs;

    fn commit_file(dir: &Path, name: &str, body: &str) {
        let git = Repository::init(dir).unwrap();
        fs::write(dir.join(name), body).unwrap();
        let mut index = git.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git.find_tree(tree_id).unwrap();
        let sig =
            git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(1_700_000_000, 0))
                .unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }

    #[test]
    fn returns_prompt_and_decision_for_anchored_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        commit_file(root, "checkout.py", "a\nship = 0 if x else 7.99\nb\n");
        let repo = Repo::discover(root).unwrap();
        let commit = repo.head_commit().unwrap().unwrap();

        let store = Store::create(&Layout::new(root)).unwrap();
        let s = store
            .create_session(Some("checkout-v1"), Some("checkout-v1"))
            .unwrap();
        let p = store
            .create_prompt(s.id, 0, "add free shipping over $50")
            .unwrap();
        let a = store
            .create_anchor("checkout.py", 2, 2, &commit.id)
            .unwrap();
        store.create_edit(p.id, a.id).unwrap();
        store
            .create_decision(a.id, "threshold is exclusive", Some("you"), Some(p.id))
            .unwrap();

        let r = why(&store, &repo, root, "checkout.py", 2, 2).unwrap();
        assert!(r.committed);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].prompt, "add free shipping over $50");
        assert_eq!(r.entries[0].session.as_deref(), Some("checkout-v1"));
        assert_eq!(r.entries[0].decisions, vec!["threshold is exclusive"]);
        assert_eq!(r.code.as_deref(), Some("ship = 0 if x else 7.99"));
    }

    #[test]
    fn unanchored_line_has_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        commit_file(root, "f.py", "x\ny\n");
        let repo = Repo::discover(root).unwrap();
        let store = Store::create(&Layout::new(root)).unwrap();
        let r = why(&store, &repo, root, "f.py", 1, 1).unwrap();
        assert!(r.committed);
        assert!(r.entries.is_empty());
    }
}
