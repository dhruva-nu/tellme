//! The `.tellme/` store: a SQLite index plus a content-addressed blob store,
//! with a typed DAO so command handlers never write raw SQL inline (#12, #13).

mod blobs;
mod models;
mod schema;

pub use models::{Anchor, Decision, Edit, PendingEdit, Prompt, Session};

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::paths::Layout;
use blobs::BlobStore;

/// Current Unix time in seconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// An open handle to a repository's `.tellme/` store.
pub struct Store {
    conn: Connection,
    blobs: BlobStore,
}

impl Store {
    /// Create a fresh store on disk (used by `tellme init`).
    ///
    /// Creates `.tellme/`, the blob directory, and a migrated database.
    pub fn create(layout: &Layout) -> Result<Self> {
        std::fs::create_dir_all(layout.tellme_dir())?;
        let blobs = BlobStore::open(&layout.blobs_dir())?;
        std::fs::create_dir_all(layout.cache_dir())?;
        let conn = Connection::open(layout.index_db())?;
        schema::migrate(&conn)?;
        Ok(Store { conn, blobs })
    }

    /// Open an existing store, erroring if it has not been initialized.
    pub fn open(layout: &Layout) -> Result<Self> {
        if !layout.is_initialized() {
            return Err(Error::StoreNotFound(layout.repo_root().to_path_buf()));
        }
        let blobs = BlobStore::open(&layout.blobs_dir())?;
        let conn = Connection::open(layout.index_db())?;
        schema::migrate(&conn)?;
        Ok(Store { conn, blobs })
    }

    /// An in-memory store for tests.
    #[cfg(test)]
    fn in_memory() -> Result<(Self, tempfile::TempDir)> {
        let dir = tempfile::tempdir().unwrap();
        let blobs = BlobStore::open(&dir.path().join("blobs"))?;
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok((Store { conn, blobs }, dir))
    }

    // ---- creation --------------------------------------------------------

    /// Create a session.
    pub fn create_session(
        &self,
        external_id: Option<&str>,
        label: Option<&str>,
    ) -> Result<Session> {
        let started_at = now();
        self.conn.execute(
            "INSERT INTO session (external_id, label, started_at) VALUES (?1, ?2, ?3)",
            (external_id, label, started_at),
        )?;
        Ok(Session {
            id: self.conn.last_insert_rowid(),
            external_id: external_id.map(str::to_string),
            label: label.map(str::to_string),
            started_at,
        })
    }

    /// Create a prompt, storing its text as a blob.
    pub fn create_prompt(&self, session_id: i64, ordinal: i64, text: &str) -> Result<Prompt> {
        let blob_hash = self.blobs.write(text)?;
        let created_at = now();
        self.conn.execute(
            "INSERT INTO prompt (session_id, ordinal, blob_hash, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            (session_id, ordinal, &blob_hash, created_at),
        )?;
        Ok(Prompt {
            id: self.conn.last_insert_rowid(),
            session_id,
            ordinal,
            blob_hash,
            created_at,
        })
    }

    /// Create an anchor for a `(file, line-range, commit)`.
    pub fn create_anchor(
        &self,
        file: &str,
        line_start: i64,
        line_end: i64,
        commit_id: &str,
    ) -> Result<Anchor> {
        self.conn.execute(
            "INSERT INTO anchor (file, line_start, line_end, commit_id) VALUES (?1, ?2, ?3, ?4)",
            (file, line_start, line_end, commit_id),
        )?;
        Ok(Anchor {
            id: self.conn.last_insert_rowid(),
            file: file.to_string(),
            line_start,
            line_end,
            commit_id: commit_id.to_string(),
        })
    }

    /// Link a prompt to an anchor as an edit.
    pub fn create_edit(&self, prompt_id: i64, anchor_id: i64) -> Result<Edit> {
        let created_at = now();
        self.conn.execute(
            "INSERT INTO edit (prompt_id, anchor_id, created_at) VALUES (?1, ?2, ?3)",
            (prompt_id, anchor_id, created_at),
        )?;
        Ok(Edit {
            id: self.conn.last_insert_rowid(),
            prompt_id,
            anchor_id,
            created_at,
        })
    }

    /// Create a decision attached to an anchor, storing its text as a blob.
    pub fn create_decision(
        &self,
        anchor_id: i64,
        text: &str,
        author: Option<&str>,
        prompt_id: Option<i64>,
    ) -> Result<Decision> {
        let blob_hash = self.blobs.write(text)?;
        let created_at = now();
        self.conn.execute(
            "INSERT INTO decision (anchor_id, blob_hash, author, prompt_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (anchor_id, &blob_hash, author, prompt_id, created_at),
        )?;
        Ok(Decision {
            id: self.conn.last_insert_rowid(),
            anchor_id,
            blob_hash,
            author: author.map(str::to_string),
            prompt_id,
            created_at,
        })
    }

