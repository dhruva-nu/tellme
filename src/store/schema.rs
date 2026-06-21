//! SQLite schema and a forward-only migration runner (#12).
//!
//! Each migration is `(version, sql)`. On open we read the current
//! `user_version` pragma and apply every migration with a higher version in
//! order, inside a transaction.

use rusqlite::Connection;

use crate::error::Result;

/// Ordered list of migrations. Append new ones; never edit applied ones.
const MIGRATIONS: &[(i64, &str)] = &[(1, V1), (2, V2)];

/// Initial schema: sessions, prompts, anchors, edits, decisions.
const V1: &str = r#"
CREATE TABLE session (
    id          INTEGER PRIMARY KEY,
    external_id TEXT UNIQUE,                 -- agent session id / label, if any
    label       TEXT,
    started_at  INTEGER NOT NULL             -- unix seconds
);

CREATE TABLE prompt (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES session(id),
    ordinal    INTEGER NOT NULL,             -- position within the session
    blob_hash  TEXT    NOT NULL,             -- content-addressed prompt text
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_prompt_session ON prompt(session_id);

-- A stable reference to a (file, line-range) at a specific commit.
CREATE TABLE anchor (
    id         INTEGER PRIMARY KEY,
    file       TEXT    NOT NULL,
    line_start INTEGER NOT NULL,
    line_end   INTEGER NOT NULL,
    commit_id  TEXT    NOT NULL              -- git oid this range was recorded against
);
CREATE INDEX idx_anchor_file ON anchor(file);

-- A file change produced by a prompt, located by an anchor.
CREATE TABLE edit (
    id         INTEGER PRIMARY KEY,
    prompt_id  INTEGER NOT NULL REFERENCES prompt(id),
    anchor_id  INTEGER NOT NULL REFERENCES anchor(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_edit_anchor  ON edit(anchor_id);
CREATE INDEX idx_edit_prompt  ON edit(prompt_id);

-- A written "why" attached to an anchor, optionally linked to a prompt.
CREATE TABLE decision (
    id         INTEGER PRIMARY KEY,
    anchor_id  INTEGER NOT NULL REFERENCES anchor(id),
    blob_hash  TEXT    NOT NULL,             -- content-addressed decision text
    author     TEXT,
    prompt_id  INTEGER REFERENCES prompt(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_decision_anchor ON decision(anchor_id);
"#;

/// v2: pending edits captured during an agent session, before they are
/// committed and can be turned into git-derived anchors (see reconcile).
const V2: &str = r#"
CREATE TABLE pending_edit (
    id         INTEGER PRIMARY KEY,
    prompt_id  INTEGER NOT NULL REFERENCES prompt(id),
    file       TEXT    NOT NULL,             -- repo-relative path
    line_start INTEGER NOT NULL,
    line_end   INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_pending_edit_prompt ON pending_edit(prompt_id);
CREATE INDEX idx_pending_edit_file   ON pending_edit(file);
"#;

/// Apply all pending migrations to `conn`. Idempotent.
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (version, sql) in MIGRATIONS {
        if *version > current {
            conn.execute_batch("BEGIN;")?;
            conn.execute_batch(sql)?;
            // user_version does not accept a bound parameter.
            conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
            conn.execute_batch("COMMIT;")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_sets_user_version_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        let latest = MIGRATIONS.last().map(|(v, _)| *v).unwrap();
        assert_eq!(v, latest);
        // Running again must not error or re-apply.
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='anchor'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn v2_adds_pending_edit() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='pending_edit'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migrates_v1_db_up_to_v2() {
        // Start at v1 only, then run the full migration set.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("BEGIN;").unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch("PRAGMA user_version = 1; COMMIT;")
            .unwrap();
        migrate(&conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);
    }
}
