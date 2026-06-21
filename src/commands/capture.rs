//! `tellme capture` — ingest a Claude Code hook event from stdin (#25).
//!
//! Invoked by the installed hooks. It must **never** fail the agent, so all
//! errors are caught and appended to `.tellme/cache/capture.log`; the command
//! always exits 0.

use std::io::{Read, Write};

use super::Ctx;
use crate::capture;
use crate::error::Result;
use crate::git::Repo;
use crate::paths::Layout;
use crate::store::Store;

/// Read one hook event from stdin and apply it. Always succeeds.
pub fn run(ctx: &Ctx) -> Result<()> {
    if let Err(e) = try_capture(ctx) {
        log_error(ctx, &e.to_string());
    }
    Ok(())
}

fn try_capture(ctx: &Ctx) -> Result<()> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let event = capture::parse_event(&buf)?;

    let repo = Repo::discover(&ctx.start_dir)?;
    let root = repo.workdir()?;
    let store = Store::open(&Layout::new(&root))?;

    let outcome = capture::ingest(&event, &store, &root, &ctx.start_dir)?;
    tracing::debug!(?outcome, "capture");
    Ok(())
}

/// Best-effort error log; never panics or propagates.
fn log_error(ctx: &Ctx, msg: &str) {
    tracing::warn!("capture failed: {msg}");
    let Ok(repo) = Repo::discover(&ctx.start_dir) else {
        return;
    };
    let Ok(root) = repo.workdir() else {
        return;
    };
    let cache = Layout::new(&root).cache_dir();
    if std::fs::create_dir_all(&cache).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cache.join("capture.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}
