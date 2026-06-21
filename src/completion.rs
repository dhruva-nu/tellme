//! Dynamic shell-completion callbacks (#10).
//!
//! Static completion (generated in `build.rs`) handles subcommands, flags, and
//! enum values. This module supplies the *dynamic* layer: completers invoked by
//! the binary at `<Tab>`-time, when the candidate set depends on repository
//! state and can't be baked into a script.
//!
//! The headline behaviour is basename-anywhere file completion — typing a bare
//! `errors.py<Tab>` resolves to `backend/dls/errors.py` (or offers the full set
//! when several files share the name). Candidates are drawn from git-tracked
//! files, so `.gitignore`d and untracked paths never appear.

use std::ffi::OsStr;
use std::path::Path;

use clap_complete::CompletionCandidate;

/// Complete a source-file argument from the repository's tracked files.
///
/// Matching rules, given the partial word `current`:
/// - empty → every tracked file,
/// - contains `/` → treated as a path prefix (ordinary path completion),
/// - otherwise → matches files whose path *or* basename starts with `current`,
///   which is what lets a bare basename expand to its full path.
///
/// Any error (not a repo, unreadable index) yields no candidates rather than a
/// failure — a completer must never make the shell misbehave.
pub fn complete_repo_file(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    tracked_files()
        .into_iter()
        .filter(|path| file_matches(path, &current))
        .map(CompletionCandidate::new)
        .collect()
}

/// Whether `path` should be offered for the partial word `current`.
///
/// See [`complete_repo_file`] for the rationale behind each branch.
fn file_matches(path: &str, current: &str) -> bool {
    if current.is_empty() {
        return true;
    }
    if current.contains('/') {
        return path.starts_with(current);
    }
    let basename = path.rsplit('/').next().unwrap_or(path);
    path.starts_with(current) || basename.starts_with(current)
}

/// Tracked files, expressed relative to the current directory where possible so
/// completing from a subdirectory inserts a usable path.
fn tracked_files() -> Vec<String> {
    let Ok(repo) = git2::Repository::discover(".") else {
        return Vec::new();
    };
    let Some(workdir) = repo.workdir().map(Path::to_path_buf) else {
        return Vec::new();
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| workdir.clone());
    let Ok(index) = repo.index() else {
        return Vec::new();
    };

    index
        .iter()
        .filter_map(|entry| {
            let rel = String::from_utf8(entry.path).ok()?;
            let abs = workdir.join(&rel);
            let shown = abs.strip_prefix(&cwd).unwrap_or(&abs);
            Some(shown.to_string_lossy().into_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::file_matches;

    #[test]
    fn bare_basename_matches_nested_path() {
        // The headline behaviour: `errors.py` finds `backend/dls/errors.py`.
        assert!(file_matches("backend/dls/errors.py", "errors.py"));
        assert!(file_matches("backend/dls/errors.py", "err"));
    }

    #[test]
    fn basename_match_is_anchored_not_substring() {
        // Matches the *start* of the basename, not any substring of it.
        assert!(!file_matches("backend/dls/errors.py", "rors"));
        assert!(!file_matches("backend/dls/errors.py", "dls"));
    }

    #[test]
    fn slash_switches_to_path_prefix() {
        assert!(file_matches("backend/dls/errors.py", "backend/dls"));
        assert!(!file_matches("backend/dls/errors.py", "dls/errors.py"));
    }

    #[test]
    fn empty_input_matches_everything() {
        assert!(file_matches("anything/at/all.py", ""));
    }

    #[test]
    fn top_level_file_still_matches_by_name() {
        assert!(file_matches("README.md", "README"));
        assert!(file_matches("README.md", "README.md"));
    }
}
