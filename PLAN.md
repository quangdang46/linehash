# PLAN: linehash

## Overview

Hash-tagged file reading and hash-anchored editing for Claude Code.
The simplest tool in the suite — pure Rust, no tree-sitter, no LLM.
Eliminates `str_replace` failures by removing the need to reproduce exact whitespace.

The first release optimizes for:

- **safety**: reject stale or ambiguous edits instead of guessing
- **predictability**: simple, explicit CLI behavior
- **low integration friction**: easy for Claude Code to adopt
- **small surface area**: no parser, no AST, no daemon, no persistent service

---

## V1 Scope

### Must ship

- `linehash read <file>`
- `linehash index <file>`
- `linehash edit <file> <anchor> <new_content>`
- `linehash edit <file> <start>..<end> <new_content>`
- `linehash insert <file> <anchor> <new_content>`
- `linehash delete <file> <anchor>`
- `linehash verify <file> <anchor> [<anchor>...]` — pre-flight anchor check, no mutation
- `linehash grep <file> <pattern>` — regex search returning anchors
- `linehash annotate <file> <substring>` — reverse lookup: content → anchor
- `linehash patch <file> <patch.json>` — atomic multi-op transaction from JSON
- `linehash swap <file> <anchor-a> <anchor-b>` — atomic line transposition
- `linehash move <file> <anchor> after <anchor-b>` — move line to after anchor
- `linehash indent <file> <start>..<end> [+N|-N]` — anchor-range indent/dedent
- `linehash find-block <file> <anchor>` — discover containing block boundaries
- `linehash stats <file>` — token budget, collision report, anchor quality
- `linehash from-diff <file> <diff>` — compile unified diff → linehash patch
- `linehash merge-patches <a.json> <b.json>` — compose/conflict-detect two patches
- `linehash watch <file>` — live hash recomputation on file change
- `linehash explode <file> --out <dir>` — decompose to one-file-per-line
- `linehash implode <dir> --out <file>` — reassemble from exploded directory
- `read --context N --anchor <anchor>` — show ±N lines around an anchor
- `read --json` includes `mtime` and `inode` for optimistic concurrency guard
- `--expect-mtime <ts>` / `--expect-inode <n>` guard on all mutation commands
- `--dry-run` flag on all mutation commands (edit, insert, delete, swap, move, indent, patch)
- `--receipt` flag on all mutations — structured JSON before/after record
- `--audit-log <path>` — append receipts to a JSONL audit log automatically
- pretty output and `--json`
- atomic writes
- clear ambiguity and stale-read errors
- tests for core resolution and file rewrite behavior

### Explicitly out of scope for v1

- `linehash diff`
- `linehash undo` (trivially implementable from audit log — deferred to v2)
- multi-line block insert/replacement
- persistent read snapshots
- move-tolerant anchor recovery
- non-UTF-8 support
- editor plugins
- `linehash mcp` MCP server mode (v2 — needs protocol research)
- `linehash session` stateful snapshot sessions (v2)
- git-aware annotation overlay (v2)
- `linehash watch` daemon / persistent socket mode (v2 — v1 watch exits after first change)
- cross-file patch (v2)

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

## Spec Decisions (Freeze Before Writing Code)

These decisions must be locked before implementation begins. Ambiguity here is
the most common source of correctness bugs in a tool like this.

### 1) Hash the raw line bytes, excluding only the newline terminator

**Decision:** hash the exact line content as stored in the file, excluding `\n` or `\r\n`.

Example:
- file bytes: `"  return decoded\n"`
- hashed content: `"  return decoded"`

Do **not** trim leading or trailing whitespace.

**Why:** trimming weakens stale-read detection for whitespace-only edits, which is
especially risky in indentation-sensitive formats like Python and YAML. A tool that
silently ignores whitespace changes in Python is dangerous. The hash must reflect
the line exactly as it lives on disk.

### 2) Preserve file formatting exactly

For each file read:
- detect newline style: LF or CRLF
- detect whether the file ends with a trailing newline
- preserve both when writing back

If the file mixes newline styles, **fail with a helpful message** in v1 rather than
silently normalizing. Mixed newlines are almost always a prior tooling bug and should
be surfaced, not hidden.

### 3) UTF-8 only in v1

Read the file as UTF-8. Invalid UTF-8 returns a clear error. If non-UTF-8 support is
needed later, redesign around `bstr` or byte slices at that time.

### 4) Canonical anchor display is `N:hash`

Display format:

```text
2:f1|   const decoded = jwt.verify(token, SECRET)
```

Accepted input forms:
- `f1` → unqualified short hash
- `2:f1` → line-qualified short hash
- `2:f1..4:9c` → inclusive range

### 5) Safety-first anchor resolution

#### Unqualified anchor: `f1`

- 0 matches → `hash not found`
- 1 match → resolve to that line
- 2+ matches → `ambiguous hash`, show candidate lines

#### Qualified anchor: `2:f1`

- if line 2 currently has hash `f1` → resolve
- if line 2 does not have hash `f1` → **stale anchor** error; if `f1` exists elsewhere,
  mention it but do not silently retarget

This is intentionally conservative. The tool rejects moved lines rather than guessing.
That is the correct tradeoff for an agent editing tool.

### 6) Single logical line content only in v1

`edit` and `insert` reject `new_content` containing `\n` or `\r`. Multi-line
insert/replace is deferred to post-v1.

### 7) Read-whole-file approach

