# tellme — Product Brief

> A CLI documentation tool that works like **git blame, but for prompts and decisions.**

---

## 1. The Problem

I (and most developers now) write a large share of my code using AI agents. The code works, it ships — but a few weeks later I open a file and hit a wall:

- **"What is this even doing?"** — I've lost the mental model of code I technically "wrote."
- **"Why was it built this way?"** — The reasoning behind a choice lived in a chat window that's now gone.
- **"What prompt produced *this exact line*?"** — There's no way to trace a line of code back to the instruction that created it.

Git tells me *who* changed a line and *when*. It does **not** tell me the **intent** — the prompt and the decision behind the change. That intent is the most valuable and most easily lost artifact of AI-assisted development.

## 2. The Vision

**tellme** is a documentation tool that captures and surfaces the *intent* behind code.

Its **primary purpose** is documentation: a living record of the decisions made while building a system. Its **signature feature** is prompt-level traceability — like `git blame`, but instead of showing the author of a line, it shows the **prompt** and the **decision** that produced it.

The goal: open any file, point at any line, and ask **"tell me why."**

## 3. What It Is

| Attribute | Decision |
|-----------|----------|
| Language | **Rust** |
| Form factor | **CLI** |
| Category | **Code documentation tool** |
| Versioning | **Git-style history** — tracks prompts & decisions across time, per file and per line |

## 4. Core Capabilities

### 4.1 Prompt Blame (the signature feature)
Trace any line of code back to the prompts that shaped it.

```
tellme why <filename> #line12
```

Returns every prompt that changed that line of code over time — the prompt-level equivalent of `git blame`.

> *Priority: this is the differentiator, but it is secondary to the broader documentation goal below.*

### 4.2 Data Flow Analysis (primary documentation feature)
Understand how data and control move through the code, rendered as a **graph**.

**Variable flow**
```
tellme flow <filename> --var foo
```
Shows where `foo` is:
- initialized
- modified
- used
- where its lifecycle ends

**Function flow**
```
tellme flow <filename> --function bar
```
Shows:
- where `bar` is called
- what functions `bar` calls
- its input and return types

Both render a **graph** visualizing the flow.

### 4.3 Cross-Layer Data Flow (Data Journey)
Where 4.2 traces a variable *within a function*, this traces a piece of data **across the architecture** — from where it originates to where it leaves the system.

```
tellme journey <filename> --endpoint items
```

For a controller like `items()` that returns all items, it shows the data moving through each layer as connected **boxes**:

```
DB table  →  repository  →  service  →  controller  →  API response
```

Each box shows what shape the data has *at that layer* (e.g. raw row → model → DTO → JSON) and where the transformation happens. This answers "where does this data actually come from, and what touches it on the way out?" — the question that matters most when debugging or onboarding.

### 4.4 Decision Editor
The flow graphs aren't just generated — they're **annotated**. On any node or edge in a graph (e.g. "why does this function call that one?" or "why does it do this specific thing?"), I can attach a written decision explaining the reasoning.

This is how the documentation gets richer over time: the tool extracts structure, the human adds intent.

### 4.5 History
Add `--history` to any flow query to see the timeline behind it.

```
tellme flow <filename> --var foo --history
```

Returns:
- when the variable/line was **created**
- when it was **last modified**
- the **decisions** attached along the way
- the **prompts** that created or modified the file/line

History stitches together three layers — **code change + decision + originating prompt** — into one auditable narrative.

## 5. How It Fits Together

```
        ┌──────────────────────────────────────────────┐
        │                   tellme                       │
        ├──────────────┬──────────────┬─────────────────┤
        │  Prompt Blame │  Flow Graphs │  Decision Editor │
        │  (why line12) │  (var/func)  │  (annotate why)  │
        └──────┬───────┴──────┬───────┴────────┬─────────┘
               │              │                │
               └──────────────┴────────────────┘
                              │
                   ┌──────────▼───────────┐
                   │   Git-style History   │
                   │ code + decision + prompt
                   │   versioned per line  │
                   └───────────────────────┘
```

The unifying idea: **every line of code is linked to the prompt that created it and the decision that justifies it**, and all three are versioned together over time.

## 6. Who It's For

Developers working with AI agents who need to **understand, defend, and revisit** decisions in code they generated — long after the original conversation is gone. Also useful for teammates onboarding onto AI-generated codebases, and for anyone doing code review who wants to know *why*, not just *what*.

## 7. Out of Scope (for now)

- IDE / editor plugins (CLI-first)
- Real-time collaboration on decisions
- Non-git versioning backends
- Language support beyond an initial target set (TBD)

## 8. Open Questions for the Product Owner

1. **Prompt capture** — how do prompts get recorded? Hook into agent sessions, a wrapper, or manual import? This is foundational to prompt blame working at all.
2. **Language scope** — which languages do flow analysis and `why` support at v1?
3. **Graph rendering** — terminal ASCII graph, export to image (SVG/DOT), or both?
4. **Storage model** — does decision/prompt history live alongside the repo (committed) or in a sidecar store?
5. **Granularity of blame** — exact line, line range, or semantic block?