    // ---- queries ---------------------------------------------------------

    /// Read blob text by hash (e.g. a prompt or decision body).
    pub fn read_text(&self, blob_hash: &str) -> Result<String> {
        self.blobs.read(blob_hash)
    }

    /// All anchors recorded for a file, ordered by starting line.
    pub fn anchors_for_file(&self, file: &str) -> Result<Vec<Anchor>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file, line_start, line_end, commit_id
             FROM anchor WHERE file = ?1 ORDER BY line_start",
        )?;
        let rows = stmt.query_map([file], |r| {
            Ok(Anchor {
                id: r.get(0)?,
                file: r.get(1)?,
                line_start: r.get(2)?,
                line_end: r.get(3)?,
                commit_id: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Prompts that produced edits at the given anchor, oldest first.
    pub fn prompts_for_anchor(&self, anchor_id: i64) -> Result<Vec<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.session_id, p.ordinal, p.blob_hash, p.created_at
             FROM prompt p
             JOIN edit e ON e.prompt_id = p.id
             WHERE e.anchor_id = ?1
             ORDER BY p.created_at",
        )?;
        let rows = stmt.query_map([anchor_id], |r| {
            Ok(Prompt {
                id: r.get(0)?,
                session_id: r.get(1)?,
                ordinal: r.get(2)?,
                blob_hash: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Decisions attached to an anchor, oldest first.
    pub fn decisions_for_anchor(&self, anchor_id: i64) -> Result<Vec<Decision>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, anchor_id, blob_hash, author, prompt_id, created_at
             FROM decision WHERE anchor_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map([anchor_id], |r| {
            Ok(Decision {
                id: r.get(0)?,
                anchor_id: r.get(1)?,
                blob_hash: r.get(2)?,
                author: r.get(3)?,
                prompt_id: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Look up a session by its external id.
    pub fn session_by_external_id(&self, external_id: &str) -> Result<Option<Session>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, external_id, label, started_at FROM session WHERE external_id = ?1",
                [external_id],
                |r| {
                    Ok(Session {
                        id: r.get(0)?,
                        external_id: r.get(1)?,
                        label: r.get(2)?,
                        started_at: r.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Look up a session by row id.
    pub fn session_by_id(&self, id: i64) -> Result<Option<Session>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, external_id, label, started_at FROM session WHERE id = ?1",
                [id],
                |r| {
                    Ok(Session {
                        id: r.get(0)?,
                        external_id: r.get(1)?,
                        label: r.get(2)?,
                        started_at: r.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Anchors a prompt produced edits at, ordered by file then line.
    pub fn anchors_for_prompt(&self, prompt_id: i64) -> Result<Vec<Anchor>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.file, a.line_start, a.line_end, a.commit_id
             FROM anchor a
             JOIN edit e ON e.anchor_id = a.id
             WHERE e.prompt_id = ?1
             ORDER BY a.file, a.line_start",
        )?;
        let rows = stmt.query_map([prompt_id], |r| {
            Ok(Anchor {
                id: r.get(0)?,
                file: r.get(1)?,
                line_start: r.get(2)?,
                line_end: r.get(3)?,
                commit_id: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Find a session by external id, creating it if absent.
    pub fn find_or_create_session(
        &self,
        external_id: &str,
        label: Option<&str>,
    ) -> Result<Session> {
        if let Some(s) = self.session_by_external_id(external_id)? {
            return Ok(s);
        }
        self.create_session(Some(external_id), label)
    }

    /// The most recently created prompt in a session, if any.
    pub fn latest_prompt_for_session(&self, session_id: i64) -> Result<Option<Prompt>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, session_id, ordinal, blob_hash, created_at
                 FROM prompt WHERE session_id = ?1
                 ORDER BY ordinal DESC, id DESC LIMIT 1",
                [session_id],
                |r| {
                    Ok(Prompt {
                        id: r.get(0)?,
                        session_id: r.get(1)?,
                        ordinal: r.get(2)?,
                        blob_hash: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Number of prompts already recorded in a session (used for the next ordinal).
    pub fn prompt_count_for_session(&self, session_id: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM prompt WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )?)
    }

    /// All prompts, newest first, optionally limited (for `prompt list`).
    pub fn recent_prompts(&self, limit: i64) -> Result<Vec<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, ordinal, blob_hash, created_at
             FROM prompt ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(Prompt {
                id: r.get(0)?,
                session_id: r.get(1)?,
                ordinal: r.get(2)?,
                blob_hash: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- pending edits ---------------------------------------------------

    /// Record an edit captured before it was committed.
    pub fn create_pending_edit(
        &self,
        prompt_id: i64,
        file: &str,
        line_start: i64,
        line_end: i64,
    ) -> Result<PendingEdit> {
        let created_at = now();
        self.conn.execute(
            "INSERT INTO pending_edit (prompt_id, file, line_start, line_end, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (prompt_id, file, line_start, line_end, created_at),
        )?;
        Ok(PendingEdit {
            id: self.conn.last_insert_rowid(),
            prompt_id,
            file: file.to_string(),
            line_start,
            line_end,
            created_at,
        })
    }

    /// Pending edits attributed to a given prompt, ordered by file then line.
    pub fn pending_edits_for_prompt(&self, prompt_id: i64) -> Result<Vec<PendingEdit>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, prompt_id, file, line_start, line_end, created_at
             FROM pending_edit WHERE prompt_id = ?1 ORDER BY file, line_start",
        )?;
        let rows = stmt.query_map([prompt_id], |r| {
            Ok(PendingEdit {
                id: r.get(0)?,
                prompt_id: r.get(1)?,
                file: r.get(2)?,
                line_start: r.get(3)?,
                line_end: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// All pending edits, oldest first.
    pub fn pending_edits(&self) -> Result<Vec<PendingEdit>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, prompt_id, file, line_start, line_end, created_at
             FROM pending_edit ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PendingEdit {
                id: r.get(0)?,
                prompt_id: r.get(1)?,
                file: r.get(2)?,
                line_start: r.get(3)?,
                line_end: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Delete a pending edit by id (after it has been reconciled).
    pub fn delete_pending_edit(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM pending_edit WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_entity() {
        let (store, _dir) = Store::in_memory().unwrap();

        let session = store
            .create_session(Some("checkout-v1"), Some("checkout"))
            .unwrap();
        assert_eq!(
            store
                .session_by_external_id("checkout-v1")
                .unwrap()
                .unwrap(),
            session
        );

        let prompt = store
            .create_prompt(session.id, 0, "add free shipping over $50")
            .unwrap();
        assert_eq!(
            store.read_text(&prompt.blob_hash).unwrap(),
            "add free shipping over $50"
        );

        let anchor = store
            .create_anchor("checkout.py", 4, 4, "deadbeef")
            .unwrap();
        assert_eq!(
            store.anchors_for_file("checkout.py").unwrap(),
            vec![anchor.clone()]
        );

        store.create_edit(prompt.id, anchor.id).unwrap();
        assert_eq!(
            store.prompts_for_anchor(anchor.id).unwrap(),
            vec![prompt.clone()]
        );

        let decision = store
            .create_decision(
                anchor.id,
                "threshold is exclusive",
                Some("you"),
                Some(prompt.id),
            )
            .unwrap();
        let got = store.decisions_for_anchor(anchor.id).unwrap();
        assert_eq!(got, vec![decision.clone()]);
        assert_eq!(
            store.read_text(&decision.blob_hash).unwrap(),
            "threshold is exclusive"
        );
    }

    #[test]
    fn anchors_for_unknown_file_is_empty() {
        let (store, _dir) = Store::in_memory().unwrap();
        assert!(store.anchors_for_file("nope.py").unwrap().is_empty());
    }

    #[test]
    fn find_or_create_session_is_stable() {
        let (store, _dir) = Store::in_memory().unwrap();
        let a = store.find_or_create_session("sess-1", Some("x")).unwrap();
        let b = store.find_or_create_session("sess-1", None).unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn latest_prompt_and_count_track_session() {
        let (store, _dir) = Store::in_memory().unwrap();
        let s = store.create_session(Some("s"), None).unwrap();
        assert_eq!(store.prompt_count_for_session(s.id).unwrap(), 0);
        assert!(store.latest_prompt_for_session(s.id).unwrap().is_none());

        store.create_prompt(s.id, 0, "first").unwrap();
        let p2 = store.create_prompt(s.id, 1, "second").unwrap();
        assert_eq!(store.prompt_count_for_session(s.id).unwrap(), 2);
        assert_eq!(
            store.latest_prompt_for_session(s.id).unwrap().unwrap().id,
            p2.id
        );
    }

    #[test]
    fn pending_edits_round_trip() {
        let (store, _dir) = Store::in_memory().unwrap();
        let s = store.create_session(Some("s"), None).unwrap();
        let p = store.create_prompt(s.id, 0, "do the thing").unwrap();
        let pe = store
            .create_pending_edit(p.id, "checkout.py", 4, 7)
            .unwrap();
        assert_eq!(store.pending_edits().unwrap(), vec![pe.clone()]);
        store.delete_pending_edit(pe.id).unwrap();
        assert!(store.pending_edits().unwrap().is_empty());
    }
}