Read the entire file into memory, transform, write back atomically. Fine for normal
source files and keeps the code simple and auditable.

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
│   └── core/
│       ├── Cargo.toml
│       ├── main.rs             # thin entry point — error print + exit code only
│       ├── cli.rs              # clap structs — CLI parsing separate from logic
│       ├── error.rs            # LinehashError enum via thiserror
│       ├── hash.rs             # xxhash per raw line → 2-char hex
│       ├── anchor.rs           # anchor parsing and resolution
│       ├── document.rs         # Document::load — Vec<LineRecord> + newline detection
│       ├── output.rs           # pretty-print and --json output
│       ├── writeback.rs        # atomic file rewrite with permission preservation
│       ├── patch.rs            # patch file schema, parsing, multi-op application
│       ├── concurrency.rs      # mtime/inode guard helpers
│       ├── receipt.rs          # operation receipt schema + audit log writer
│       ├── block.rs            # brace/indent block boundary discovery
│       ├── diff_import.rs      # unified diff → PatchFile compiler
│       ├── stats.rs            # token budget, collision, anchor quality analysis
│       ├── watch.rs            # inotify/kqueue/ReadDirectoryChanges wrapper
│       ├── explode.rs          # file → per-line directory decomposition
│       └── commands/
│           ├── mod.rs
│           ├── read.rs
│           ├── index.rs
│           ├── edit.rs
│           ├── insert.rs
│           ├── delete.rs
│           ├── verify.rs
│           ├── grep.rs
│           ├── annotate.rs
│           ├── patch.rs
│           ├── swap.rs
│           ├── move.rs
│           ├── indent.rs
│           ├── find_block.rs
│           ├── stats.rs
│           ├── from_diff.rs
│           ├── merge_patches.rs
│           ├── watch.rs
│           ├── explode.rs
│           └── implode.rs
│
├── poc/
│   └── linehash.js         # Node.js POC — zero npm deps, pure built-ins
│
├── tests/
│   ├── read_cli.rs
│   ├── index_cli.rs
│   ├── edit_cli.rs
│   ├── insert_cli.rs
│   ├── delete_cli.rs
│   ├── verify_cli.rs
│   ├── grep_cli.rs
│   ├── annotate_cli.rs
│   ├── patch_cli.rs
│   ├── concurrency_cli.rs
│   ├── swap_cli.rs
│   ├── move_cli.rs
│   ├── indent_cli.rs
│   ├── find_block_cli.rs
│   ├── stats_cli.rs
│   ├── from_diff_cli.rs
│   ├── merge_patches_cli.rs
│   ├── watch_cli.rs
│   ├── explode_implode_cli.rs
│   ├── receipt_cli.rs
│   ├── dry_run_cli.rs
│   └── fixtures/           # static test files (LF, CRLF, no-trailing-newline, binary, etc.)
│
└── benches/
    └── hash_bench.rs       # target: hash a 10k-line file in < 5ms
```

### Layout rationale

- CLI parsing lives in `cli.rs`, never mixed with business logic
- `document.rs` and `anchor.rs` are unit-testable without invoking the CLI
- one file per command in `commands/` keeps each path independently auditable
- `writeback.rs` is isolated so atomic write behavior can be reviewed and tested alone
- library modules (`block.rs`, `stats.rs`, etc.) are unit-testable without the CLI

### Root `Cargo.toml`

```toml
[workspace]
members = ["crates/core"]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
xxhash-rust = { version = "0.8", features = ["xxh32"] }
clap = { version = "4", features = ["derive"] }
serde_json = "1"
serde = { version = "1", features = ["derive"] }
thiserror = "1"
tempfile = "3"
anyhow = "1"
regex = "1"        # grep, annotate --regex; no default features to avoid Unicode bloat
notify = "6"       # cross-platform file watching; feature-flagged
walkdir = "2"      # directory traversal for explode/implode and grep --files
```

### `crates/core/Cargo.toml`

```toml
[package]
name = "linehash"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "linehash"
path = "main.rs"

[dependencies]
xxhash-rust = { workspace = true }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tempfile = { workspace = true }
anyhow = { workspace = true }
regex = { workspace = true }
notify = { workspace = true }
walkdir = { workspace = true }

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
insta = "1"
tempfile = { workspace = true }
```

### Bootstrap commands

```bash
cargo new linehash --bin
cd linehash
cargo add clap --features derive
cargo add xxhash-rust --features xxh32
cargo add serde --features derive
cargo add serde_json
cargo add thiserror
cargo add tempfile
cargo add anyhow
cargo add regex --no-default-features
cargo add notify
cargo add walkdir
cargo add --dev assert_cmd predicates insta tempfile
```

---

## Core Data Model

```rust
pub struct LineRecord {
    pub number: usize,      // 1-based for all UX and error messages
    pub content: String,    // raw content, no newline terminator
    pub short_hash: String, // always 2 lowercase hex chars
    pub full_hash: u32,     // kept internal; only short_hash exposed in output
}

pub enum NewlineStyle {
    Lf,
    Crlf,
}

pub struct FileMeta {
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub inode: u64,         // 0 on platforms without inodes (Windows)
}

pub struct Document {
    pub path: std::path::PathBuf,
    pub newline: NewlineStyle,
    pub trailing_newline: bool,
    pub lines: Vec<LineRecord>,
    pub meta: Option<FileMeta>, // captured at load time; included in --json output
}

pub enum Anchor {
    Hash { short: String },
    LineHash { line: usize, short: String },
}

pub struct RangeAnchor {
    pub start: Anchor,
    pub end: Anchor,
}

pub struct ResolvedLine {
    pub index: usize,       // 0-based for internal Vec mutations
    pub line_no: usize,     // 1-based for UX messages
    pub short_hash: String,
}

// Patch file schema
#[derive(serde::Deserialize)]
pub struct PatchFile {
    pub file: String,
    pub ops: Vec<PatchOp>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "op")]
pub enum PatchOp {
    #[serde(rename = "edit")]
    Edit { anchor: String, content: String },
    #[serde(rename = "insert")]
    Insert { anchor: String, content: String },
    #[serde(rename = "delete")]
    Delete { anchor: String },
}

// Operation receipt — emitted on --receipt, appended to audit log on --audit-log
#[derive(serde::Serialize)]
pub struct Receipt {
    pub op: String,               // "edit" | "insert" | "delete" | "swap" | "move" | "indent" | "patch"
    pub file: String,
    pub timestamp: i64,           // unix seconds
    pub changes: Vec<LineChange>, // one entry per affected line
    pub file_hash_before: u32,    // xxh32 of full file bytes before write
    pub file_hash_after: u32,     // xxh32 of full file bytes after write
}

#[derive(serde::Serialize)]
pub struct LineChange {
    pub line_no: usize,
    pub kind: ChangeKind,
    pub before: Option<String>,   // None for insertions
    pub after: Option<String>,    // None for deletions
    pub hash_before: Option<String>,
    pub hash_after: Option<String>,
}

#[derive(serde::Serialize)]
pub enum ChangeKind { Modified, Inserted, Deleted }

// Block discovery result
pub struct BlockBounds {
    pub start: ResolvedLine,
    pub end: ResolvedLine,
    pub language_hint: BlockLanguage,
}

