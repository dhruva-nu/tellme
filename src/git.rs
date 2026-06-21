//! Git integration baseline over libgit2 (#14).
//!
//! Only the operations later phases need: discover the repo, resolve HEAD,
//! blame a line, list the commits that touched a line range, and detect a
//! dirty tree. Blame is intentionally git-derived — see `docs/anchors.md`.

use std::path::{Path, PathBuf};

use git2::{BlameOptions, Repository, StatusOptions};

use crate::error::{Error, Result};

/// A thin wrapper around an opened repository.
pub struct Repo {
    inner: Repository,
}

/// Metadata about a single commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    /// Full hex object id.
    pub id: String,
    /// First line of the commit message.
    pub summary: String,
    /// Author display name.
    pub author_name: String,
    /// Author email.
    pub author_email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
}

/// The result of blaming one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    /// One-based line number that was blamed.
    pub line: usize,
    /// The commit that last touched the line.
    pub commit: CommitInfo,
}

impl Repo {
    /// Discover the repository at or above `start`.
    pub fn discover(start: &Path) -> Result<Self> {
        let inner =
            Repository::discover(start).map_err(|_| Error::NotAGitRepo(start.to_path_buf()))?;
        Ok(Repo { inner })
    }

    /// The working directory (errors for bare repos).
    pub fn workdir(&self) -> Result<PathBuf> {
        self.inner
            .workdir()
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::Other("repository has no working directory (bare repo)".into()))
    }

    /// Resolve `HEAD` to its commit metadata. Returns `None` on an unborn branch.
    pub fn head_commit(&self) -> Result<Option<CommitInfo>> {
        match self.inner.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                Ok(Some(commit_info(&commit)))
            }
            // Fresh repo with no commits yet.
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Whether the working tree or index has uncommitted changes.
    pub fn is_dirty(&self) -> Result<bool> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true).include_ignored(false);
        let statuses = self.inner.statuses(Some(&mut opts))?;
        Ok(!statuses.is_empty())
    }

    /// Blame a single one-based line in `file` (path relative to the repo root).
    pub fn blame_line(&self, file: &Path, line: usize) -> Result<BlameLine> {
        let blame = self
            .inner
            .blame_file(file, Some(&mut BlameOptions::new()))?;
        let hunk = blame.get_line(line).ok_or_else(|| {
            Error::Other(format!(
                "no blame information for {}:{line}",
                file.display()
            ))
        })?;
        let commit = self.inner.find_commit(hunk.final_commit_id())?;
        Ok(BlameLine {
            line,
            commit: commit_info(&commit),
        })
    }

    /// The distinct commits that touched `[start_line, end_line]` in `file`.
    ///
    /// A pragmatic baseline for `git log -L`: blame the range and collect the
    /// unique originating commits, most recent first by author time.
    pub fn commits_touching(
        &self,
        file: &Path,
        start_line: usize,
        end_line: usize,
    ) -> Result<Vec<CommitInfo>> {
        let blame = self
            .inner
            .blame_file(file, Some(&mut BlameOptions::new()))?;
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for line in start_line..=end_line {
            if let Some(hunk) = blame.get_line(line) {
                let id = hunk.final_commit_id();
                if !seen.contains(&id) {
                    seen.push(id);
                    let commit = self.inner.find_commit(id)?;
                    out.push(commit_info(&commit));
                }
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.time));
        Ok(out)
    }
}

fn commit_info(commit: &git2::Commit) -> CommitInfo {
    let author = commit.author();
    CommitInfo {
        id: commit.id().to_string(),
        summary: commit.summary().unwrap_or_default().to_string(),
        author_name: author.name().unwrap_or_default().to_string(),
        author_email: author.email().unwrap_or_default().to_string(),
        time: author.when().seconds(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a repo in a temp dir with one committed file and return (dir, Repo).
    fn repo_with_file(name: &str, contents: &str) -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let git = Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join(name), contents).unwrap();

        let mut index = git.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = git.find_tree(index.write_tree().unwrap()).unwrap();
        let sig =
            git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(1_700_000_000, 0))
                .unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        let repo = Repo::discover(dir.path()).unwrap();
        (dir, repo)
    }

    #[test]
    fn discover_errors_outside_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Repo::discover(dir.path()),
            Err(Error::NotAGitRepo(_))
        ));
    }

    #[test]
    fn head_and_dirty_state() {
        let (dir, repo) = repo_with_file("a.txt", "one\ntwo\n");
        let head = repo.head_commit().unwrap().unwrap();
        assert_eq!(head.summary, "initial commit");
        assert_eq!(head.author_name, "Ada");
        assert!(!repo.is_dirty().unwrap());

        fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
        assert!(repo.is_dirty().unwrap());
    }

    #[test]
    fn blames_a_known_line() {
        let (_dir, repo) = repo_with_file("checkout.py", "x = 1\ny = 2\n");
        let blamed = repo.blame_line(Path::new("checkout.py"), 2).unwrap();
        assert_eq!(blamed.line, 2);
        assert_eq!(blamed.commit.author_email, "ada@example.com");
        assert_eq!(blamed.commit.summary, "initial commit");
    }

    #[test]
    fn commits_touching_range_dedups() {
        let (_dir, repo) = repo_with_file("checkout.py", "a\nb\nc\n");
        let commits = repo
            .commits_touching(Path::new("checkout.py"), 1, 3)
            .unwrap();
        assert_eq!(commits.len(), 1);
    }
}
