# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commits & PRs

- **Never commit or open a PR unless explicitly asked.**
- All commits and PRs must carry this trailer (and only this co-author — do **not** add a "Co-Authored-By: Claude" line):

  ```
  Co-Authored-By: Dhruva <dhruvanu3@gmail.com>
  ```

## Commands

```sh
cargo build                  # build
cargo test                   # run all tests
cargo test parses_verbose    # run a single test by name substring
cargo run -- --help          # run the binary (args after `--`)
cargo run -- -vv             # run with trace-level logging

cargo fmt --all -- --check   # CI's format gate (rustfmt.toml: max_width = 100)
cargo clippy --all-targets --all-features -- -D warnings   # CI's lint gate
```

CI (`.github/workflows/ci.yml`) runs fmt → clippy → build → test with `RUSTFLAGS="-D warnings"`, so **warnings fail the build**. `Cargo.toml` also forbids `unsafe_code` and warns on all clippy lints. Run fmt + clippy locally before pushing.

## What this is

`tellme` is **"git blame, but for prompts and decisions"** — a CLI that links every line of code → the prompt that created it → the decision that justifies it, and versions all three alongside the repo. Planned command surface: `why` (prompt blame, the signature feature), `flow` (data-flow of a var/function), `journey` (cross-layer DB→API trace), `decision add`, `prompt add`, `init`.

**Status: early development.** `src/main.rs` is currently only the CLI scaffold (clap skeleton + tracing setup); the real subcommands are not built yet. Before implementing, read the design docs — they are the source of truth for architecture and scope:

- **`PLAN.md`** — tech stack, architecture diagram, data model, and the 9-phase roadmap (each phase = a GitHub milestone). **Read this first.**
- **`MBA.md`** — product brief / rationale.
- **`EXAMPLES.md`** — feature walk-through showing intended command output (the ASCII/table formats to target).

## Architecture (per PLAN.md — mostly not yet implemented)

The intended design, so new code lands in the right place:

- **CLI layer (clap)** → **command handlers** → five subsystems: Analysis (tree-sitter, Python-only for v1), Blame/History (git2), Prompt store, Decision store, Rendering (ASCII first).
- **Storage:** a committed `.tellme/` sidecar — SQLite index + content-addressed blobs for prompt/decision text. `.tellme/cache` is gitignored. Data model: `session`, `prompt`, `edit`, `anchor`, `decision`.
- **The core hard problem — line identity across edits:** prompts/decisions anchor to a `(file, line-range)` at a specific commit; queries resolve a current line back through git line-history (`git log -L`/blame) to find the commits that touched it. Blame is **git-derived**, not a fragile parallel record. Keep this invariant when touching anchor/query logic.
- **Capture is live, not reconstructed:** the primary path is a Claude Code agent hook that records the prompt↔edit linkage at creation time; `tellme prompt add` is the manual fallback.

When adding analysis features, build behind a trait so non-Python languages slot in later (v1 is Python-only by deliberate scope choice).