pub enum BlockLanguage {
    Brace,    // { } — JS, Rust, C, Java, Go...
    Indent,   // Python, YAML, TOML sections
    Unknown,  // returned when detection is inconclusive — do not guess
}

// Stats report
pub struct FileStats {
    pub line_count: usize,
    pub unique_hashes: usize,
    pub collision_count: usize,              // lines sharing a 2-char hash with another line
    pub collision_pairs: Vec<(usize, usize)>,// (line_a, line_b) sharing a hash
    pub estimated_read_tokens: usize,        // rough: (total chars / 4) + overhead
    pub hash_length_advice: u8,              // 2 if p(collision) < 1%, else 3 or 4
    pub suggested_context_n: usize,          // median function size / 2, capped at 20
}
```

---

## Error Model

Use a dedicated error enum with `thiserror`. Every variant must suggest a recovery action.

```rust
#[derive(Debug, thiserror::Error)]
pub enum LinehashError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("file is not valid UTF-8")]
    InvalidUtf8,

    #[error("file contains mixed LF and CRLF line endings")]
    MixedNewlines,

    #[error("invalid anchor '{0}'")]
    InvalidAnchor(String),

    #[error("invalid range '{0}'")]
    InvalidRange(String),

    #[error("hash '{hash}' not found\nHint: run `linehash read <file>` to get current hashes")]
    HashNotFound { hash: String },

    #[error("hash '{hash}' matches {count} lines ({lines:?})\nUse line-qualified form, e.g. {example}:{hash}")]
    AmbiguousHash { hash: String, count: usize, lines: Vec<usize>, example: usize },

    #[error("line {line} content changed since last read (expected hash {expected}, got {actual})\nHint: re-read with `linehash read <file>`")]
    StaleAnchor { line: usize, expected: String, actual: String },

    #[error("multi-line content is not supported in v1")]
    MultiLineContentUnsupported,

    #[error("file is empty — no anchor to resolve")]
    EmptyFile,

    // Optimistic concurrency guard
    #[error("file was modified since last read (expected mtime {expected}, got {actual})\nHint: re-read with `linehash read <file>`")]
    StaleFile { expected: String, actual: String },

    // Patch errors
    #[error("patch op {op_index} failed: {reason}\nNo changes were applied.")]
    PatchFailed { op_index: usize, reason: String },

    #[error("invalid patch file: {0}")]
    InvalidPatch(String),

    // Grep / annotate
    #[error("invalid regex pattern '{0}': {1}")]
    InvalidPattern(String, String),

    // swap / move
    #[error("swap anchor A and anchor B resolve to the same line ({line_no})")]
    SwapSameLine { line_no: usize },

    // indent
    #[error("range start (line {start}) is after range end (line {end})")]
    InvalidIndentRange { start: usize, end: usize },

    #[error("dedent by {amount} would underflow line {line_no} (only {available} leading spaces available)")]
    IndentUnderflow { line_no: usize, amount: usize, available: usize },

    // block discovery
    #[error("could not find balanced block boundary from line {line_no} — check for unmatched braces")]
    UnbalancedBlock { line_no: usize },

    #[error("block language is ambiguous at line {line_no} — use an explicit range anchor instead")]
    AmbiguousBlockLanguage { line_no: usize },

    // diff import
    #[error("diff hunk at line {hunk_line} could not be matched to current file content")]
    DiffHunkMismatch { hunk_line: usize },

    #[error("diff targets '{diff_file}' but file argument is '{given_file}'")]
    DiffFileMismatch { diff_file: String, given_file: String },

    // patch merge
    #[error("conflict between patch A op {a} and patch B op {b}: both target anchor {anchor}")]
    PatchConflict { a: usize, b: usize, anchor: String },

    // watch
    #[error("watch is not supported on this platform")]
    WatchUnsupported,

    // explode / implode
    #[error("output directory '{0}' already exists and is non-empty — use --force to overwrite")]
    ExplodeTargetExists(String),

    #[error("implode directory '{0}' contains non-linehash files — aborting")]
    ImplodeDirty(String),
}
```

Exit behavior:
- clap parse errors → clap's default behavior
- `LinehashError` → print to stderr, exit non-zero
- `main` uses `anyhow` only as a thin wrapper at the top level

---

## CLI Contract

### `read` — annotated file view

```bash
linehash read src/auth.js
1:a3| function verifyToken(token) {
2:f1|   const decoded = jwt.verify(token, SECRET)
3:0e|   if (!decoded.exp) throw new TokenError('missing expiry')
4:9c|   return decoded
5:b2| }
```

#### `read --anchor --context` — focused neighborhood view

```bash
linehash read src/auth.js --anchor 14:f1 --context 3
11:2a|   // previous middleware
12:b3|   if (!token) return res.status(401).send()
13:0e|   token = token.split(' ')[1]
→ 14:f1|   const decoded = jwt.verify(token, SECRET)
15:9c|   if (!decoded.exp) throw new TokenError('missing expiry')
16:a1|   return decoded
17:b2| }
```

When `--anchor` is given, only lines within ±N of that anchor are shown. The target line
is marked with `→`. Multiple `--anchor` flags show multiple neighborhoods, merged if
overlapping. Default context is 5 lines if `--anchor` is given without `--context`.

#### `read --json` with file metadata

```json
{
  "file": "src/auth.js",
  "newline": "lf",
  "trailing_newline": true,
  "mtime": 1714000000,
  "mtime_nanos": 123456789,
  "inode": 12345678,
  "lines": [
    { "n": 1, "hash": "a3", "content": "function verifyToken(token) {" },
    { "n": 2, "hash": "f1", "content": "  const decoded = jwt.verify(token, SECRET)" }
  ]
}
```

### `index` — hashes only

```bash
linehash index src/auth.js
1:a3
2:f1
3:0e
4:9c
5:b2
```

`index --json`:

```json
{
  "file": "src/auth.js",
  "lines": [
    { "n": 1, "hash": "a3" },
    { "n": 2, "hash": "f1" }
  ]
}
```

### `verify` — pre-flight anchor check

```bash
linehash verify src/auth.js 2:f1 4:9c f1
✓  2:f1  resolves → "  const decoded = jwt.verify(token, SECRET)"
✓  4:9c  resolves → "  return decoded"
✗  f1    AMBIGUOUS — matches lines 2, 14 — use 2:f1 or 14:f1
```

Exit code 0 only if all anchors resolved. Exit non-zero if any failed.
`--json` returns a structured array with `anchor`, `status`, `line_no`, `content`, `error`.

### `grep` — regex search returning anchors

```bash
linehash grep src/auth.js "jwt\.verify"
2:f1|   const decoded = jwt.verify(token, SECRET)
```

Returns only matching lines in standard anchor format. With `--json`, returns the same
`lines` array schema as `read --json` filtered to matches only.

### `annotate` — reverse lookup: content → anchor

```bash
linehash annotate src/auth.js "jwt.verify"
2:f1|   const decoded = jwt.verify(token, SECRET)
```

Finds lines whose content contains the given substring (or regex with `--regex`). If
multiple lines match, returns all. Use `--expect-one` to error on ambiguity.

### `patch` — atomic multi-op transaction

```bash
linehash patch src/auth.js changes.json
Applied 3 ops: 1 edit, 1 insert, 1 delete.
```

Patch file format:

```json
{
  "file": "src/auth.js",
  "ops": [
    { "op": "edit",   "anchor": "2:f1", "content": "  const decoded = jwt.verify(token, SECRET_KEY)" },
    { "op": "insert", "anchor": "4:9c", "content": "  logger.debug('token verified')" },
    { "op": "delete", "anchor": "5:b2" }
  ]
}
```

Behavior:
- File is read **once**. All anchors resolved against the same snapshot.
- If **any** anchor is stale, ambiguous, or not found → entire patch rejected, file untouched.
- `--dry-run` validates all anchors and shows what would change without writing.
- `--receipt` emits structured JSON after a successful write.

### `swap` — atomic line transposition

```bash
linehash swap src/config.js 14:f1 28:9c
Swapped lines 14 and 28.

