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
stable Rust toolchain — install from <https://rustup.rs>.

## Build from source

```sh
cargo build
cargo test
```

Requires a recent stable Rust toolchain (edition 2021).

## License

MIT — see [LICENSE](./LICENSE).
