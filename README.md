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

## Status

Early development. Tracked by milestones on
[GitHub](https://github.com/dhruva-nu/tellme/milestones); Phase 1 lays the
foundation (CLI skeleton, storage, git integration).

## Build

```sh
cargo build
cargo test
```

Requires a recent stable Rust toolchain (edition 2021).

## License

MIT — see [LICENSE](./LICENSE).