linehash swap src/config.js 14:f1 28:9c --dry-run
Would swap:
  line 14: "  const timeout = 5000"
  line 28: "  const retries = 3"
No file was written.
```

Both anchors resolved against same snapshot. Single atomic write.

### `move` — relocate a line by anchor

```bash
linehash move src/config.js 14:f1 after 28:9c
Moved line 14 to after line 28.
```

Removes the source line and inserts it after the target anchor. Both resolved from the
same snapshot. Single atomic write. `--dry-run` supported.

### `indent` — anchor-range indent/dedent

```bash
linehash indent src/main.py 14:f1..28:9c +4
linehash indent src/main.py 14:f1..28:9c -2
```

Adds or removes N spaces of leading whitespace from every line in the range.
Auto-detects whether the file uses spaces or tabs. Fails on mixed indentation within
the range rather than silently mangling. `--dry-run` shows the full before/after for
every line in the range.

### `find-block` — block boundary discovery

```bash
linehash find-block src/auth.js 14:f1
Block: 12:b3..19:7a  (8 lines — brace-balanced)

linehash find-block src/main.py 22:a1
Block: 20:3c..31:f0  (12 lines — indent-delimited)
```

Walks outward from the anchor line counting brace/bracket depth (C-family) or indent
level (Python/YAML). Returns start and end as anchors that can be passed directly to
`edit`, `delete`, or `patch` as a range. Returns an error rather than a wrong answer
when boundaries are ambiguous.

`--json`: `{ "start": "12:b3", "end": "19:7a", "lines": 8, "language": "brace" }`

### `stats` — token budget and anchor quality report

```bash
linehash stats src/auth.js
Lines:                   847
Unique hashes (2-char):  831  (98.1% — low collision risk)
Collisions:               16  lines share a hash with ≥1 other line
Collision pairs:       [2,14]  [67,203]  [...]
Est. read tokens:    ~4,200
Hash length advice:    2-char sufficient  (p(any collision) ≈ 1.8%)
Suggested --context:   8 lines for function-level edits
```

`--json` returns a machine-readable version of the same report. Agents can run `stats`
before `read` to decide whether to read the full file or use `--context`, and to
preemptively know which anchors need qualification.

### `from-diff` — compile unified diff to linehash patch

```bash
git diff HEAD src/auth.js | linehash from-diff src/auth.js -
# pipe directly into patch
git diff HEAD src/auth.js | linehash from-diff src/auth.js - | linehash patch src/auth.js -
```

Reads a unified diff (from stdin or file), resolves each hunk against the current file
on disk, and emits a `PatchFile` JSON to stdout. If a hunk cannot be matched to current
file content, fails with `DiffHunkMismatch` naming the hunk. Never silently applies a
mismatched hunk.

### `merge-patches` — compose or conflict-detect two patch files

```bash
linehash merge-patches feature-a.json feature-b.json --base src/auth.js
```

Resolves both patches against the current file. If no ops overlap, emits a merged patch
that applies both changesets in a single atomic write. If ops conflict, reports all
conflicts explicitly:

```
CONFLICT: op 2 in feature-a.json and op 1 in feature-b.json both target 14:f1
  feature-a wants: "  const decoded = jwt.verify(token, SECRET_KEY)"
  feature-b wants: "  const decoded = jwt.verify(token, REFRESH_SECRET)"
Resolve manually, then re-run.
```

Non-conflicting ops from both patches are included in the merged output even when
conflicts exist, so a human can see exactly what needs manual resolution.

### `watch` — live hash recomputation on file change

```bash
linehash watch src/auth.js
Watching src/auth.js — Ctrl-C to stop
[14:22:01] Changed: line 2 f1→3a, line 5 unchanged
[14:22:01] New index: 847 lines, 1 hash changed
```

Uses `notify` crate (inotify/kqueue/ReadDirectoryChangesW) to watch the file. On each
save, recomputes all hashes, diffs old vs. new hash list, and emits a structured change
report. `--json` streams newline-delimited JSON events suitable for agent framework
subscription.

v1: `--once` is the default — exits after the first change event. `--continuous` keeps
watching until Ctrl-C. Daemon/socket mode deferred to v2.

### `explode` / `implode` — line-per-file decomposition

```bash
linehash explode src/auth.js --out .linehash/exploded/auth/
# Creates: .linehash/exploded/auth/0001_a3.txt  ("function verifyToken(token) {")
#          .linehash/exploded/auth/0002_f1.txt  ("  const decoded = jwt.verify...")
#          .linehash/exploded/auth/.meta.json   (newline style, trailing newline, source path)

