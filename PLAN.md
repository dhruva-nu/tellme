# tellme — Technical Plan

> Git blame, but for prompts and decisions. A Rust CLI that captures the *intent* behind AI-generated code and makes it queryable per file and per line.

This document is the high-level engineering plan: tech stack, architecture, and a phase-by-phase roadmap. Each phase maps to a GitHub **milestone**; work inside a phase is tracked as **issues**.

---

## 1. Product recap

`tellme` links **every line of code → the prompt that created it → the decision that justifies it**, and versions all three together over time. Core surfaces:

| Command | Feature |
|---------|---------|
| `tellme why <file> #lineN` | **Prompt Blame** — prompts that shaped a line (the signature feature) |
| `tellme flow <file> --var foo` | **Data Flow** — a variable's lifecycle in a function |
| `tellme flow <file> --function bar` | **Data Flow** — callers, callees, signature |
| `tellme journey <file> --endpoint x` | **Cross-Layer Journey** — data DB → repo → service → controller → API |
| `tellme decision add ...` | **Decision Editor** — attach written "why" to a node/line |
| `… --graph` | **Graph rendering** — visualize flow |
| `… --history` | **History** — code + prompt + decision timeline |

---

## 2. Key decisions (v1 scope)

These resolve the open questions in `MBA.md`:

1. **Prompt capture → agent session hooks (Claude Code first).** The primary path is a hook that captures each user prompt *and* the file edits that prompt produced, recording the prompt↔line linkage **at the moment of creation** rather than reconstructing it later. Manual `tellme prompt add` exists as a fallback. This is the single most important architectural bet: linkage is captured live, then versioned by git.
2. **Language scope → Python only for v1.** All examples are Python; one `tree-sitter` grammar gets us to a working signature demo fastest. The analysis engine is built behind a trait so additional languages slot in later.
3. **Graph rendering → ASCII first.** Terminal ASCII graphs match `EXAMPLES.md`. DOT/SVG export is deferred to the hardening phase.
4. **Storage → committed sidecar.** A `.tellme/` directory lives alongside the repo and is committed, so prompt/decision history travels with the code and is shareable. SQLite is the index; large blobs (full prompt text) stored as content-addressed files.
5. **Granularity → line range, git-aware.** Anchors target a `(file, line-range)` at a specific commit and are carried forward across edits using git's line history.

---

## 3. Tech stack

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Language | **Rust** (edition 2021) | Per brief; single static binary, fast. |
| CLI parsing | **clap** (derive) | De-facto standard, subcommands, help. |
| Code parsing | **tree-sitter** + `tree-sitter-python` | Incremental, multi-language-ready, robust ASTs. |
| Git integration | **git2** (libgit2) | Blame, log, diff, line-history without shelling out. |
| Storage index | **rusqlite** (bundled SQLite) | Structured queries over prompts/decisions/anchors; zero external dep. |
| Graph model | **petgraph** | In-memory graph; ASCII layout on top. |
| Serialization | **serde** / `serde_json` | Hook payloads, config, exports. |
| Terminal UI | **owo-colors** / `comfy-table` | Colored output, tables, boxes. |
| Editor launch | `$EDITOR` via **std::process** | Decision editor. |
| Errors / logging | **anyhow** + **thiserror**, **tracing** | Ergonomic errors, structured logs. |
| Testing | `cargo test` + **insta** (snapshots) | Snapshot the ASCII/graph output. |
| CI | **GitHub Actions** | build · test · fmt · clippy on PRs. |

---

## 4. Architecture

