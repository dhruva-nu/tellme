//! Domain structs mirroring the storage rows (#13).
//!
//! Timestamps are Unix seconds. `id` fields are SQLite rowids.

/// An agent (or manual) session that produced prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Row id.
    pub id: i64,
    /// External agent session identifier, if any.
    pub external_id: Option<String>,
    /// Human-friendly label.
    pub label: Option<String>,
    /// When the session started (unix seconds).
    pub started_at: i64,
}

/// A single prompt within a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// Row id.
    pub id: i64,
    /// Owning session.
    pub session_id: i64,
    /// Position within the session.
    pub ordinal: i64,
    /// Content-addressed hash of the prompt text.
    pub blob_hash: String,
    /// When the prompt was recorded (unix seconds).
    pub created_at: i64,
}

/// A stable reference to a `(file, line-range)` at a specific commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// Row id.
    pub id: i64,
    /// Repo-relative file path.
    pub file: String,
    /// First line (one-based, inclusive).
    pub line_start: i64,
    /// Last line (one-based, inclusive).
    pub line_end: i64,
    /// Git oid the range was recorded against.
    pub commit_id: String,
}

/// A file change produced by a prompt, located by an anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Row id.
    pub id: i64,
    /// Prompt that produced the edit.
    pub prompt_id: i64,
    /// Where the edit landed.
    pub anchor_id: i64,
    /// When the edit was recorded (unix seconds).
    pub created_at: i64,
}

/// A written "why" attached to an anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Row id.
    pub id: i64,
    /// Anchor the decision is attached to.
    pub anchor_id: i64,
    /// Content-addressed hash of the decision text.
    pub blob_hash: String,
    /// Who recorded the decision.
    pub author: Option<String>,
    /// Optional originating prompt.
    pub prompt_id: Option<i64>,
    /// When the decision was recorded (unix seconds).
    pub created_at: i64,
}

/// An edit captured during a session but not yet committed, so it has no
/// commit id and cannot be a git-derived anchor yet (see reconcile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEdit {
    /// Row id.
    pub id: i64,
    /// Prompt that produced the edit.
    pub prompt_id: i64,
    /// Repo-relative file path.
    pub file: String,
    /// First line (one-based, inclusive).
    pub line_start: i64,
    /// Last line (one-based, inclusive).
    pub line_end: i64,
    /// When the edit was captured (unix seconds).
    pub created_at: i64,
}