linehash implode .linehash/exploded/auth/ --out src/auth.js
```

`explode` decomposes a file into one tiny file per line. Each filename is
`{NNNN}_{hash}.txt` — the filename *is* the anchor. `.meta.json` stores formatting
metadata for faithful round-trip. `implode` reassembles by sorting filenames and
joining with the original newline style.

Use cases:
- Any file-level tool can now target individual lines by filename
- `diff .linehash/exploded/auth/ .linehash/exploded/auth.bak/` shows exactly which lines
  changed as a clean file diff
- Git treats each line as a tracked file: `git status` on an exploded directory shows
  which lines are dirty after an edit session
- `--force` overwrites an existing exploded directory

### `--dry-run` on all mutation commands

```bash
linehash edit src/auth.js 2:f1 "new content" --dry-run
Would change line 2:
  - "  const decoded = jwt.verify(token, SECRET)"
  + "  const decoded = jwt.verify(token, SECRET_KEY)"
No file was written.

linehash indent src/main.py 14:f1..28:9c +4 --dry-run
Would indent 15 lines by +4 spaces:
  line 14: "def foo():" → "    def foo():"
  line 15: "    x = 1"  → "        x = 1"
  [...]
No file was written.
```

`--dry-run --json` returns the full proposed post-mutation `Document` as a structured
object, suitable for agent inspection before committing.

### `--receipt` and `--audit-log`

```bash
linehash edit src/auth.js 2:f1 "new content" --receipt
{
  "op": "edit",
  "file": "src/auth.js",
  "timestamp": 1714000123,
  "changes": [
    {
      "line_no": 2,
      "kind": "Modified",
      "before": "  const decoded = jwt.verify(token, SECRET)",
      "after":  "  const decoded = jwt.verify(token, SECRET_KEY)",
      "hash_before": "f1",
      "hash_after": "3a"
    }
  ],
  "file_hash_before": 2871289732,
  "file_hash_after":  3109283710
}

# Append all receipts to a persistent audit log
linehash edit src/auth.js 2:f1 "new content" --audit-log .linehash/audit.jsonl
```

`--receipt` prints the receipt to stdout after the write. `--audit-log <path>` appends
it to a JSONL file (one receipt per line). The audit log is the foundation for `undo`
in v2 — reading the log in reverse and applying inverse operations requires no
additional state beyond what v1 already writes.

### Optimistic concurrency guard

Any mutation command accepts:

```bash
linehash edit src/auth.js 2:f1 "new content" --expect-mtime 1714000000
linehash edit src/auth.js 2:f1 "new content" --expect-inode 12345678
```

Before applying the change, `stat()` the file. If mtime or inode doesn't match,
reject with `StaleFile` error. The agent round-trips these from `read --json` output —
zero extra work required. On Windows, `inode` is always 0; only `mtime` is used.

### Standard error messages

```text
Error: hash 'xx' not found in src/auth.js
Hint: run `linehash read src/auth.js` to get current hashes
```

```text
Error: hash 'f1' matches 3 lines (lines 2, 14, 67)
Use line-qualified hash: 2:f1, 14:f1, or 67:f1
```

```text
Error: line 2 content changed since last read (expected hash f1, got 3a)
Hint: re-read the file with `linehash read src/auth.js`
```

```text
Error: file was modified since last read (expected mtime 1714000000, got 1714000999)
Hint: re-read with `linehash read src/auth.js`
```

---

## File Loading and Hashing

### Load algorithm

1. Read file bytes
2. Decode as UTF-8; error on invalid bytes
3. Detect newline style:
   - only `\n` → LF
   - only `\r\n` → CRLF
   - mixed → error
4. Split into logical lines, stripping newline terminators
5. Detect trailing newline (does last byte sequence end with a newline?)
6. Compute `xxh32` over each line's raw content (not trimmed)
7. Store full hash internally; derive 2-char short hash for display and anchors
8. Capture `FileMeta` (mtime, inode) from `stat()`

### Hash function

```rust
pub fn full_hash(line: &str) -> u32 {
    xxhash_rust::xxh32::xxh32(line.as_bytes(), 0)
}

pub fn short_hash(line: &str) -> String {
    format!("{:02x}", full_hash(line) & 0xFF)
}
```

Build a lookup index after loading:

```rust
HashMap<String, Vec<usize>>  // short_hash → Vec of 0-based line indexes
```

---

## Anchor Parsing and Resolution

### Parsing rules

- `f1` → `Anchor::Hash { short: "f1" }`
- `2:f1` → `Anchor::LineHash { line: 2, short: "f1" }`
- `2:f1..4:9c` → `RangeAnchor { start: LineHash(2,"f1"), end: LineHash(4,"9c") }`
- normalize uppercase to lowercase
- short hash must be exactly 2 hex chars in v1
- line number must be >= 1
- range must contain exactly one `..`

### Unqualified resolution

1. Look up short hash in the index
2. 0 matches → `HashNotFound`
3. 1 match → resolve
4. 2+ matches → `AmbiguousHash` with all candidate line numbers

### Qualified resolution

1. Verify the referenced line exists
2. Compare current line's short hash to the anchor's short hash
3. Match → resolve
4. Mismatch → `StaleAnchor`; if the hash exists elsewhere, mention it in the error

Never silently retarget to another line. Conservative rejection is always correct here.

### Range resolution

1. Resolve start anchor
2. Resolve end anchor independently
3. Verify `start.index <= end.index`
4. Replace the inclusive slice with one new line (v1 limit)

---

## Atomic Write Strategy

```
1. Load original file permissions/metadata
2. Create NamedTempFile in the same directory as the target
3. Render updated contents: join lines with original newline style;
   append trailing newline only if original had one and file is non-empty
