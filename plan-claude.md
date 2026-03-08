# PLAN: linehash

## Overview

Hash-tagged file reading and hash-anchored editing for Claude Code.
The simplest tool in the suite — pure Rust, no tree-sitter, no LLM.
Eliminates `str_replace` failures by removing the need to reproduce exact whitespace.

---

## Why Workspace + `crates/` (not `src/`)

Popular Rust CLIs like ripgrep, fd, and bat all use this pattern:
- The workspace root holds config, docs, poc, and tests — **no source code**
- Each concern lives in its own crate under `crates/` — independently testable
- `src/` still exists but **inside each sub-crate**, never at the root
- Entry point is `crates/core/main.rs` instead of `src/main.rs`

`linehash` is small enough to need only one crate, but the layout stays consistent
with the other three tools in this suite.

---

## File Structure

```
linehash/
├── Cargo.toml              # workspace root — no source code here
├── Cargo.lock
├── README.md
├── POC.md
├── PLAN.md
├── .gitignore
│
├── crates/
│   └── core/               # single crate — simple enough, no need to split
│       ├── Cargo.toml
│       ├── main.rs         # CLI entry + command dispatch
│       ├── hash.rs         # xxhash per line → 2-char hex
│       ├── reader.rs       # parse file into Vec<Line>
│       ├── editor.rs       # edit / insert / delete operations
│       └── output.rs       # pretty-print and --json output
│
├── poc/
│   └── linehash.js         # Node.js POC — zero npm deps, pure built-ins
│
├── tests/
│   ├── edit_test.rs        # basic edit round-trips
│   ├── stale_test.rs       # stale hash rejection
│   └── ambiguity_test.rs   # collision handling
│
└── benches/
    └── hash_bench.rs       # target: hash a 10k-line file in < 5ms
```

### Root `Cargo.toml`

```toml
[workspace]
members = ["crates/core"]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"

[[bin]]
name = "linehash"
path = "crates/core/main.rs"

[workspace.dependencies]
xxhash-rust = { version = "0.8", features = ["xxh32"] }
clap = { version = "4", features = ["derive"] }
serde_json = "1"
serde = { version = "1", features = ["derive"] }
tempfile = "3"
anyhow = "1"
```

---

## Phases

### Phase 1 — POC (Node.js, half day)

Pure Node.js built-ins — no `npm install` required.

- [ ] Hash function: MD5 truncated to 2 hex chars per line
- [ ] `read`: output lines with `N:hash|` prefix
- [ ] `edit`: replace a line by hash reference
- [ ] `insert`: insert a new line after a hash anchor
- [ ] Stale detection: hash mismatch → reject with clear error
- [ ] Ambiguity detection: same hash on multiple lines → require `N:hash` form
- [ ] Benchmark: `str_replace` vs `linehash` on intentionally whitespace-corrupted edits

**Success criteria:** `str_replace` fails, `linehash` succeeds on the same task.

---

### Phase 2 — Rust Core (1–2 days)

The simplest Rust tool in the suite — no tree-sitter, no graph, no LLM.

- [ ] `hash.rs`: xxhash-rust, 2-char hex per trimmed line
- [ ] `reader.rs`: `Vec<Line>` with `n`, `hash`, `content`
- [ ] `editor.rs`:
  - `edit <file> <N:hash> <new_content>` — replace single line
  - `edit <file> <N:hash>..<N:hash> <new_content>` — replace range
  - `insert <file> <N:hash> <new_content>` — insert after anchor
  - `delete <file> <N:hash>` — remove line
- [ ] `output.rs`: pretty-print terminal output + `--json` mode
- [ ] Atomic writes via `tempfile` → rename
- [ ] CRLF line ending support (Windows compatibility)
- [ ] Actionable error messages for all failure modes

---

### Phase 3 — Polish (half day)

- [ ] `linehash index <file>` — show line numbers and hashes only, no content
- [ ] `linehash undo <file>` — revert last edit (backup in `.linehash/`)
- [ ] Graceful rejection of binary files
- [ ] Consistent handling of blank lines
- [ ] `cargo install linehash`

---

## Integration with Claude Code

Add to any project's `CLAUDE.md`:

```markdown
## File Editing Protocol

Always use linehash instead of str_replace for editing existing files.

1. Read:   `linehash read <file>`              — note the N:hash on each line
2. Edit:   `linehash edit <file> <N:hash> "<new content>"`
3. Insert: `linehash insert <file> <N:hash> "<new line>"`

Never reproduce old content. Reference the hash anchor only.
If you see "Stale hash": re-read the file first.
If you see "Ambiguous hash": use the N:hash form, e.g. 14:f1 instead of f1.
```

---

## Why Build This First

- Smallest scope: ~300 lines of Rust
- Zero heavy dependencies — only xxhash and clap
- Directly measurable improvement: str_replace failure rate vs linehash
- Immediate value: works on any Claude Code project from day one
- Fastest to ship: **~3 days total**

---

## Timeline

| Phase | Duration |
|---|---|
| 1 — POC | 0.5 day |
| 2 — Rust core | 1–2 days |
| 3 — Polish | 0.5 day |
| **Total** | **~3 days** |
