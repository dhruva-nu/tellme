# Line anchors — design (#11)

> How prompts and decisions stay attached to lines as code moves.

This is the foundational scheme that **Prompt Blame** (Phase 3) and **History**
(Phase 8) build on. It defines what an *anchor* is, how one is recorded, and how
a current line resolves back to the prompts/decisions that shaped it.

---

## 1. The problem

Prompts and decisions are attached to *lines*. But lines move: edits insert and
delete above them, refactors split functions, files get renamed. A naïve
"prompt X → line 7" record rots the moment line 7 shifts. We need attachments
that survive ordinary code evolution without us maintaining a fragile parallel
copy of git's history.

## 2. Core idea: anchors are git-derived, not a parallel record

An **anchor** is a reference to a region of code *at a specific commit*:

```
anchor = (file, line_start, line_end, commit_id)
```

- `file` — repo-relative path **as it existed at `commit_id`**.
- `line_start..=line_end` — one-based, inclusive line range.
- `commit_id` — the git oid the range was recorded against.

The key invariant: **we never try to keep `line_start`/`line_end` "current".**
They are frozen at `commit_id`. To answer questions about the code *today*, we
ask git to bridge from today's lines back to the commits anchors were recorded
against. Git is the source of truth for line movement; our store only holds the
`(commit, range) → prompt/decision` associations.

This is why blame is *git-derived*: a `tellme why` result is always consistent
with the actual repository, because the line→commit mapping comes from libgit2,
not from a record we have to keep in sync by hand.

## 3. Recording an anchor (capture time)

When a prompt produces an edit (primary path: the Claude Code hook in Phase 2),
we know exactly what changed and at which commit it was committed:

1. The hook captures the prompt text and the files/line-ranges it edited.
2. When that work is committed, we record an `anchor` with the **commit oid**
   and the **edited line range** in that commit.
3. An `edit` row links the `prompt` to the `anchor`; a `decision` (if any)
   links to the same `anchor`.

Manual `tellme prompt add` / `tellme decision add` follow the same shape, using
`HEAD` as the commit and a user-supplied (or current) line range.

## 4. Resolving a current line (query time)

To answer `tellme why <file> #lineN`:

1. **Find the commits that touched line N.** Use git line-history
   (`git log -L N,N:<file>` semantics). The baseline implementation in `git.rs`
   blames the line/range and collects the distinct originating commits
   (`Repo::blame_line`, `Repo::commits_touching`); a later phase can upgrade to
   full `-L` walking for multi-revision history.
2. **Match anchors.** For each touching commit, look up anchors recorded against
   that `commit_id` whose range overlaps the blamed line (after mapping through
   the diff for that commit).
3. **Collect attachments.** Gather the `prompt`s (via `edit`) and `decision`s on
   the matched anchors, ordered by commit time → the timeline `why`/`--history`
   renders.

Because step 1 is pure git, the moment a line is edited again the new commit
enters its history automatically; we only need to have recorded an anchor at the
commit where each change happened.

## 5. Edge cases

| Situation | Handling |
|-----------|----------|
| **Line moved** (insert/delete above) | No anchor change needed — git blame maps today's line number to the commit that introduced it; we match anchors at that commit. |
| **Line modified** | The modifying commit appears in the line's history; its anchor (recorded at capture) matches. Older anchors remain attached to the earlier commits in the chain. |
| **Line split** (one line → many) | Each resulting line blames to either the original or the splitting commit; anchors on the original commit still resolve via that commit. Range overlap is checked per commit. |
| **Line deleted** | The line no longer exists today, so `why <file> #lineN` can't target it. Its anchors are still reachable through `--history` on neighbouring lines / the file, and remain in the store (not garbage-collected). |
| **File renamed** | Blame and line-history follow renames (libgit2 rename detection). Anchors store the path *at their commit*; resolution keys on `commit_id`, so a later rename does not break the link. |
| **Uncommitted lines** | Blame has no commit for working-tree-only lines; `why` reports the line as not yet committed. Capture records the anchor once the change is committed. |

## 6. Why not store "current line numbers"?

Two records of the same truth always drift. Keeping line numbers current would
mean re-mapping every anchor on every commit — duplicating what git already
computes, and getting it subtly wrong on merges, rebases, and rename chains. By
freezing anchors at their commit and bridging with git at query time, the store
stays small, append-only, and correct by construction.

## 7. What this commits us to (invariants for later phases)

- Anchors are immutable once written; corrections are new anchors, not edits.
- Every attachment (prompt/decision) hangs off an `anchor`, never a bare line.
- Query paths must derive the line→commit mapping from `git.rs`, never from a
  cached line number in the store.