4. Write all bytes to temp file
5. flush() then sync_all()
6. Apply original permissions to temp file
7. persist() (rename) over the target path
```

This guarantees the file is never partially overwritten. On failure at any step,
the original file remains intact.

Rendering edge cases:
- empty result after delete → write zero bytes (no newline)
- original had no trailing newline → preserve that state

---

## Phases

### Phase 1 — POC (Node.js, ~half day)

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

### Phase 2 — Rust Core (~1–2 days)

#### Milestone 0 — Freeze the spec

**Done when:** all decisions (hashing, newline, anchor resolution, content limits) are
written down and unambiguous. No code until this is done.

#### Milestone 1 — Bootstrap

**Done when:** crate compiles, all subcommands parse, `cargo test` runs.

#### Milestone 2 — Document loading and hashing

- [ ] `document.rs`: `Document::load` with UTF-8 decode and newline detection
- [ ] `hash.rs`: `xxh32` over raw line bytes → 2-char hex, deterministic
- [ ] LF files work, CRLF files work, mixed newline files fail clearly
- [ ] Hashes are stable across runs

**Done when:** `Document::load` unit tests pass for LF, CRLF, mixed, empty,
no-trailing-newline.

#### Milestone 3 — `read` and `index`

- [ ] `output.rs`: pretty-print `N:hash| content` format
- [ ] `--json` mode with `file`, `newline`, `trailing_newline`, `mtime`, `inode`, `lines`
- [ ] `index` command: hashes only, no content
- [ ] Snapshot tests with `insta`

**Done when:** pretty output matches spec; `--json` parses as valid JSON; snapshot
tests pass.

#### Milestone 4 — Anchor parsing and resolution

- [ ] `anchor.rs`: parse `f1`, `2:f1`, `2:f1..4:9c`
- [ ] Build `HashMap<String, Vec<usize>>` index on `Document`
- [ ] Unqualified: not found / resolve / ambiguous
- [ ] Qualified: resolve / stale anchor
- [ ] Range: start ≤ end validation

**Done when:** all resolution paths have unit tests; ambiguous and stale cases produce
distinct, actionable error messages.

#### Milestone 5 — Mutation commands + atomic writes

- [ ] `writeback.rs`: tempfile → flush → sync_all → persist
- [ ] Permission preservation on writeback
- [ ] `edit` (single line and range)
- [ ] `insert` after anchor
- [ ] `delete` line
- [ ] Newline style preserved on all mutations
- [ ] Trailing newline state preserved on all mutations

**Done when:** round-trip integration tests pass; no partial writes on simulated failure.

#### Milestone 5b — Concurrency guard

- [ ] `concurrency.rs`: `stat()` helper returning `FileMeta`
- [ ] `Document::load` captures `FileMeta`
- [ ] `--expect-mtime` / `--expect-inode` flags wired to all mutation commands
- [ ] `StaleFile` error with hint

**Done when:** integration test writes a file between `read` and `edit` with
`--expect-mtime`; the edit is rejected with a clear error.

#### Milestone 5c — `verify`, `grep`, `annotate`

- [ ] `commands/verify.rs`: resolve anchors, report ✓/✗ per anchor, exit code
- [ ] `commands/grep.rs`: regex match → anchor output, `--json`
- [ ] `commands/annotate.rs`: substring and `--regex` mode → anchor output, `--expect-one`
- [ ] `regex` crate wired; invalid patterns produce `InvalidPattern` error with position

**Done when:** integration tests cover match/no-match, regex errors, `--json` output
shape, multi-match behavior on `annotate --expect-one`.

#### Milestone 5d — `patch`

- [ ] `patch.rs`: deserialize `PatchFile` from JSON
- [ ] Resolve all anchors against a single document snapshot before any mutation
- [ ] Apply ops in declared order to the in-memory `Vec<LineRecord>`
- [ ] Single atomic write on success; file untouched on any failure
- [ ] `--dry-run` mode: validate and report without writing
- [ ] `PatchFailed` error names the failing op index and reason

**Done when:** patch with one bad anchor in the middle leaves file completely unchanged;
dry-run reports all errors without writing; successful patch round-trips correctly.

#### Milestone 5e — `--dry-run`, `--receipt`, `--audit-log`

- [ ] `receipt.rs`: `Receipt` and `LineChange` structs, JSONL append logic
- [ ] `--dry-run` flag wired to all mutation commands; diff output formatter
- [ ] `--dry-run --json` returns proposed post-mutation Document
- [ ] `--receipt` flag: emit receipt to stdout after successful write
- [ ] `--audit-log <path>`: append receipt to JSONL file
- [ ] `file_hash_before`/`after`: xxh32 over full file bytes

**Done when:** dry-run shows correct diff for edit/insert/delete/swap/indent/patch;
receipt JSON parses and round-trips correctly; audit log is valid JSONL; no entry
appended for a failed operation.

#### Milestone 5f — `swap`, `move`, `indent`

- [ ] `commands/swap.rs`: resolve two anchors, swap content, single atomic write
- [ ] `commands/move.rs`: resolve source + target, remove + insert, single atomic write
- [ ] `commands/indent.rs`: resolve range, apply +/- N spaces, detect space vs tab,
      fail on mixed indentation within range, respect `IndentUnderflow` error
- [ ] All three support `--dry-run` and `--receipt`

**Done when:** swap A→B then B→A is byte-identical to original; indent +N then -N on
any range is byte-identical to original; move is equivalent to a delete + insert at
the target position.

#### Milestone 5g — `find-block`

- [ ] `block.rs`: brace-counting walk for C-family languages
- [ ] `block.rs`: indent-level walk for Python/YAML
- [ ] Language detection from file extension (`.py`, `.yaml`, `.yml` → indent; else brace)
- [ ] Returns `UnbalancedBlock` on mismatch rather than a wrong range
- [ ] Returns `AmbiguousBlockLanguage` when extension is unknown and heuristics disagree
- [ ] `--json` output with start, end, line count, language hint

**Done when:** find-block on a well-formed JS function returns exact open/close brace
lines; find-block on a Python function returns correct indent boundary; unbalanced brace
returns error, not a wrong range.

#### Milestone 5h — `stats`, `from-diff`, `merge-patches`

- [ ] `stats.rs`: count unique hashes, collision pairs, estimate tokens, recommend hash length
- [ ] `diff_import.rs`: parse unified diff format, match hunks to current document lines,
      emit PatchFile JSON; fail on hunk mismatch
- [ ] `merge_patches.rs`: resolve both patches, detect overlapping anchors, emit merged
      PatchFile or conflict report with full detail
- [ ] All three: `--json` output

**Done when:** `stats` output is stable for snapshot tests; `from-diff` round-trip
(diff → patch → apply) produces byte-identical result to `git apply`; `merge-patches`
detects all conflicts and includes non-conflicting ops in partial merge output.

#### Milestone 5i — `watch`, `explode`, `implode`

- [ ] `watch.rs`: `notify` crate integration, hash diff on change event, `--once` default
- [ ] `watch.rs`: `--json` streaming newline-delimited events
- [ ] `explode.rs`: decompose file → `{NNNN}_{hash}.txt` + `.meta.json`
- [ ] `explode.rs`: `--force` flag for overwrite
- [ ] `implode.rs`: sort by filename, validate `.meta.json`, reassemble, atomic write
- [ ] `implode.rs`: detect dirty non-linehash files and fail with `ImplodeDirty`
- [ ] Explode → implode round-trip is byte-identical to original on all fixture files

**Done when:** watch detects a file save within 500ms; explode+implode round-trip passes
on LF, CRLF, no-trailing-newline, and empty fixtures; dirty implode dir is rejected.

#### Milestone 6 — Error hardening and polish

- [ ] Every `LinehashError` variant includes a recovery hint
- [ ] Binary file detection (reject gracefully)
- [ ] Consistent blank line handling
- [ ] README examples verified end-to-end

#### Milestone 7 — Release prep

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` (all unit + integration + snapshot) passes
- [ ] `cargo install linehash` smoke-tested

