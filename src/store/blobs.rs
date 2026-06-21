//! Content-addressed blob store for prompt and decision text (#12).
//!
//! Text is stored once per distinct content, keyed by its SHA-256 hex digest,
//! under `.tellme/blobs/`. The digest is what the SQLite index references.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Result;

/// A directory of content-addressed text blobs.
#[derive(Debug, Clone)]
pub struct BlobStore {
    dir: PathBuf,
}

impl BlobStore {
    /// Open (and create if missing) a blob store at `dir`.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(BlobStore {
            dir: dir.to_path_buf(),
        })
    }

    /// Hash `content`, store it if new, and return the hex digest.
    pub fn write(&self, content: &str) -> Result<String> {
        let hash = hash_hex(content);
        let path = self.dir.join(&hash);
        if !path.exists() {
            fs::write(&path, content)?;
        }
        Ok(hash)
    }

    /// Read the content for a previously stored digest.
    pub fn read(&self, hash: &str) -> Result<String> {
        Ok(fs::read_to_string(self.dir.join(hash))?)
    }
}

/// SHA-256 of `content` as a lowercase hex string.
fn hash_hex(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut s = String::with_capacity(digest.len() * 2);
    for byte in digest {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_is_content_addressed_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let h1 = store.write("add free shipping over $50").unwrap();
        let h2 = store.write("add free shipping over $50").unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert_eq!(store.read(&h1).unwrap(), "add free shipping over $50");
    }

    #[test]
    fn distinct_content_distinct_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        assert_ne!(store.write("a").unwrap(), store.write("b").unwrap());
    }
}
