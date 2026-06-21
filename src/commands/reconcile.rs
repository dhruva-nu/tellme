//! `tellme reconcile` — promote captured edits to anchors (#27).

use super::Ctx;
use crate::error::Result;
use crate::git::Repo;
use crate::paths::Layout;
use crate::reconcile::reconcile;
use crate::store::Store;

/// Run reconciliation over the repository's pending edits.
pub fn run(ctx: &Ctx) -> Result<()> {
    let repo = Repo::discover(&ctx.start_dir)?;
    let store = Store::open(&Layout::new(&repo.workdir()?))?;
    let report = reconcile(&store, &repo)?;
    ctx.emit(
        "reconciled",
        &format!(
            "Reconciled {} edit(s); {} still pending.",
            report.reconciled, report.skipped
        ),
    );
    Ok(())
}