---

### Phase 3 — Post-v1 Roadmap

Deferred intentionally. Ship v1 first.

1. Multi-line insert and replace
2. `linehash diff` — show pending changes vs last read
3. `linehash undo <file>` — implemented by reading `.linehash/audit.jsonl` in reverse
   and emitting the inverse patch; requires no additional state beyond what v1 already
   writes
4. Optional relaxed anchor recovery for moved lines
5. Support for longer hashes when 2-char collision rate is too high
6. Benchmark harness: `str_replace` failure rate vs `linehash` failure rate
7. `linehash mcp --stdio` — MCP server exposing all commands as structured tools;
   thin dispatch layer over existing Rust functions (~200 lines); single persistent
   process eliminates per-operation spawn overhead
8. `linehash session start/end` — stateful snapshot transactions; natural fit once
   `patch.rs` exists (sessions are just patch accumulation)
9. Git-aware annotation overlay (`read --git-aware`) — lines new since HEAD marked
   `[+]`; recently modified lines flagged as "hot anchors"
10. `watch` daemon / persistent Unix socket mode — agent framework subscribes once,
    receives all change events without spawning a process per edit
11. Cross-file patch — PatchFile referencing multiple files, applied transactionally
    using a two-phase write strategy

---

## Test Strategy

### Unit tests (in each module)

- `hash.rs`: hash determinism, 2-char formatting, empty line behavior
- `document.rs`: newline detection (LF, CRLF, mixed), trailing newline detection,
  UTF-8 error, empty file
- `anchor.rs`: parse all three anchor forms, invalid inputs, normalize uppercase,
  unqualified resolution (not found / resolve / ambiguous), qualified resolution
  (match / stale), range validation
- `writeback.rs`: LF round-trip, CRLF round-trip, no-trailing-newline preserved,
  empty result
- `block.rs`: brace balancing, indent balancing, unbalanced input, ambiguous language
- `stats.rs`: collision counting, token estimate formula, hash length advice thresholds
- `diff_import.rs`: hunk matching, mismatch detection, file mismatch detection
- `receipt.rs`: JSONL serialization, append ordering, no-append on failure

### Integration tests (via `assert_cmd`)

- `read` pretty output
- `read --json` (valid JSON, correct shape, includes mtime/inode)
- `read --anchor --context` → only neighborhood lines, target marked with →
- `index` pretty output
- `index --json`
- edit single line → correct new content
- edit range → correct collapsed content
- insert after anchor → line at correct position
- delete line → removed, others unchanged
- not found error with hint
- ambiguous hash error with candidate list
- stale qualified anchor error with hint
- CRLF file preserved after edit
- no-trailing-newline file preserved after edit
- empty file behavior on read/index
- invalid UTF-8 rejection
- mixed newline rejection
- binary file rejection
- `verify` all valid → exit 0, ✓ lines
- `verify` with one stale anchor → exit 1, mixed ✓/✗ report
- `verify` with ambiguous anchor → exit 1, ? line with candidates
- `verify --json` → structured array output
- `grep` regex match → anchor format output
- `grep` no match → empty output, exit 0
- `grep` invalid regex → `InvalidPattern` error
- `grep --json` → filtered lines array
- `annotate` substring match → anchor output
- `annotate` no match → helpful message
- `annotate --expect-one` with multiple matches → error
- `patch` all ops valid → file updated correctly, single write
- `patch` op 2 of 3 stale → file untouched, `PatchFailed` names op index 2
- `patch --dry-run` → no file write, validation report only
- `patch --dry-run` with errors → all errors reported, not just first
- `edit --expect-mtime` with correct mtime → succeeds
- `edit --expect-mtime` with wrong mtime → `StaleFile` error, file untouched
- `patch` with `--expect-mtime` → concurrency guard applies to whole transaction
- `swap` two distinct lines → contents transposed, hashes updated
- `swap` with `--dry-run` → no write, diff shown
- `swap` same-line anchor → `SwapSameLine` error
- `move` line to new position → correct result, single write
- `indent` range +4 spaces → all lines in range have 4 more leading spaces
- `indent` range -2 tabs → all lines dedented, tab-aware
- `indent` dedent underflow → `IndentUnderflow` names the offending line
- `indent` +N then -N round-trip → byte-identical to original
- `indent` with `--dry-run` → full before/after for every line in range
- `find-block` on JS function → exact brace-balanced range returned
- `find-block` on Python function → indent-delimited range returned
- `find-block` on unbalanced braces → `UnbalancedBlock` error, no range
- `find-block --json` → parseable object with start, end, line count, language
- `stats` on fixture → correct line count, collision count, token estimate
- `stats --json` → parseable object, stable schema
- `from-diff` with valid unified diff → emits correct PatchFile JSON
- `from-diff` piped to `patch` → byte-identical to `git apply` result
- `from-diff` with mismatched hunk → `DiffHunkMismatch` error, no output
- `merge-patches` no conflicts → merged patch applies cleanly
- `merge-patches` with conflict → all conflicts reported, non-conflicting ops in output
- `watch --once` detects file save → emits change report and exits
- `watch --once --json` → parses as newline-delimited JSON events
- `watch` on non-existent file → clear error immediately
- `explode` → correct file count, filenames match `{NNNN}_{hash}.txt` pattern
- `implode` after `explode` → byte-identical to original (all fixture types)
- `implode` with dirty dir → `ImplodeDirty` error
- `explode` existing dir without `--force` → `ExplodeTargetExists` error
- `--dry-run` on edit → no write, diff shown, exit 0
- `--dry-run` on patch → no write, all op diffs shown
- `--dry-run --json` → proposed Document object parseable
- `--receipt` on edit → JSON receipt printed, contains correct before/after hashes
- `--audit-log` on two sequential edits → JSONL file has two valid receipt entries
- `--audit-log` entry for failed edit → no entry appended