```
                        ┌─────────────────────────────┐
                        │      CLI layer (clap)        │
                        │  why · flow · journey ·      │
                        │  decision · prompt · init    │
                        └──────────────┬──────────────┘
                                       │
                        ┌──────────────▼──────────────┐
                        │      Command handlers        │
                        └──────────────┬──────────────┘
                                       │
        ┌──────────────┬───────────────┼───────────────┬──────────────┐
        ▼              ▼               ▼               ▼              ▼
 ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐
 │  Analysis  │ │   Blame /  │ │   Prompt   │ │  Decision  │ │  Rendering │
 │ (tree-     │ │  History   │ │   store    │ │   store    │ │ (ASCII /   │
 │  sitter)   │ │  (git2)    │ │            │ │            │ │  tables)   │
 └─────┬──────┘ └─────┬──────┘ └─────┬──────┘ └─────┬──────┘ └────────────┘
       │              │              │              │
       └──────────────┴──────┬───────┴──────────────┘
                             ▼
                  ┌─────────────────────┐
                  │  Storage (.tellme/) │
                  │  SQLite index +     │
                  │  content-addressed  │
                  │  prompt/decision    │
                  │  blobs (committed)  │
                  └─────────────────────┘
              ▲
   ┌──────────┴───────────┐
   │  Capture: agent hook │  (writes prompts + edit linkage into the store)
   │  (Claude Code)       │
   └──────────────────────┘
```

**The linking engine** is the heart: it joins three indices — code lines (from git blame), prompts (from capture), and decisions (from the editor) — keyed on git-aware line anchors, so any one can be queried from any other across history.

### 4.1 Data model (initial)

- `session` — an agent session (id, label, started_at).
- `prompt` — prompt text (content hash → blob), session_id, timestamp, ordinal.
- `edit` — a file change produced by a prompt: prompt_id, file, commit (nullable until committed), line-range.
- `anchor` — a stable reference to a `(file, line-range)` resolved per commit; carries forward via git line-history.
- `decision` — written "why" (content hash → blob), attached to an anchor (and optionally a graph node), author, timestamp, linked prompt_id.

### 4.2 The hard problem: line identity across edits

Prompts/decisions are attached to lines, but lines move. Strategy:
- At capture time, the hook records the **exact commit + line-range** an edit touched.
- At query time, `tellme why file #lineN` resolves the current line back through git's line history (`git log -L` / blame) to find every commit that touched it, then looks up prompts/decisions anchored to those commits/ranges.
- This makes blame *git-derived* (always consistent with the repo) rather than a fragile parallel record.

---

## 5. Phase roadmap (→ GitHub milestones)

| # | Milestone | Outcome |
|---|-----------|---------|
| **1** | **Foundation & Storage** | Buildable CLI skeleton, `.tellme/` SQLite store, git2 baseline, CI. `tellme init` works. |
| **2** | **Prompt Capture & Ingestion** | Claude Code hook captures prompts + edit linkage into the store; `tellme prompt add/list` manual fallback. |
| **3** | **Prompt Blame** | `tellme why <file> #lineN` resolves a line to its prompt history via git line-history. The signature demo. |
| **4** | **Data Flow Analysis** | tree-sitter Python; `tellme flow --var` and `--function` (list output). |
| **5** | **Graph Rendering** | ASCII flow graphs for `--graph`; petgraph model + layout. |
| **6** | **Cross-Layer Journey** | `tellme journey --endpoint`; multi-file layer tracing + transformation detection. |
| **7** | **Decision Editor** | `tellme decision add` with `$EDITOR`; attach to line/var/graph node. |
| **8** | **History** | `--history` on flow/why; stitch code + prompt + decision into one timeline. |
| **9** | **Hardening & Distribution** | DOT/SVG export, error polish, docs, `cargo install` + release binaries; groundwork for more languages. |

Phases are ordered so the **signature feature (Prompt Blame) ships by Phase 3**, before the heavier analysis features. Each subsequent phase is independently demoable.

---

## 6. Phase 1 — Foundation & Storage (detail)

**Goal:** a buildable, tested CLI skeleton with a working storage layer and git integration, so every later feature has a stable substrate. No user-facing feature yet except `tellme init`.

**Outcomes / definition of done:**
- `cargo build` / `cargo test` green; `clippy` and `fmt` clean in CI.
- `tellme --help` lists all planned subcommands (stubbed where not yet implemented).
- `tellme init` creates `.tellme/` with an initialized SQLite schema and config.
- Storage DAO can create/read `session`, `prompt`, `edit`, `anchor`, `decision` rows.
- git2 baseline: open the repo, resolve HEAD, blame a given line → commit.
- Documented line-anchor model.

Issues are tracked in the **Phase 1: Foundation & Storage** milestone on GitHub.
