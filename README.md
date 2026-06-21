# tellme

> Git blame, but for prompts and decisions.

`tellme` is a Rust CLI that captures the *intent* behind AI-generated code — the
prompt that created a line and the decision that justifies it — and makes it
queryable per file and per line, versioned alongside your repo.

```
tellme why checkout.py #line4     # which prompt produced this line?
tellme flow checkout.py --var total   # where does this variable live and die?
tellme journey controller/items.py --endpoint items   # DB → repo → service → API
tellme decision add checkout.py --line 7   # write down WHY
```

See [`PLAN.md`](./PLAN.md) for the technical plan, [`MBA.md`](./MBA.md) for the
product brief, and [`EXAMPLES.md`](./EXAMPLES.md) for a feature walk-through.

## Commands

| Command | What it does |
|---------|--------------|
| `tellme init` | Create the committed `.tellme/` store. |
| `tellme why <file> #lineN` | Prompts + decisions that shaped a line. |
| `tellme flow <file> --var <name>` | A variable's lifecycle (init → modify → use → end). |
| `tellme flow <file> --function <name>` | Callers, callees, signature. |
| `tellme flow … --graph` | Render the flow as a graph. |
| `tellme flow … --history` | Code + prompt + decision timeline. |
| `tellme journey <file> --endpoint <name>` | Trace data DB → repo → service → controller → API. |
| `tellme decision add <file> --line N` | Attach a written "why" (opens `$EDITOR`, or `-m`). |
| `tellme prompt add <file> --line N -m …` | Manually record a prompt against a line. |
| `tellme prompt list [--file <p>]` | List recorded prompts. |
| `tellme hook install` / `uninstall` | Wire up / remove capture hooks. |
| `tellme reconcile` | Promote captured edits to anchors after a commit. |

Global flags: `--repo <path>`, `--format <text\|json\|dot>`, `-v/-vv/-vvv`.
Language support is **Python** at v1 (analysis is behind a trait for more
later). `--format dot` on `flow --graph` emits Graphviz:

```sh
tellme flow checkout.py --function calculate_total --graph --format dot | dot -Tsvg > flow.svg
```

## Capturing prompts

`tellme` records the prompt behind each change live, then versions it with git.

```sh
tellme init            # create the .tellme/ store
tellme hook install    # add Claude Code capture hooks + a git post-commit hook
```

From then on, an agent session's prompts and the edits they produce are captured
automatically; on each commit they're reconciled into git-derived anchors. No
agent? Record one by hand:

```sh
tellme prompt add checkout.py --line 3 -m "free shipping over $50, else flat 7.99"
tellme prompt list --file checkout.py
```

Under the hood: `tellme capture` ingests a hook event from stdin, `tellme
reconcile` promotes captured edits to anchors once committed, and `tellme hook
uninstall` removes the hooks.

## Status

Early development. Tracked by milestones on
[GitHub](https://github.com/dhruva-nu/tellme/milestones); Phase 1 lays the
foundation (CLI skeleton, storage, git integration).

## Install

```sh
./install.sh                      # build release + install onto your PATH
BINDIR=~/.local/bin ./install.sh  # choose the install directory
./install.sh --uninstall          # remove it
```

The script builds in release mode and installs the `tellme` binary to
`/usr/local/bin` (using `sudo` only if needed) or `~/.local/bin`. Requires a
stable Rust toolchain — install from <https://rustup.rs>. Or use cargo
directly:

```sh
cargo install --path .
```

Tagged releases (`vX.Y.Z`) publish a prebuilt Linux binary via GitHub Actions.

## Build from source

```sh
cargo build
cargo test
```

Requires a recent stable Rust toolchain (edition 2021).

## License

MIT — see [LICENSE](./LICENSE).