### Snapshot tests (via `insta`)

- `read` pretty output on the reference fixture
- `index` pretty output on the reference fixture
- `read --json` output (stable schema)
- `index --json` output (stable schema)
- `stats --json` output (stable schema)
- `verify --json` output (stable schema)
- each error message form (ensure hint text doesn't regress)

---

## Acceptance Criteria for v1

Ship v1 only when **all** of the following are true:

- [ ] A unique unqualified hash resolves correctly and edits the intended line
- [ ] A qualified anchor edits the intended line only when the hash still matches
- [ ] Ambiguous hashes never silently choose a target
- [ ] Stale anchors never silently choose a target
- [ ] CRLF files remain CRLF after any mutation
- [ ] Files without trailing newline keep that state after any mutation
- [ ] `read` and `index` output is stable enough for agent consumption
- [ ] All mutation commands are atomic: file is intact on any failure
- [ ] Every user-facing error includes a recovery hint
- [ ] `verify` exits non-zero if any anchor fails; reports all failures, not just first
- [ ] `grep` and `annotate` output is directly usable as anchor input to edit/insert/delete
- [ ] `patch` with any failing anchor leaves file byte-for-byte identical to before
- [ ] `--expect-mtime` on any mutation rejects stale files before touching them
- [ ] `read --context --anchor` reduces output to the anchor neighborhood only
- [ ] `swap` of A→B then B→A is byte-identical to the original file
- [ ] `indent` +N then -N on any range is byte-identical to the original file
- [ ] `find-block` never returns a range that doesn't balance from the anchor line
- [ ] `from-diff` output, when applied via `patch`, produces byte-identical result to `git apply`
- [ ] `merge-patches` never silently drops conflicting ops — all conflicts are reported
- [ ] `explode` → `implode` is byte-identical for all fixture files
- [ ] `--dry-run` never writes any bytes to the target file under any circumstances
- [ ] `--audit-log` never appends an entry for an operation that did not complete successfully
- [ ] `watch --once` exits within 1 second of a file save event
- [ ] `cargo test` passes clean
- [ ] `cargo clippy --all-targets -- -D warnings` passes clean

---

## Integration with Claude Code

Add to any project's `CLAUDE.md`:

```markdown
## File Editing Protocol

Always use linehash instead of str_replace for editing existing files.

1. Budget:    `linehash stats <file>`                         — run first on files > 500 lines
2. Read:      `linehash read <file>`                          — note the N:hash on each line
   Focused:   `linehash read <file> --anchor <N:hash> --context 5`  — neighborhood only
3. Find:      `linehash grep <file> "<pattern>"`              — locate by regex
   Reverse:   `linehash annotate <file> "<substring>"`        — locate by content description
   Boundary:  `linehash find-block <file> <anchor>`           — discover block start/end
4. Verify:    `linehash verify <file> <anchor> [<anchor>...]` — pre-flight all anchors
5. Preview:   add `--dry-run` to any mutation command         — inspect before committing
6. Edit:      `linehash edit <file> <N:hash> "<new content>"`
7. Insert:    `linehash insert <file> <N:hash> "<new line>"`
8. Reorder:   `linehash swap <file> <anchor-a> <anchor-b>`
              `linehash move <file> <anchor> after <anchor-b>`
9. Indent:    `linehash indent <file> <start>..<end> +4`      — or -N to dedent
10. Batch:    `linehash patch <file> patch.json`              — multi-op atomic transaction
    From diff:`git diff HEAD <file> | linehash from-diff <file> - | linehash patch <file> -`
11. Audit:    add `--audit-log .linehash/audit.jsonl`         — full history of every change

Never reproduce old content. Reference the hash anchor only.
If you see "Stale hash":       re-read the file first.
If you see "Ambiguous hash":   use N:hash form, e.g. 14:f1 instead of f1.
If you see "Stale file":       another process edited the file — re-read before continuing.
If you see "Unbalanced block": use an explicit range anchor instead of find-block.

For multi-step edits, prefer patch files over sequential edit commands.
Always pass --expect-mtime from the read output when editing files that others may touch.
Run `linehash stats` before reading files larger than ~500 lines to decide whether
a full read or a --context read is more token-efficient.
```

---

## Why Build This First

- Smallest scope of any tool in the suite — core is ~300 lines of Rust
- Zero heavy dependencies — only xxhash, clap, and small utility crates
- Directly measurable improvement: `str_replace` failure rate vs `linehash`
- Immediate value: works on any Claude Code project from day one
- Every feature compounds — `patch` + `verify` + `--dry-run` together form a
  complete safe editing pipeline that no existing tool provides
- Fastest to ship: **~5 days total** — but correctness over speed; freeze the spec first

---

## Timeline

| Phase | Duration |
|---|---|
| 0 — Spec freeze | 1–2 hours |
| 1 — POC | 0.5 day |
| 2 — Rust core (milestones 1–5) | 1.5–2 days |
| 2b — First power wave (milestones 5b–5d) | 1 day |
| 2c — Second power wave (milestones 5e–5i) | 1.5 days |
| **Total** | **~5 days** |

Milestones 5e–5i can be developed in parallel with each other after milestone 5d ships.
Each is independently testable and does not block the others.

The power features add roughly two days to the timeline but deliver disproportionate
value for agent workflows:
- `patch` alone eliminates the most common multi-edit failure mode
- `--context` alone cuts token cost for large-file edits by 10–100x
- `--dry-run` + `--receipt` + `--audit-log` form a complete observability layer
- `swap` + `move` + `indent` eliminate the most dangerous multi-step mutation patterns
- `find-block` makes range operations safe without requiring the agent to guess boundaries
- `stats` prevents token waste on large files before it happens
- `from-diff` + `merge-patches` make linehash interoperable with the entire diff ecosystem
- `verify` + concurrency guard make the tool safe for concurrent agent use
