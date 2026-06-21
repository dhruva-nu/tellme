//! Promote pending edits to committed, git-derived anchors (#27).
//!
//! During an agent session edits are captured as pending edits with no commit.
//! Once the code is committed (typically via a `post-commit` git hook), this
//! turns each pending edit whose lines are now committed into an `anchor` at
//! its blame commit plus an `edit`, then drops the pending row. Idempotent:
//! reconciled rows are deleted, so re-running is a no-op.

use std::path::Path;

use crate::error::Result;
use crate::git::Repo;
use crate::store::Store;

/// Summary of a reconcile run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Pending edits turned into anchors.
    pub reconciled: usize,
    /// Pending edits left in place (file still dirty or lines not committed).
    pub skipped: usize,
}

/// Reconcile all pending edits against the repository.
pub fn reconcile(store: &Store, repo: &Repo) -> Result<Report> {
    let mut report = Report::default();
    for pe in store.pending_edits()? {
        let path = Path::new(&pe.file);

        // Only reconcile committed, clean files; leave the rest pending.
        if repo.is_path_dirty(path).unwrap_or(true) {
            report.skipped += 1;
            continue;
        }
        match repo.blame_line(path, pe.line_start as usize) {
            Ok(blamed) => {
                let anchor =
                    store.create_anchor(&pe.file, pe.line_start, pe.line_end, &blamed.commit.id)?;
                store.create_edit(pe.prompt_id, anchor.id)?;
                store.delete_pending_edit(pe.id)?;
                report.reconciled += 1;
            }
            // Lines not committed yet (or out of range): keep pending.
            Err(_) => report.skipped += 1,
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Layout;
    use git2::Repository;
    use std::fs;

    fn commit_all(git: &Repository, msg: &str) {
        let mut index = git.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = git.find_tree(index.write_tree().unwrap()).unwrap();
        let sig =
            git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(1_700_000_000, 0))
                .unwrap();
        let parents: Vec<git2::Commit> = match git.head().ok().and_then(|h| h.peel_to_commit().ok())
        {
            Some(c) => vec![c],
            None => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
            .unwrap();
    }

    #[test]
    fn reconciles_committed_pending_edit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = Repository::init(root).unwrap();
        // .tellme must not block a clean status check for our file.
        fs::write(root.join("checkout.py"), "a\nTARGET\nb\n").unwrap();
        commit_all(&git, "add checkout");

        let store = Store::create(&Layout::new(root)).unwrap();
        let s = store.create_session(Some("s1"), None).unwrap();
        let p = store.create_prompt(s.id, 0, "add target").unwrap();
        store
            .create_pending_edit(p.id, "checkout.py", 2, 2)
            .unwrap();

        let repo = Repo::discover(root).unwrap();
        let report = reconcile(&store, &repo).unwrap();
        assert_eq!(report.reconciled, 1);

        let anchors = store.anchors_for_file("checkout.py").unwrap();
        assert_eq!(anchors.len(), 1);
        let prompts = store.prompts_for_anchor(anchors[0].id).unwrap();
        assert_eq!(prompts, vec![p]);
        // Idempotent: nothing left to do.
        assert_eq!(reconcile(&store, &repo).unwrap().reconciled, 0);
    }

    #[test]
    fn skips_dirty_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = Repository::init(root).unwrap();
        fs::write(root.join("f.py"), "x\ny\n").unwrap();
        commit_all(&git, "init");
        // Make it dirty.
        fs::write(root.join("f.py"), "x\ny\nz\n").unwrap();

        let store = Store::create(&Layout::new(root)).unwrap();
        let s = store.create_session(Some("s"), None).unwrap();
        let p = store.create_prompt(s.id, 0, "edit").unwrap();
        store.create_pending_edit(p.id, "f.py", 3, 3).unwrap();

        let repo = Repo::discover(root).unwrap();
        let report = reconcile(&store, &repo).unwrap();
        assert_eq!(report.reconciled, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(store.pending_edits().unwrap().len(), 1);
    }
}
