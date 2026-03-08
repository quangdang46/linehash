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

---

## Module Implementation Specs

This section provides exhaustive implementation contracts for every module in
`crates/core/`. Each module entry lists: purpose, public API surface, invariants,
error conditions, edge cases, and integration notes. Write code only after reading
the relevant section here.

---

### `hash.rs` — Line Hashing

#### Purpose

Produce a stable, deterministic 2-character hexadecimal hash for any line of text.
The hash is the single most critical primitive in the tool: all anchor safety
guarantees depend on its collision properties and stability across builds.

#### Public API

```rust
/// Compute the full xxh32 hash of a raw line (no newline terminator).
/// Seed is always 0 for stability across platforms and Rust versions.
pub fn full_hash(line: &str) -> u32

/// Derive the canonical 2-char lowercase hex short hash.
/// Takes the lowest byte of full_hash and formats as two hex digits.
pub fn short_hash(line: &str) -> String

/// Format a u32 full hash as a 2-char short hash string.
/// Used internally when the full hash is already computed.
pub fn short_from_full(full: u32) -> String

/// Return true if two lines share the same short hash (collision).
pub fn collides(a: &str, b: &str) -> bool
```

#### Invariants

- `short_hash(x)` must equal `short_hash(x)` for any `x` across all platforms,
  Rust versions, and build modes.
- `short_hash` always returns exactly 2 lowercase hex characters (`[0-9a-f]{2}`).
- The empty string `""` has a defined, stable hash (do not special-case it).
- Whitespace-only lines like `"  "`, `"\t"`, `"    "` each hash distinctly from `""`.
- `short_hash("  return decoded")` ≠ `short_hash("return decoded")` in general —
  leading whitespace is intentionally included.

#### Collision Probability

With 256 possible 2-char hex values (00–ff), the expected collision probability
for a file of N distinct lines follows the birthday paradox:

```
P(at least one collision) ≈ 1 - exp(-N*(N-1) / (2*256))
```

| File Lines | P(collision) |
|---|---|
| 10  | ~17% |
| 20  | ~54% |
| 32  | ~82% |
| 50  | ~97% |
| 100 | ~100% |

This means collisions are **expected** and **normal**. The tool handles them by
requiring users to qualify ambiguous anchors with a line number. The `stats`
command helps users understand their specific file's collision situation.

#### Extending to 3- or 4-char Hashes (Post-v1)

The hash length is currently fixed at 2 characters (1 byte). If collision rates
are too high for a given file, v2 may support `--hash-len 3` or `--hash-len 4`.
Keep the hash derivation logic isolated in `hash.rs` to make this extension clean.

The extension plan:
- Add `pub fn short_hash_n(line: &str, n: usize) -> String`
- `n=2` → lowest byte → 2 hex chars (current default)
- `n=3` → lowest 12 bits → 3 hex chars
- `n=4` → lowest 2 bytes → 4 hex chars
- `stats.rs` sets `hash_length_advice` to recommend the minimum N that reduces
  collision probability below 1%

#### Unit Tests Required

```
hash::test_empty_line_stable
hash::test_whitespace_only_stable
hash::test_leading_space_differs_from_no_space
hash::test_trailing_space_differs_from_no_space
hash::test_short_hash_always_2_chars
hash::test_short_hash_always_lowercase_hex
hash::test_deterministic_across_calls
hash::test_crlf_content_stripped_before_hashing  (caller strips; hash never sees \r\n)
hash::test_collides_returns_true_on_collision
hash::test_collides_returns_false_on_distinct
hash::test_full_hash_seed_zero
```

---

### `document.rs` — File Loading and the Document Model

#### Purpose

Load a source file from disk into an in-memory `Document` struct. Detect and
validate newline style. Build the per-line hash index. Capture filesystem
metadata for the concurrency guard. Render the document back to bytes for writing.

#### Public API

```rust
impl Document {
    /// Load a file from disk.
    /// Fails if: file is not valid UTF-8, file has mixed newlines,
    ///           file is not accessible.
    pub fn load(path: &Path) -> Result<Document, LinehashError>

    /// Construct a Document from an already-decoded string (for tests).
    pub fn from_str(path: &Path, content: &str) -> Result<Document, LinehashError>

    /// Build a HashMap<short_hash, Vec<0-based-index>> lookup index.
    /// Called once by load(); accessible for resolution logic.
    pub fn build_index(&self) -> HashMap<String, Vec<usize>>

    /// Render lines back to bytes using the original newline style.
    /// Preserves trailing newline state from load.
    pub fn render(&self) -> Vec<u8>

    /// Return the number of lines.
    pub fn len(&self) -> usize

    /// Return true if the document has no lines.
    pub fn is_empty(&self) -> bool
}
```

#### Load Algorithm (Detailed)

```
Step 1 — Read bytes
  - std::fs::read(path)
  - If fails → LinehashError::Io

Step 2 — UTF-8 decode
  - String::from_utf8(bytes)
  - If fails → LinehashError::InvalidUtf8
  - Do NOT use lossy conversion; an edit tool must refuse corrupted input

Step 3 — Binary file heuristic
  - Scan first 8000 bytes for null bytes (0x00)
  - If found → LinehashError::BinaryFile
  - Rationale: binary files are almost certainly a user mistake

Step 4 — Newline detection
  - Count \n occurrences
  - Count \r\n occurrences
  - If only \n present (or no newlines) → NewlineStyle::Lf
  - If only \r\n present → NewlineStyle::Crlf
  - If both → LinehashError::MixedNewlines
    Hint: "run `dos2unix <file>` or `unix2dos <file>` to normalize first"

Step 5 — Detect trailing newline
  - If string is empty → trailing_newline = false
  - If last char is \n → trailing_newline = true
  - Else → trailing_newline = false

Step 6 — Split into logical lines
  - For LF: split_terminator('\n')
  - For CRLF: split on "\r\n" using split_terminator or manual scan
  - Result is a Vec<&str> with no newline terminators
  - Empty file → empty Vec (zero lines), not Vec of one empty string

Step 7 — Build LineRecord for each line
  - line.number = 1-based position
  - line.content = owned String (cloned from slice)
  - line.full_hash = hash::full_hash(&line.content)
  - line.short_hash = hash::short_from_full(line.full_hash)

Step 8 — Capture FileMeta
  - std::fs::metadata(path) → mtime_secs, mtime_nanos, inode
  - On Windows, inode = 0 (not available through std)
  - mtime from SystemTime → unix seconds via duration_since(UNIX_EPOCH)

Step 9 — Return Document
```

#### Render Algorithm (Detailed)

```
If self.lines is empty:
  - Return empty Vec<u8>
  - (regardless of trailing_newline; no bytes to write)

Build output string:
  - join lines with newline separator:
    - NewlineStyle::Lf  → "\n"
    - NewlineStyle::Crlf → "\r\n"
  - if self.trailing_newline → append newline terminator
  - if !self.trailing_newline → do not append

Convert to bytes with str::as_bytes() or String::into_bytes()
```

#### Edge Cases

| Input | Expected behavior |
|---|---|
| Empty file (0 bytes) | `Document { lines: [], trailing_newline: false }` |
| Single line, no trailing newline | `lines: [line]`, `trailing_newline: false` |
| Single line, with trailing newline | `lines: [line]`, `trailing_newline: true` |
| File with only `\n` | `lines: [""]`, `trailing_newline: true` |
| File with only whitespace lines | All lines hashed including whitespace |
| 1MB file | Loads into memory without issue |
| 10MB file | Loads; emit warning in `stats` if read tokens will exceed context window |
| Windows CRLF file on Linux | Detected and preserved |
| File with BOM (`\xEF\xBB\xBF`) | Treated as content of line 1; no special handling in v1 |
| File with null bytes | Rejected as binary |
| Non-UTF-8 bytes (e.g. Latin-1) | Rejected with `InvalidUtf8` |

#### Unit Tests Required

```
document::test_load_lf_simple
document::test_load_crlf_simple
document::test_load_mixed_newlines_fails
document::test_load_empty_file
document::test_load_single_line_no_trailing_newline
document::test_load_single_line_with_trailing_newline
document::test_load_whitespace_only_lines
document::test_load_invalid_utf8_fails
document::test_load_binary_file_fails
document::test_render_lf_round_trip
document::test_render_crlf_round_trip
document::test_render_no_trailing_newline_preserved
document::test_render_trailing_newline_preserved
document::test_render_empty_document_is_empty_bytes
document::test_build_index_unique_hashes
document::test_build_index_collision_has_multiple_entries
document::test_line_numbers_are_1_based
document::test_filemeta_captured
```

---

### `anchor.rs` — Anchor Parsing and Resolution

#### Purpose

Parse anchor strings from CLI arguments into typed values. Resolve anchors against
a loaded Document. Produce typed, actionable errors on every failure path.

#### Public API

```rust
/// Parse a single anchor string into an Anchor enum.
/// Accepts "f1", "2:f1". Normalizes uppercase to lowercase.
pub fn parse_anchor(s: &str) -> Result<Anchor, LinehashError>

/// Parse a range anchor string. Accepts "2:f1..4:9c".
pub fn parse_range(s: &str) -> Result<RangeAnchor, LinehashError>

/// Resolve an Anchor against a Document.
/// Returns the resolved ResolvedLine on success.
pub fn resolve(anchor: &Anchor, doc: &Document, index: &HashMap<String, Vec<usize>>)
    -> Result<ResolvedLine, LinehashError>

/// Resolve a RangeAnchor against a Document.
/// Returns (start_resolved, end_resolved), validated start <= end.
pub fn resolve_range(range: &RangeAnchor, doc: &Document, index: &HashMap<String, Vec<usize>>)
    -> Result<(ResolvedLine, ResolvedLine), LinehashError>

/// Resolve multiple anchors in bulk (for verify, patch).
/// Returns Vec<Result<ResolvedLine, LinehashError>> — one per anchor.
/// Does NOT short-circuit; collects all errors.
pub fn resolve_all(anchors: &[Anchor], doc: &Document, index: &HashMap<String, Vec<usize>>)
    -> Vec<Result<ResolvedLine, LinehashError>>
```

#### Parsing Rules (Detailed)

```
Input: raw string from CLI argument

Normalize:
  - Convert to lowercase (uppercase hex is accepted, stored lowercase)
  - Trim leading and trailing whitespace (defensive; CLI should not pass whitespace)

Detect form:
  - Contains ".." → parse as range
    - Split on first ".."
    - Parse left as Anchor, parse right as Anchor
    - Both must be LineHash form (qualified) in v1
  - Contains ":" → parse as LineHash
    - Split on first ":"
    - Left is decimal line number (>= 1)
    - Right is 2-char hex hash
  - No ":" and no ".." → parse as unqualified Hash
    - Must be exactly 2 hex chars

Validation:
  - Short hash must match /^[0-9a-f]{2}$/
  - Line number must be parseable as usize and >= 1
  - Range start and end must both be qualified (LineHash form)
```

#### Resolution Logic (Detailed)

```
Unqualified Hash { short }:
  1. Look up short in index
  2. matches.len() == 0 → HashNotFound { hash: short }
  3. matches.len() == 1 → resolve to that index
  4. matches.len() >= 2 → AmbiguousHash {
       hash: short,
       count: matches.len(),
       lines: 1-based line numbers,
       example: matches[0] + 1  (for hint text)
     }

Qualified LineHash { line, short }:
  1. Convert 1-based line to 0-based index: idx = line - 1
  2. If idx >= doc.lines.len() → InvalidAnchor (line out of range)
  3. actual_hash = doc.lines[idx].short_hash.clone()
  4. If actual_hash == short → resolve to ResolvedLine { index: idx, line_no: line, short_hash }
  5. If actual_hash != short:
     - Look up `short` in index to see if it exists elsewhere
     - StaleAnchor {
         line,
         expected: short,
         actual: actual_hash,
         // optional: mention if `short` exists at another line
       }

Range resolution:
  1. resolve_start = resolve(&range.start, ...)
  2. resolve_end   = resolve(&range.end,   ...)
  3. If either fails → propagate the error (start failure takes precedence)
  4. If start.index > end.index → InvalidRange(...)
  5. Return (resolve_start, resolve_end)
```

#### Unit Tests Required

```
anchor::test_parse_unqualified_lowercase
anchor::test_parse_unqualified_uppercase_normalizes
anchor::test_parse_qualified_basic
anchor::test_parse_qualified_uppercase_normalizes
anchor::test_parse_range_basic
anchor::test_parse_invalid_hash_length_3_chars_fails
anchor::test_parse_invalid_hash_non_hex_fails
anchor::test_parse_line_number_zero_fails
anchor::test_parse_line_number_negative_fails
anchor::test_resolve_unqualified_not_found
anchor::test_resolve_unqualified_single_match
anchor::test_resolve_unqualified_ambiguous
anchor::test_resolve_qualified_match
anchor::test_resolve_qualified_stale
anchor::test_resolve_qualified_out_of_range_line
anchor::test_resolve_range_valid
anchor::test_resolve_range_start_after_end_fails
anchor::test_resolve_all_collects_all_errors
anchor::test_resolve_all_all_success
```

---

### `writeback.rs` — Atomic File Rewrite

#### Purpose

Safely replace a file's contents using a temp-file-then-rename strategy.
This module must never leave a file in a partially written state.
It is the single point of mutation for all file-modifying commands.

#### Public API

```rust
/// Atomically write new_content to path.
/// Preserves file permissions from the original file.
/// Creates a NamedTempFile in the same directory (ensures same filesystem).
/// Flushes, syncs, then renames over the original.
pub fn atomic_write(path: &Path, new_content: &[u8]) -> Result<(), LinehashError>

/// Read file permissions from path, apply to temp_path.
/// No-op on platforms where permission preservation is not supported.
pub fn preserve_permissions(original: &Path, temp: &Path) -> Result<(), LinehashError>
```

#### Write Algorithm

```
1. Resolve original path to absolute (prevents race on cwd change)
2. stat(path) → capture mode bits (Unix) / ACLs (Windows — omit in v1)
3. parent_dir = path.parent().unwrap_or(Path::new("."))
4. tmp = NamedTempFile::new_in(parent_dir)?
   - CRITICAL: same directory = same filesystem = rename is atomic
   - Cross-filesystem rename is not atomic; wrong parent would break this
5. tmp.write_all(new_content)?
6. tmp.flush()?
7. tmp.as_file().sync_all()?
8. preserve_permissions(path, tmp.path())?
9. tmp.persist(path)?  — std::fs::rename under the hood
   - On Windows, persist() uses MoveFileExW with MOVEFILE_REPLACE_EXISTING
   - On Unix, rename(2) is atomic at the syscall level
10. Return Ok(())
```

#### Failure Modes

| Step | Failure | File State |
|---|---|---|
| NamedTempFile creation | Disk full, permissions | Original intact |
| write_all | Disk full mid-write | Temp file incomplete, original intact |
| flush/sync_all | I/O error | Temp file incomplete, original intact |
| preserve_permissions | Permission denied on temp | Original intact |
| persist/rename | Permission denied on parent | Original intact |

The `tmp` variable being dropped on any failure path automatically deletes the
temp file via `NamedTempFile`'s `Drop` impl (unless `persist()` succeeded).

#### Permission Preservation

On Unix:
```rust
use std::os::unix::fs::PermissionsExt;
let mode = std::fs::metadata(original)?.permissions().mode();
std::fs::set_permissions(temp, std::fs::Permissions::from_mode(mode))?;
```

On Windows: No-op in v1. NTFS ACLs are not preserved. Mark as v2 work item.

#### Edge Cases

| Case | Behavior |
|---|---|
| Target file deleted before persist | `persist` fails; return Io error |
| Target is a symlink | Rename replaces the symlink target, not the symlink itself |
| Target directory is read-only | NamedTempFile creation fails with clear Io error |
| New content is zero bytes | Write succeeds; zero-byte file produced |
| New content is exactly 4GB | Uses streaming write_all; no in-memory size limit |

#### Unit Tests Required

```
writeback::test_basic_round_trip
writeback::test_lf_content_preserved
writeback::test_crlf_content_preserved
writeback::test_empty_content_writes_empty_file
writeback::test_permissions_preserved_unix
writeback::test_original_intact_if_write_fails  (simulate via tiny disk)
writeback::test_same_directory_used_for_temp
```

---

### `output.rs` — Output Formatting

#### Purpose

Format Document and related data structures for human-readable (pretty) and
machine-readable (`--json`) output. This module owns every user-visible string
that is not an error message.

#### Public API

```rust
/// Print a Document in pretty "N:hash| content" format to stdout.
pub fn print_read(doc: &Document, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print a Document in --json format.
pub fn print_read_json(doc: &Document, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print focused neighborhood: only lines within ±context of anchor line(s).
pub fn print_read_context(
    doc: &Document,
    anchors: &[ResolvedLine],
    context: usize,
    writer: &mut impl Write
) -> Result<(), std::io::Error>

/// Print index (hashes only, no content) in pretty format.
pub fn print_index(doc: &Document, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print index in --json format.
pub fn print_index_json(doc: &Document, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print a verify result array.
pub fn print_verify(results: &[VerifyResult], writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print a verify result array in --json format.
pub fn print_verify_json(results: &[VerifyResult], writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print a stats report.
pub fn print_stats(stats: &FileStats, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print stats in --json format.
pub fn print_stats_json(stats: &FileStats, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Print a dry-run diff (before/after lines).
pub fn print_dry_run_diff(changes: &[LineChange], writer: &mut impl Write) -> Result<(), std::io.Error>

/// Print a receipt in --json format.
pub fn print_receipt_json(receipt: &Receipt, writer: &mut impl Write) -> Result<(), std::io::Error>
```

#### Pretty Output Format Spec

**read**:
```
{N}:{hash}| {content}
```
- `N` is right-aligned to the width of the maximum line number in the file
- Hash is always 2 chars
- Separator is `|` with one space after it
- Content is printed verbatim; no escaping

Example — 3-digit line count file:
```
  1:a3| function verifyToken(token) {
  2:f1|   const decoded = jwt.verify(token, SECRET)
 10:0e|   if (!decoded.exp) throw new TokenError('missing expiry')
100:9c|   return decoded
```

**read --anchor --context**:
- Print only lines in the neighborhood
- Mark the anchor line with `→` before the N:hash
- Suppress `→` for context lines (they use a space instead)
- Multiple anchor neighborhoods are merged if they overlap; deduplicated if they share lines
- A separator line (`...`) is printed between non-contiguous neighborhoods

Example:
```
   11:2a|   // previous middleware
   12:b3|   if (!token) return res.status(401).send()
   13:0e|   token = token.split(' ')[1]
→  14:f1|   const decoded = jwt.verify(token, SECRET)
   15:9c|   if (!decoded.exp) throw new TokenError('missing expiry')
   16:a1|   return decoded
   17:b2| }
```

**index**:
```
{N}:{hash}
```
One per line, no padding, no pipe.

**verify**:
```
✓  {anchor}  resolves → "{content}"
✗  {anchor}  {error message}
?  {anchor}  AMBIGUOUS — matches lines {n}, {n} — use {n}:{hash} or {n}:{hash}
```

**dry-run diff**:
```
Would change line {N}:
  - "{before}"
  + "{after}"
No file was written.
```

For insert:
```
Would insert after line {N}:
  + "{content}"
No file was written.
```

For delete:
```
Would delete line {N}:
  - "{content}"
No file was written.
```

#### JSON Output Format Spec

**read --json**:
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

**index --json**:
```json
{
  "file": "src/auth.js",
  "lines": [
    { "n": 1, "hash": "a3" },
    { "n": 2, "hash": "f1" }
  ]
}
```

**verify --json**:
```json
[
  { "anchor": "2:f1", "status": "ok",        "line_no": 2,  "content": "..." },
  { "anchor": "99:xx", "status": "not_found", "error": "hash 'xx' not found" },
  { "anchor": "f1",   "status": "ambiguous",  "candidates": [2, 14] }
]
```

**stats --json**:
```json
{
  "file": "src/auth.js",
  "line_count": 847,
  "unique_hashes": 831,
  "collision_count": 16,
  "collision_pairs": [[2, 14], [67, 203]],
  "estimated_read_tokens": 4200,
  "hash_length_advice": 2,
  "suggested_context_n": 8
}
```

**dry-run --json** (proposed post-mutation Document in `read --json` schema):
```json
{
  "dry_run": true,
  "file": "src/auth.js",
  "newline": "lf",
  "trailing_newline": true,
  "lines": [ ... ]
}
```

**receipt --json**:
```json
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
```

#### Writer Abstraction

All print functions accept `&mut impl Write`. In production, pass `std::io::stdout()`.
In tests, pass a `Vec<u8>` or `std::io::Cursor<Vec<u8>>` for capture and assertion.
This makes every output function unit-testable without spawning a process.

#### Unit Tests Required

```
output::test_read_format_single_line
output::test_read_format_line_number_padding_2_digits
output::test_read_format_line_number_padding_3_digits
output::test_read_context_marks_anchor_line
output::test_read_context_suppresses_other_lines
output::test_read_context_separator_between_neighborhoods
output::test_read_context_multiple_anchors_merged
output::test_index_format_no_content
output::test_verify_format_ok
output::test_verify_format_error
output::test_verify_format_ambiguous
output::test_dry_run_diff_edit
output::test_dry_run_diff_insert
output::test_dry_run_diff_delete
output::test_read_json_valid
output::test_read_json_schema_stable  (snapshot)
output::test_index_json_valid
output::test_verify_json_valid
output::test_stats_json_valid
output::test_receipt_json_valid
output::test_dry_run_json_proposed_document
```

---

### `concurrency.rs` — Optimistic Concurrency Guard

#### Purpose

Provide helpers for reading filesystem metadata and comparing it against
expected values supplied on the command line. Enables safe concurrent use of the
tool across multiple agents or processes editing the same file.

#### Public API

```rust
/// Read mtime and inode from the filesystem.
pub fn read_meta(path: &Path) -> Result<FileMeta, LinehashError>

/// Compare actual metadata against an expected mtime and/or inode.
/// Returns Ok(()) if all specified expectations are met.
/// Returns Err(StaleFile) if any expectation fails.
pub fn check_guard(
    actual: &FileMeta,
    expect_mtime: Option<i64>,
    expect_inode: Option<u64>,
) -> Result<(), LinehashError>

/// Format an mtime as a display string for error messages.
pub fn format_mtime(secs: i64, nanos: u32) -> String
```

#### Guard Algorithm

```
Called at the top of every mutation command handler, before Document::load.

1. If neither --expect-mtime nor --expect-inode was provided → skip (no guard)
2. stat(path) → actual FileMeta
3. If --expect-mtime provided:
   - Compare actual.mtime_secs to expected
   - Mismatch → StaleFile { expected: format(expected), actual: format(actual) }
4. If --expect-inode provided AND actual.inode != 0:
   - Compare actual.inode to expected
   - Mismatch → StaleFile (with inode context in the hint)
5. Return Ok(())
```

#### Platform Notes

| Platform | mtime precision | inode availability |
|---|---|---|
| Linux (ext4) | 1 nanosecond | yes |
| macOS (APFS) | 1 nanosecond | yes |
| macOS (HFS+) | 1 second | yes |
| Windows (NTFS) | 100 nanosecond | no (use 0) |
| NFS mounts | may be truncated | unreliable |

Agents using `--expect-mtime` should use the full `mtime` + `mtime_nanos` values
from `read --json`. On 1-second-resolution filesystems, back-to-back edits within
the same second may not be detected. For those cases, `--expect-inode` is a
complementary guard.

#### Unit Tests Required

```
concurrency::test_check_guard_no_expectations_passes
concurrency::test_check_guard_mtime_match_passes
concurrency::test_check_guard_mtime_mismatch_fails
concurrency::test_check_guard_inode_match_passes
concurrency::test_check_guard_inode_mismatch_fails
concurrency::test_check_guard_inode_zero_skipped_on_windows
concurrency::test_format_mtime_readable
```

---

### `receipt.rs` — Operation Receipts and Audit Log

#### Purpose

Record the exact outcome of every successful mutation. Receipts enable:
- Agent-side verification that the right lines were changed
- Foundation for `undo` in v2 (apply inverse operations in reverse log order)
- Debugging: full before/after content and hash state

#### Public API

```rust
/// Build a Receipt from operation metadata and per-line changes.
pub fn build_receipt(
    op: &str,
    file: &Path,
    changes: Vec<LineChange>,
    bytes_before: &[u8],
    bytes_after: &[u8],
) -> Receipt

/// Print a receipt to a writer as pretty-printed JSON.
pub fn print_receipt(receipt: &Receipt, writer: &mut impl Write) -> Result<(), std::io::Error>

/// Append a receipt to a JSONL audit log file.
/// Creates the file (and parent directories) if they do not exist.
/// Each receipt is one line (no pretty-print in the log).
pub fn append_to_audit_log(receipt: &Receipt, log_path: &Path) -> Result<(), LinehashError>

/// Compute xxh32 over a byte slice.
pub fn file_hash(bytes: &[u8]) -> u32
```

#### Receipt Population Logic

For each mutation command, build `Vec<LineChange>` as follows:

| Command | Changes produced |
|---|---|
| `edit` single | One `Modified` change |
| `edit` range | One `Modified` change (range collapsed to one line) |
| `insert` | One `Inserted` change |
| `delete` | One `Deleted` change |
| `swap` | Two `Modified` changes (one per line) |
| `move` | One `Deleted` + one `Inserted` change |
| `indent` | One `Modified` change per line in range |
| `patch` | Accumulate changes across all ops |

#### Audit Log Format

The audit log is a JSONL file (one JSON object per line, no trailing newline after last entry). Each line is a compact (non-pretty-printed) JSON receipt.

Example:
```
{"op":"edit","file":"src/auth.js","timestamp":1714000100,"changes":[...],"file_hash_before":1234,"file_hash_after":5678}
{"op":"insert","file":"src/auth.js","timestamp":1714000200,"changes":[...],"file_hash_before":5678,"file_hash_after":9012}
```

**Append must be atomic**: use `OpenOptions::new().create(true).append(true)` to avoid
truncation. On any write failure, do NOT consider the primary operation failed — the
primary file write has already succeeded. Log the audit failure to stderr as a warning.

**Never append a receipt for a failed operation.** The audit log is a record of
successful changes only.

#### Unit Tests Required

```
receipt::test_build_receipt_edit
receipt::test_build_receipt_insert
receipt::test_build_receipt_delete
receipt::test_build_receipt_patch_multi_op
receipt::test_file_hash_deterministic
receipt::test_print_receipt_valid_json
receipt::test_append_creates_file_if_not_exists
receipt::test_append_creates_parent_dirs
receipt::test_append_two_receipts_is_valid_jsonl
receipt::test_append_does_not_truncate_existing_log
receipt::test_no_append_if_op_failed
```

---

### `patch.rs` — Multi-Op Atomic Transaction

#### Purpose

Deserialize a PatchFile from JSON (file or stdin), validate all operations
against a single document snapshot, apply them in order to the in-memory model,
and write once. Any failure aborts the entire transaction.

#### Public API

```rust
/// Deserialize a PatchFile from a JSON string.
pub fn parse_patch(json: &str) -> Result<PatchFile, LinehashError>

/// Validate all ops against a document snapshot.
/// Returns Ok(Vec<ResolvedOp>) if all ops are valid.
/// Returns Err listing the FIRST failing op (in v1; v2 can collect all failures).
pub fn validate_patch(
    patch: &PatchFile,
    doc: &Document,
    index: &HashMap<String, Vec<usize>>,
) -> Result<Vec<ResolvedOp>, LinehashError>

/// Apply a list of ResolvedOps to a Document in-memory.
/// Returns the modified Document.
/// Does NOT write to disk — caller handles writeback.
pub fn apply_ops(doc: Document, ops: Vec<ResolvedOp>) -> Result<Document, LinehashError>

/// Resolved operation — anchor replaced by 0-based index.
pub struct ResolvedOp {
    pub kind: PatchOpKind,
    pub index: usize,
    pub content: Option<String>, // Some for edit/insert, None for delete
}
```

#### Validation Rules

All ops are resolved against the *same* Document snapshot, not sequentially.
This is critical: if op 1 edits line 5 and op 2 deletes line 5, the deletion
should fail (stale anchor on the pre-edit hash) or succeed (depending on anchor
qualification). The current rule: resolve all anchors against the *original*
Document before any mutation. In-flight conflicts are detected at apply time.

```
For each op in patch.ops:
  1. Parse op.anchor as an Anchor
  2. Resolve anchor against original Document
  3. If resolution fails → PatchFailed { op_index, reason: error.to_string() }
     → Stop immediately; return error
  4. For "edit" / "insert" ops:
     - Validate content does not contain \n or \r
     - If invalid → PatchFailed with MultiLineContentUnsupported reason
  5. Store ResolvedOp with 0-based index
```

#### Application Rules

Ops are applied in the order they appear in the patch file, to an in-memory
`Vec<LineRecord>`. Because anchors were resolved before mutation, indices may
shift as lines are inserted/deleted. Apply with offset tracking:

```
offset: i64 = 0  (tracks cumulative line count change)

for each resolved_op:
  adjusted_index = (resolved_op.index as i64 + offset) as usize
  match op.kind:
    Edit   → lines[adjusted_index].content = content; recompute hash
    Insert → lines.insert(adjusted_index + 1, new_line); offset += 1
    Delete → lines.remove(adjusted_index); offset -= 1
```

Line numbers are recomputed after all ops are applied (renumber from 1).

#### Stdin Support

If the patch file path is `-`, read from stdin:
```rust
if patch_path == "-" {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    parse_patch(&buf)?
} else {
    let content = std::fs::read_to_string(patch_path)?;
    parse_patch(&content)?
}
```

#### Unit Tests Required

```
patch::test_parse_valid_patch_file
patch::test_parse_invalid_json_fails
patch::test_parse_missing_op_field_fails
patch::test_parse_unknown_op_type_fails
patch::test_validate_all_valid_ops
patch::test_validate_stale_anchor_fails_at_correct_index
patch::test_validate_ambiguous_anchor_fails
patch::test_validate_not_found_anchor_fails
patch::test_validate_multiline_content_fails
patch::test_apply_edit_op
patch::test_apply_insert_op
patch::test_apply_delete_op
patch::test_apply_mixed_ops_in_order
patch::test_apply_index_offset_after_insert
patch::test_apply_index_offset_after_delete
patch::test_apply_does_not_write_disk
patch::test_stdin_dash_reads_stdin
```

---

### `block.rs` — Block Boundary Discovery

#### Purpose

Given an anchor line within a file, discover the start and end of the logical
code block containing that line. Supports two detection strategies:
brace-counting (C/JS/Rust/Java/Go) and indent-level (Python/YAML/TOML).

#### Public API

```rust
/// Detect the block language from a file extension.
pub fn detect_language(path: &Path) -> BlockLanguage

/// Find the block containing anchor_line using brace counting.
pub fn find_brace_block(doc: &Document, anchor_index: usize)
    -> Result<BlockBounds, LinehashError>

/// Find the block containing anchor_line using indent-level detection.
pub fn find_indent_block(doc: &Document, anchor_index: usize)
    -> Result<BlockBounds, LinehashError>

/// Dispatch to the appropriate strategy based on language.
pub fn find_block(doc: &Document, anchor: &ResolvedLine)
    -> Result<BlockBounds, LinehashError>
```

#### Language Detection Rules

| Extension | Strategy |
|---|---|
| `.py` | Indent |
| `.yaml`, `.yml` | Indent |
| `.toml` | Indent |
| `.js`, `.ts`, `.jsx`, `.tsx` | Brace |
| `.rs` | Brace |
| `.c`, `.h`, `.cpp`, `.cc` | Brace |
| `.java` | Brace |
| `.go` | Brace |
| `.cs` | Brace |
| `.rb` | Indent (Ruby uses `end`, but indent-based is safer in v1) |
| unknown | Try brace; if balanced return `Brace`; else return `AmbiguousBlockLanguage` |

#### Brace-Counting Algorithm

```
Start at anchor_index.

Phase 1 — Find the opening brace of the enclosing block:
  Walk backward from anchor_index.
  Track depth: +1 for '}', -1 for '{'
  When depth reaches -1 (i.e., we've consumed one more '{' than '}'),
  that line is the block start.
  If we reach line 0 without finding the opening brace:
    → UnbalancedBlock { line_no: anchor_line.line_no }

Phase 2 — Find the closing brace:
  From the block start, walk forward.
  Track depth: +1 for '{', -1 for '}'
  When depth reaches 0, that line is the block end.
  If we reach EOF without closing:
    → UnbalancedBlock { line_no: block_start.line_no }

Return BlockBounds { start, end, language_hint: BlockLanguage::Brace }
```

Note: This is a line-level heuristic, not a full parser. Braces inside string
literals or comments are not excluded. This is acceptable for agent use — the
agent can inspect the returned range and reject it if it looks wrong.

#### Indent-Level Algorithm

```
Start at anchor_index.
base_indent = count leading whitespace chars on anchor line.

Phase 1 — Find block start:
  Walk backward from anchor_index.
  Find the first non-blank line with indent level < base_indent.
  That line is the block opener (e.g., the `def`, `class`, or key: line).
  If we reach line 0 without finding a less-indented line:
    → Use line 0 as start (top-level block).

Phase 2 — Find block end:
  Walk forward from anchor_index.
  Find the first non-blank line with indent level < base_indent.
  The line *before* that line is the block end.
  If we reach EOF without de-indenting:
    → Last line of file is block end.

Return BlockBounds { start, end, language_hint: BlockLanguage::Indent }
```

Blank lines (empty or whitespace-only) are skipped in both phases — they do not
affect indent detection. A block end cannot be a blank line; skip forward to
the next non-blank to find the true last content line.

#### Unit Tests Required

```
block::test_brace_find_simple_function
block::test_brace_find_nested_function
block::test_brace_anchor_inside_body
block::test_brace_unbalanced_open_fails
block::test_brace_unbalanced_close_fails
block::test_indent_find_python_function
block::test_indent_find_yaml_section
block::test_indent_anchor_mid_body
block::test_indent_top_level_block
block::test_indent_EOF_terminates_block
block::test_language_detection_py
block::test_language_detection_rs
block::test_language_detection_unknown_tries_brace
block::test_find_block_dispatches_by_language
```

---

### `stats.rs` — File Statistics and Token Budget

#### Purpose

Analyze a file's hash distribution to help agents make informed decisions
about whether to read the whole file or use `--context`, and to understand
the anchor reliability landscape before issuing edits.

#### Public API

```rust
/// Compute full statistics for a Document.
pub fn compute_stats(doc: &Document) -> FileStats

/// Estimate the number of tokens a `read` of this file would consume.
pub fn estimate_read_tokens(doc: &Document) -> usize

/// Recommend the minimum hash length to keep p(collision) < 1%.
pub fn recommend_hash_length(doc: &Document) -> u8

/// Suggest a --context N value based on median function size in the file.
pub fn suggest_context_n(doc: &Document) -> usize
```

#### Token Estimation Formula

```
Base: sum of all line content lengths, in chars
Add:  anchor prefix overhead: line_count * 7 chars avg ("NNN:ab| " is ~8 chars)
Divide by 4 (approximate chars per token)
Add: JSON envelope overhead if --json mode (typically 50-200 tokens)

estimated_read_tokens = (content_chars + anchor_overhead) / 4 + envelope
```

This is intentionally a rough upper bound. Actual tokenizer counts vary by
content type (code ≈ 3.2 chars/token, prose ≈ 4 chars/token). The estimate
is meant to help the agent decide "is this file too large to read in full?"

#### Collision Statistics

```
collision_count: number of lines whose short_hash appears on more than one line
collision_pairs: all (a, b) pairs where short_hash(a) == short_hash(b)

From the Document's build_index result:
  for each (hash, indices) in index:
    if indices.len() >= 2:
      collision_count += indices.len()
      for each pair in indices.combinations(2):
        collision_pairs.push(pair)
```

#### Hash Length Advice

Use the birthday paradox formula:
```
N = doc.lines.len()
For hash_len in [2, 3, 4]:
  p = collision_probability(N, 2^(hash_len * 4))
  if p < 0.01: return hash_len
return 4  // 4 is always sufficient for any realistic file
```

#### Context N Suggestion

```
Scan for "function", "def ", "class ", "fn ", "impl " patterns in line content.
Record the line numbers where each pattern appears.
Compute gaps between consecutive occurrences → these are "function sizes".
Median function size / 2, capped at 20, minimum 3.
```

This gives a reasonable default `--context` that shows approximately one
function worth of context around any anchor.

#### Unit Tests Required

```
stats::test_empty_file_stats
stats::test_no_collisions_file
stats::test_collision_count_correct
stats::test_collision_pairs_correct
stats::test_token_estimate_proportional_to_size
stats::test_hash_length_advice_2_for_small_file
stats::test_hash_length_advice_3_for_medium_file
stats::test_context_suggestion_capped_at_20
stats::test_context_suggestion_minimum_3
```

---

### `diff_import.rs` — Unified Diff → PatchFile Compiler

#### Purpose

Read a standard unified diff (from `git diff`, `diff -u`, etc.) and emit a
valid `PatchFile` JSON that can be piped into `linehash patch`. This allows
linehash to interoperate with the entire Unix diff ecosystem.

#### Public API

```rust
/// Parse a unified diff string and compile it to a PatchFile.
/// Resolves each hunk against the current Document on disk.
pub fn from_diff(diff: &str, file_path: &Path, doc: &Document)
    -> Result<PatchFile, LinehashError>
```

#### Unified Diff Format Recap

```
--- a/src/auth.js
+++ b/src/auth.js
@@ -2,3 +2,3 @@
-  const decoded = jwt.verify(token, SECRET)
+  const decoded = jwt.verify(token, SECRET_KEY)
   if (!decoded.exp) throw new TokenError('missing expiry')
```

- Lines starting with `-` are removed
- Lines starting with `+` are added
- Lines starting with ` ` are context (unchanged)
- `@@ -L,C +L,C @@` gives original/new line numbers and counts

#### Compilation Algorithm

```
For each hunk in the diff:
  1. Extract context lines and removed lines from the hunk
  2. Locate the hunk in the current Document:
     - Try matching the context lines + removed lines around the @@ position
     - If the lines match at the stated position → match found
     - If not → scan ±10 lines around the stated position for a fuzzy match
     - If still no match → DiffHunkMismatch { hunk_line: hunk_start_in_diff }
  3. For each '-' line matched to a document line:
     - Emit { op: "edit", anchor: "{N}:{hash}", content: replacement }
     - Where replacement is the corresponding '+' line content
  4. For insertions with no matching '-' line:
     - Emit { op: "insert", anchor: "{N}:{hash}", content: inserted_line }
  5. For '-' lines with no corresponding '+':
     - Emit { op: "delete", anchor: "{N}:{hash}" }
  6. Emit file-level field from diff header or from file_path argument
```

#### Hunk Matching Guarantee

If the diff was generated from the current file (not from a stale snapshot),
every hunk should match exactly at the stated position. The ±10 fuzzy scan
handles cases where intervening edits shifted line numbers without changing
the actual content. If the content itself changed, `DiffHunkMismatch` is correct.

#### Unit Tests Required

```
diff_import::test_simple_edit_hunk
diff_import::test_insert_only_hunk
diff_import::test_delete_only_hunk
diff_import::test_mixed_hunk
diff_import::test_hunk_mismatch_fails
diff_import::test_file_mismatch_fails
diff_import::test_piped_stdin_dash
diff_import::test_round_trip_with_git_apply
```

---

### `watch.rs` — Live Hash Recomputation

#### Purpose

Watch a file for changes using the platform's native filesystem notification
API (via the `notify` crate). On each change, recompute all hashes, diff the
old and new index, and emit a structured change report.

#### Public API

```rust
/// Watch a file and emit change events.
/// once: if true, exit after the first event.
/// json: if true, emit NDJSON events to writer.
pub fn watch_file(
    path: &Path,
    once: bool,
    json: bool,
    writer: &mut impl Write
) -> Result<(), LinehashError>

/// Compare two hash indexes and produce a diff.
pub fn diff_indexes(
    old: &HashMap<String, Vec<usize>>,
    new: &HashMap<String, Vec<usize>>,
    old_doc: &Document,
    new_doc: &Document,
) -> Vec<HashDiff>

pub struct HashDiff {
    pub line_no: usize,
    pub kind: DiffKind,      // Changed, Added, Removed
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
    pub content: String,
}
```

#### Watch Algorithm

```
1. Load initial Document; build initial hash index
2. Set up notify::Watcher on path
3. Main loop:
   a. Wait for notify event (Modify or Write)
   b. Re-load Document from disk
   c. Compute diff_indexes(old_doc, new_doc)
   d. Emit change report (pretty or JSON)
   e. old_doc = new_doc
   f. If once: break
4. On Ctrl-C: clean exit
```

#### Change Report Format (pretty)

```
[14:22:01] Changed: line 2 f1→3a
[14:22:01] No change on: lines 1, 3, 4, 5
[14:22:01] New index: 847 lines, 1 hash changed
```

#### Change Report Format (NDJSON, --json)

Each event is one JSON line:
```json
{"timestamp":1714001321,"event":"changed","path":"src/auth.js","changes":[{"line_no":2,"kind":"Changed","old_hash":"f1","new_hash":"3a","content":"  const decoded = jwt.verify(token, SECRET_KEY)"}],"total_lines":847}
```

#### Platform Support

| Platform | Backend |
|---|---|
| Linux | inotify |
| macOS | kqueue / FSEvents |
| Windows | ReadDirectoryChangesW |
| Other | WatchUnsupported error |

The `notify` crate abstracts these, but watch out for:
- inotify on NFS: does not work reliably; document this
- macOS latency: FSEvents has ~50ms latency; acceptable for v1
- Windows: events fire on the directory, filtered by filename

#### Unit Tests Required

```
watch::test_watch_detects_file_write
watch::test_watch_once_exits_after_first_change
watch::test_diff_indexes_change
watch::test_diff_indexes_add_line
watch::test_diff_indexes_remove_line
watch::test_diff_indexes_no_change
watch::test_watch_nonexistent_file_fails_immediately
watch::test_json_output_parses
```

---

### `explode.rs` / `implode.rs` — Line-per-File Decomposition

#### Purpose

`explode` decomposes a source file into one tiny `.txt` file per line, enabling
line-level operations with any file-aware tool (grep, diff, git, etc.).
`implode` reverses the process, reassembling the original file byte-for-byte.

#### Public API

```rust
// explode.rs
pub fn explode(source: &Path, out_dir: &Path, force: bool) -> Result<ExplodeReport, LinehashError>

pub struct ExplodeReport {
    pub file_count: usize,
    pub out_dir: PathBuf,
}

// implode.rs
pub fn implode(in_dir: &Path, out_path: &Path) -> Result<(), LinehashError>
```

#### Explode Algorithm

```
1. If out_dir exists and is non-empty:
   - if !force → ExplodeTargetExists(out_dir.to_string())
   - if force → remove_dir_all(out_dir), then create_dir_all(out_dir)
2. Load Document from source
3. For each line in doc.lines:
   - filename = format!("{:04}_{}.txt", line.number, line.short_hash)
   - write line.content (NO newline terminator) to out_dir/filename
4. Write .meta.json:
   {
     "source": source_path_string,
     "newline": "lf" or "crlf",
     "trailing_newline": bool,
     "line_count": usize
   }
5. Return ExplodeReport
```

#### Implode Algorithm

```
1. Read .meta.json from in_dir; fail with ImplodeDirty if not present
2. List all files in in_dir
3. Validate each filename matches /^\d{4}_[0-9a-f]{2}\.txt$/
   - Any non-matching, non-.meta.json file → ImplodeDirty(in_dir.to_string())
4. Sort filenames lexicographically (NNNN prefix ensures correct order)
5. Read each file → line content (no newline terminator expected)
6. Reassemble:
   - Join with newline from .meta.json
   - Append trailing newline if .meta.json says so
7. atomic_write(out_path, assembled_bytes)
```

#### Filename Format

`{NNNN}_{hash}.txt` where:
- `NNNN` is zero-padded to 4 digits (files up to 9999 lines)
- `{hash}` is the 2-char short hash of the line content
- Extension is `.txt` for universal tool compatibility

For files > 9999 lines (unusual but valid), use more digits: the sort order is
preserved as long as all filenames have the same number of leading digits. In v1,
document the 9999-line limit; extend in v2 if needed.

#### Unit Tests Required

```
explode::test_explode_basic
explode::test_explode_filename_format
explode::test_explode_meta_json_written
explode::test_explode_existing_dir_fails_without_force
explode::test_explode_existing_dir_succeeds_with_force
explode::test_explode_empty_file
explode::test_implode_basic
explode::test_implode_lf_round_trip
explode::test_implode_crlf_round_trip
explode::test_implode_no_trailing_newline_round_trip
explode::test_implode_missing_meta_json_fails
explode::test_implode_dirty_dir_fails
explode::test_explode_implode_byte_identical
```

---

## Command Implementation Specs

This section details the exact behavior contract for each CLI command.
Implementation should match this contract exactly; divergence is a bug.

---

### Command: `read`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct ReadCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub anchor: Vec<String>,   // multiple allowed; each triggers a neighborhood
    #[arg(long, default_value = "5")]
    pub context: usize,
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load(file)?
2. If --json:
   a. output::print_read_json(&doc, &mut stdout())
   b. exit 0
3. If --anchor provided:
   a. For each anchor string:
      - parse_anchor(s)?
      - resolve(anchor, &doc, &index)?
   b. output::print_read_context(&doc, &resolved_lines, context, &mut stdout())
   c. exit 0
4. Else:
   a. output::print_read(&doc, &mut stdout())
   b. exit 0
```

**Edge cases:**
- `--anchor` with no `--context` uses default of 5
- Multiple `--anchor` flags are all honored; neighborhoods merged if overlapping
- `--context 0` shows only the anchor line itself
- `--anchor` with `--json` returns the full document JSON (anchor is ignored in JSON mode — document this in help text)

---

### Command: `index`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct IndexCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load(file)?
2. If --json: output::print_index_json(&doc, &mut stdout())
3. Else: output::print_index(&doc, &mut stdout())
4. exit 0
```

---

### Command: `edit`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct EditCmd {
    pub file: PathBuf,
    pub anchor: String,        // single anchor or range
    pub content: String,       // new line content
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. concurrency::check_guard(file, expect_mtime, expect_inode)?
2. Document::load(file)?
3. Validate content has no \n or \r → MultiLineContentUnsupported
4. Try parse_range(anchor):
   a. If Ok(range): resolve_range(&range, &doc, &index)?
      - Replace slice [start.index..=end.index] with one new line
   b. If Err: parse_anchor(anchor)? → resolve single anchor
      - Replace lines[idx].content with content; recompute hash
5. If --dry-run:
   a. Print diff showing before/after (or proposed Document if --json)
   b. exit 0 (no write)
6. Capture bytes_before = doc.render()
7. atomic_write(file, new_doc.render())?
8. If --receipt or --audit-log:
   - Build receipt; print if --receipt; append if --audit-log
9. Print "Edited line {N}." to stdout
10. exit 0
```

**Range edit behavior:**
- Range `2:f1..4:9c` replaces lines 2, 3, and 4 with a single new line
- The resulting file has (original_lines - 2) lines
- The new line gets the hash of the new content
- This is the only multi-line replacement mechanism in v1

---

### Command: `insert`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct InsertCmd {
    pub file: PathBuf,
    pub anchor: String,
    pub content: String,
    #[arg(long)]
    pub before: bool,   // insert before anchor instead of after
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}
```

**Execution flow:**
```
1. concurrency check
2. Document::load
3. parse + resolve anchor
4. Validate content (no newlines)
5. Create new LineRecord with correct number and hash
6. If --before: lines.insert(idx, new_line)
7. Else:        lines.insert(idx + 1, new_line)
8. Renumber all lines after insertion point
9. dry-run / write / receipt
10. Print "Inserted after line {N}." (or "before line {N}." if --before)
```

---

### Command: `delete`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct DeleteCmd {
    pub file: PathBuf,
    pub anchor: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}
```

**Edge cases:**
- Deleting the last line of a file → file becomes empty (0 bytes)
- Deleting a range (v2) → not supported in v1; error if range anchor provided
- Range anchors passed to `delete` → InvalidAnchor with hint to use `patch`

---

### Command: `verify`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct VerifyCmd {
    pub file: PathBuf,
    pub anchors: Vec<String>,  // 1 or more positional
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load(file)?
2. index = doc.build_index()
3. For each anchor string:
   a. parse_anchor(s)?
   b. resolve(anchor, &doc, &index) → Ok or Err
   c. Push VerifyResult { anchor_str, status, line_no?, content?, error? }
4. output::print_verify(&results, ...) or print_verify_json(...)
5. If any result failed → exit 1
6. Else → exit 0
```

**VerifyResult statuses:**
- `ok` — resolved cleanly; content shown
- `not_found` — hash not found
- `ambiguous` — multiple matches; candidates listed
- `stale` — hash found elsewhere; line number changed
- `parse_error` — invalid anchor string

---

### Command: `grep`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct GrepCmd {
    pub file: PathBuf,
    pub pattern: String,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub invert: bool,   // print non-matching lines
    #[arg(long)]
    pub case_insensitive: bool,
}
```

**Execution flow:**
```
1. Document::load(file)?
2. regex::Regex::new(&pattern)? → InvalidPattern on error
3. For each line in doc.lines:
   a. Compute match: regex.is_match(&line.content)
   b. Apply --invert if set
   c. Push matching lines to results
4. output::print_grep_results(&results, ...) or JSON
5. exit 0 (even if no matches; no match is not an error)
```

**Output format (pretty):**
```
{N}:{hash}| {matching_line_content}
```
Identical to `read` format, just filtered.

---

### Command: `annotate`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct AnnotateCmd {
    pub file: PathBuf,
    pub query: String,
    #[arg(long)]
    pub regex: bool,    // treat query as regex
    #[arg(long)]
    pub expect_one: bool,  // error if multiple matches
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load
2. If --regex: parse query as Regex
   Else: use query as literal substring
3. Filter lines by match
4. If --expect-one and results.len() > 1:
   → error: "annotate: expected 1 match, found N"
   → Print all candidates as hints
5. Output matches in `read` format
```

---

### Command: `swap`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct SwapCmd {
    pub file: PathBuf,
    pub anchor_a: String,
    pub anchor_b: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}
```

**Execution flow:**
```
1. concurrency check
2. Document::load
3. Resolve anchor_a → idx_a
4. Resolve anchor_b → idx_b
5. If idx_a == idx_b → SwapSameLine { line_no: idx_a + 1 }
6. Swap contents: lines[idx_a].content ↔ lines[idx_b].content
7. Recompute hashes for both lines
8. dry-run / write / receipt
9. "Swapped lines {a} and {b}."
```

**Receipt for swap:**
Two `Modified` changes, one for each swapped line.

---

### Command: `move`

**Syntax:** `linehash move <file> <anchor> after <anchor-b>`
or: `linehash move <file> <anchor> before <anchor-b>`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct MoveCmd {
    pub file: PathBuf,
    pub anchor: String,
    pub direction: MoveDirection,  // after | before
    pub target: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}

pub enum MoveDirection { After, Before }
```

**Execution flow:**
```
1. concurrency check
2. Document::load
3. Resolve anchor → source_idx
4. Resolve target_anchor → target_idx
5. Capture source_content = lines[source_idx].content.clone()
6. Remove source line: lines.remove(source_idx)
7. Recompute target_idx accounting for removal offset
8. Insert source_content at target_idx + 1 (after) or target_idx (before)
9. Renumber all lines
10. dry-run / write / receipt
```

---

### Command: `indent`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct IndentCmd {
    pub file: PathBuf,
    pub range: String,    // "start..end" anchor range
    pub amount: String,   // "+4", "-2", "+1t" (tabs)
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}
```

**Indent unit detection:**
```
Scan all lines in the range for their leading whitespace.
If any line has a leading tab and no leading spaces → tab mode
If any line has a leading space → space mode
If mixed → IndentMixedWhitespace error (new variant in v1.1; in v1 treat as spaces)

+N: prepend N space (or tab) chars to each line
-N: remove N space (or tab) chars from each line's start
    If a line has < N leading chars → IndentUnderflow
```

**Dry-run output:**
```
Would indent 15 lines by +4 spaces:
  line 14: "def foo():"   → "    def foo():"
  line 15: "    x = 1"   → "        x = 1"
  [...]
No file was written.
```

---

### Command: `find-block`

**Execution flow:**
```
1. Document::load
2. parse_anchor(anchor)?
3. resolve(anchor, &doc, &index)?
4. find_block(&doc, &resolved)?
5. Print: "Block: {start}..{end}  ({N} lines — {language})"
   or JSON: { "start": "...", "end": "...", "lines": N, "language": "..." }
```

---

### Command: `stats`

**Execution flow:**
```
1. Document::load
2. compute_stats(&doc) → FileStats
3. output::print_stats (or print_stats_json)
```

---

### Command: `from-diff`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct FromDiffCmd {
    pub file: PathBuf,
    pub diff: String,   // path or "-" for stdin
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load(file)?
2. Read diff content (file or stdin)
3. diff_import::from_diff(&diff_content, &file, &doc)?
4. Serialize PatchFile as JSON to stdout
5. exit 0
```

The output is always JSON (a PatchFile), regardless of --json flag.
The --json flag is reserved for future "verbose" or "debug" mode.

---

### Command: `merge-patches`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct MergePatchesCmd {
    pub patch_a: PathBuf,
    pub patch_b: PathBuf,
    #[arg(long)]
    pub base: PathBuf,    // base file to resolve anchors against
    #[arg(long)]
    pub json: bool,
}
```

**Execution flow:**
```
1. Document::load(base)?
2. parse_patch(patch_a_content)?
3. parse_patch(patch_b_content)?
4. Resolve all anchors in patch_a against doc → resolved_a
5. Resolve all anchors in patch_b against doc → resolved_b
6. Detect conflicts: find ops in resolved_a and resolved_b targeting the same index
7. If conflicts found:
   a. Print all conflicts
   b. Print merged (non-conflicting ops from both)
   c. exit 1
8. If no conflicts:
   a. Merge into single PatchFile
   b. Print merged JSON
   c. exit 0
```

---

### Command: `watch`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct WatchCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub once: bool,       // default behavior in v1
    #[arg(long)]
    pub continuous: bool, // keep watching; Ctrl-C to stop
    #[arg(long)]
    pub json: bool,
}
```

If neither `--once` nor `--continuous` → default to `--once` in v1.

---

### Command: `explode`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct ExplodeCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub force: bool,
}
```

---

### Command: `implode`

**Subcommand struct:**
```rust
#[derive(Parser)]
pub struct ImplodeCmd {
    pub dir: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
}
```

---

## CLI Wiring (`cli.rs` and `main.rs`)

### `cli.rs` — Clap Struct Hierarchy

```rust
#[derive(Parser)]
#[command(name = "linehash", version, about = "Hash-anchored file editing for agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Read(ReadCmd),
    Index(IndexCmd),
    Edit(EditCmd),
    Insert(InsertCmd),
    Delete(DeleteCmd),
    Verify(VerifyCmd),
    Grep(GrepCmd),
    Annotate(AnnotateCmd),
    Patch(PatchCmd),
    Swap(SwapCmd),
    Move(MoveCmd),
    Indent(IndentCmd),
    FindBlock(FindBlockCmd),
    Stats(StatsCmd),
    FromDiff(FromDiffCmd),
    MergePatches(MergePatchesCmd),
    Watch(WatchCmd),
    Explode(ExplodeCmd),
    Implode(ImplodeCmd),
}
```

All subcommand structs live in `cli.rs`. The `commands/` modules import from
`cli.rs` for their arg types.

### `main.rs` — Entry Point

```rust
fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("Error: {e}");
        // Print hint if available
        if let Some(hint) = e.hint() {
            eprintln!("Hint: {hint}");
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), LinehashError> {
    match cli.command {
        Commands::Read(cmd)         => commands::read::run(cmd),
        Commands::Index(cmd)        => commands::index::run(cmd),
        Commands::Edit(cmd)         => commands::edit::run(cmd),
        // ... etc
    }
}
```

**Exit codes:**
| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Error (any `LinehashError` or I/O error) |
| 1 | `verify` with any failed anchor |
| 1 | `merge-patches` with any conflict |

No custom exit codes beyond 0 and 1 in v1.

### Error Hint System

Add a `hint()` method to `LinehashError`:

```rust
impl LinehashError {
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            LinehashError::HashNotFound { .. } =>
                Some("run `linehash read <file>` to get current hashes"),
            LinehashError::StaleAnchor { .. } =>
                Some("re-read the file with `linehash read <file>`"),
            LinehashError::StaleFile { .. } =>
                Some("re-read with `linehash read <file>`"),
            LinehashError::AmbiguousHash { hash, example, .. } =>
                // Can't use format! in a &'static str; handle separately
                None,  // Hint is inline in the error message
            LinehashError::MixedNewlines =>
                Some("run `dos2unix <file>` or `unix2dos <file>` to normalize"),
            LinehashError::InvalidUtf8 =>
                Some("file contains non-UTF-8 bytes — linehash only supports UTF-8 in v1"),
            LinehashError::BinaryFile =>
                Some("this appears to be a binary file — linehash only edits text files"),
            LinehashError::MultiLineContentUnsupported =>
                Some("use `linehash patch` with multiple ops for multi-line replacement"),
            _ => None,
        }
    }
}
```

---

## Performance Specifications

### Benchmarks

Located in `benches/hash_bench.rs`. Run with `cargo bench`.

#### Target: Hash a 10,000-line file in < 5ms

```rust
fn bench_hash_10k_lines(c: &mut Criterion) {
    let file = generate_10k_line_fixture();
    c.bench_function("hash_10k_lines", |b| {
        b.iter(|| {
            Document::from_str(Path::new("bench.rs"), &file).unwrap()
        })
    });
}
```

Expectation: xxh32 over 10k lines of ~60-char average should be <1ms on any
modern CPU. The 5ms target accounts for UTF-8 validation, newline detection,
index building, and allocation overhead.

#### Target: `read` command on a 10,000-line file in < 10ms total

Including process startup, file I/O, hashing, and output formatting.
This is a wall-clock integration benchmark (not a micro-benchmark).

#### Target: `edit` command single-line edit in < 5ms

For a 1,000-line file, a single-line edit should complete in under 5ms.

### Memory Usage

The entire file is read into memory. This is intentional and documented.
The expected memory ceiling for reasonable source files:
- 100,000 lines × 80 chars avg = ~8MB of content
- Plus 3× overhead for LineRecord structs, index, output buffer ≈ 32MB total
- Well within typical process limits

**Document:** If a user passes a >100MB file, emit a warning to stderr but proceed.
This is not an error condition.

### File I/O

Always use buffered I/O:
- `BufWriter` wrapping the temp file for writes
- `std::fs::read` for reads (returns `Vec<u8>` directly, OS handles buffering)

---

## Platform Compatibility Matrix

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| UTF-8 support | ✓ | ✓ | ✓ |
| LF newlines | ✓ | ✓ | ✓ |
| CRLF newlines | ✓ | ✓ | ✓ |
| Atomic write (rename) | ✓ | ✓ | ✓ (MoveFileExW) |
| Permission preservation | ✓ | ✓ | ✗ (v2) |
| mtime guard | ✓ | ✓ | ✓ |
| inode guard | ✓ | ✓ | ✗ (always 0) |
| File watching | inotify | kqueue/FSEvents | ReadDirChangesW |
| Unicode filenames in --out | ✓ | ✓ | ✓ (wide API) |
| stdin via `-` | ✓ | ✓ | ✓ |
| SIGINT (Ctrl-C) on watch | ✓ | ✓ | ✓ (Ctrl-C event) |

### Windows-Specific Notes

- Use `cfg(target_os = "windows")` for inode skipping in `concurrency.rs`
- The `notify` crate handles Windows via `ReadDirectoryChangesW` automatically
- `tempfile::NamedTempFile::persist()` uses `MoveFileExW` internally; this is
  atomic for files on the same volume
- Path separators: always use `std::path::Path` / `PathBuf`; never hardcode `/`
- Line ending detection works correctly on Windows; CRLF is common there

### macOS-Specific Notes

- HFS+ has 1-second mtime resolution; warn in docs
- APFS has sub-second mtime; no issue
- `kqueue` / `FSEvents` backend for `notify`; event latency is ~50ms
- `sync_all()` on macOS flushes to the OS buffer; not to physical disk
  (acceptable for our use case)

---

## Test Strategy — Expanded

### Test Infrastructure

#### Fixtures

Located in `tests/fixtures/`. These are static files committed to the repo.
They are never regenerated automatically — any change is intentional.

| Fixture | Description |
|---|---|
| `simple_lf.js` | 10-line JS file, LF, trailing newline |
| `simple_crlf.js` | Same content, CRLF |
| `no_trailing_newline.js` | Same content, no trailing newline |
| `empty.txt` | Zero bytes |
| `single_line.txt` | One line, LF |
| `single_line_no_newline.txt` | One line, no newline |
| `duplicate_hashes.py` | Deliberately crafted for max collisions |
| `large_1000.rs` | 1000-line Rust file for performance tests |
| `large_10000.py` | 10000-line Python file |
| `binary.bin` | Binary file (rejects cleanly) |
| `invalid_utf8.txt` | Contains 0x80 byte (rejects cleanly) |
| `mixed_newlines.js` | Has both LF and CRLF (rejects cleanly) |
| `brace_balanced.rs` | Well-formed Rust file for find-block |
| `indent_python.py` | Well-formed Python for find-block |
| `unbalanced_braces.js` | For testing UnbalancedBlock error |
| `unicode_content.py` | Lines with emoji and CJK chars |
| `whitespace_only_lines.txt` | File where some lines are only spaces |

#### Helper Utilities (`tests/helpers.rs`)

```rust
// Create a temp file with given content, return its path
pub fn tmpfile(content: &str) -> TempPath

// Run linehash with given args; return stdout, stderr, exit code
pub fn run_linehash(args: &[&str]) -> (String, String, i32)

// Assert linehash exits 0 with stdout containing expected
pub fn assert_ok_contains(args: &[&str], expected: &str)

// Assert linehash exits non-zero with stderr containing expected
pub fn assert_err_contains(args: &[&str], expected: &str)

// Parse JSON output from linehash
pub fn parse_json(args: &[&str]) -> serde_json::Value

// Write content to a tempfile and run linehash edit
pub fn do_edit(content: &str, anchor: &str, new_content: &str) -> String
```

### Integration Test Matrix

This section enumerates every `assert_cmd`-style integration test. Each test
is listed with its fixture, command args, and expected outcome.

#### `read` integration tests

| Test | File | Args | Expected |
|---|---|---|---|
| `read_simple_lf` | `simple_lf.js` | `read` | Output matches `N:hash| content` format |
| `read_simple_crlf` | `simple_crlf.js` | `read` | Same format (CRLF internal, LF in output) |
| `read_json_valid` | `simple_lf.js` | `read --json` | Valid JSON, `newline: "lf"` |
| `read_json_crlf` | `simple_crlf.js` | `read --json` | `newline: "crlf"` |
| `read_json_mtime_present` | `simple_lf.js` | `read --json` | `mtime` field is non-zero |
| `read_json_inode_linux` | `simple_lf.js` | `read --json` | `inode` field is non-zero on Linux |
| `read_context_shows_neighborhood` | `simple_lf.js` | `read --anchor 3:xx --context 1` | Lines 2,3,4 only |
| `read_context_marks_anchor` | `simple_lf.js` | `read --anchor 3:xx --context 1` | Line 3 marked with `→` |
| `read_context_zero` | `simple_lf.js` | `read --anchor 3:xx --context 0` | Only line 3 |
| `read_invalid_file` | `/nonexistent` | `read` | Exit 1, I/O error message |
| `read_binary_file` | `binary.bin` | `read` | Exit 1, "binary file" error |
| `read_invalid_utf8` | `invalid_utf8.txt` | `read` | Exit 1, "not valid UTF-8" error |
| `read_mixed_newlines` | `mixed_newlines.js` | `read` | Exit 1, "mixed" error with hint |
| `read_empty_file` | `empty.txt` | `read` | Empty output, exit 0 |

#### `edit` integration tests

| Test | Setup | Args | Expected |
|---|---|---|---|
| `edit_single_line` | `simple_lf.js` | `edit {file} {anchor} "new content"` | Line changed, others intact |
| `edit_preserves_lf` | `simple_lf.js` | `edit ...` | Output has LF |
| `edit_preserves_crlf` | `simple_crlf.js` | `edit ...` | Output has CRLF |
| `edit_preserves_no_trailing_newline` | `no_trailing_newline.js` | `edit ...` | No trailing newline |
| `edit_stale_anchor` | Modify file between read and edit | `edit {old_anchor} "content"` | Exit 1, "stale anchor" |
| `edit_hash_not_found` | Any file | `edit {file} xx "content"` | Exit 1, "not found" |
| `edit_ambiguous_hash` | `duplicate_hashes.py` | `edit {file} {colliding_hash} "content"` | Exit 1, "ambiguous" |
| `edit_range` | `simple_lf.js` | `edit {file} {start}..{end} "replacement"` | Range collapsed to 1 line |
| `edit_dry_run_no_write` | `simple_lf.js` | `edit ... --dry-run` | File unmodified, diff printed |
| `edit_dry_run_json` | `simple_lf.js` | `edit ... --dry-run --json` | Proposed Document JSON |
| `edit_receipt` | `simple_lf.js` | `edit ... --receipt` | Receipt JSON printed |
| `edit_audit_log` | `simple_lf.js` | `edit ... --audit-log log.jsonl` | `log.jsonl` has one entry |
| `edit_audit_log_two_edits` | `simple_lf.js` | two sequential edits | `log.jsonl` has two entries |
| `edit_expect_mtime_ok` | `simple_lf.js` | `edit ... --expect-mtime {mtime}` | Succeeds |
| `edit_expect_mtime_stale` | `simple_lf.js`, modified before edit | `edit ... --expect-mtime {old_mtime}` | Exit 1, "stale file" |
| `edit_multiline_content_fails` | `simple_lf.js` | `edit {file} {anchor} "line1\nline2"` | Exit 1, multiline error |

#### `insert` integration tests

| Test | Expected |
|---|---|
| `insert_after_anchor` | New line appears directly after anchor |
| `insert_before_anchor` | New line appears directly before anchor |
| `insert_at_end_of_file` | Works correctly |
| `insert_preserves_trailing_newline` | Trailing newline state unchanged |
| `insert_dry_run` | Shows insertion diff, no file write |
| `insert_receipt` | Receipt has one Inserted change |

#### `delete` integration tests

| Test | Expected |
|---|---|
| `delete_middle_line` | Line removed, surrounding lines intact |
| `delete_last_line` | File correctly shortened |
| `delete_only_line` | File becomes empty |
| `delete_preserves_newline_style` | Style maintained |
| `delete_dry_run` | Shows deletion diff, no write |
| `delete_receipt` | Receipt has one Deleted change |

#### `verify` integration tests

| Test | Expected exit | Expected output |
|---|---|---|
| `verify_all_valid` | 0 | All `✓` lines |
| `verify_one_stale` | 1 | Mixed `✓` and `✗` |
| `verify_all_stale` | 1 | All `✗` lines |
| `verify_ambiguous` | 1 | `?` line with candidates |
| `verify_json_schema` | 0 or 1 | Valid JSON array |
| `verify_not_found` | 1 | `✗` with "not found" |
| `verify_multiple_anchors_reports_all` | 1 | All failures listed, not just first |

#### `grep` integration tests

| Test | Expected |
|---|---|
| `grep_match_found` | Matching lines in read format |
| `grep_no_match` | Empty output, exit 0 |
| `grep_invalid_regex` | Exit 1, InvalidPattern error |
| `grep_json` | Valid JSON `lines` array |
| `grep_invert` | Non-matching lines |
| `grep_case_insensitive` | Matches regardless of case |

#### `patch` integration tests

| Test | Expected |
|---|---|
| `patch_all_valid_ops` | File correctly modified, single write |
| `patch_first_op_stale` | File untouched, PatchFailed op_index 0 |
| `patch_middle_op_stale` | File untouched, PatchFailed at correct index |
| `patch_last_op_stale` | File untouched, PatchFailed at last index |
| `patch_dry_run_validates_all` | Reports all errors, no write |
| `patch_receipt` | Receipt covers all ops |
| `patch_stdin_dash` | Reads JSON from stdin |
| `patch_mixed_ops` | Edit + insert + delete applied correctly |
| `patch_expect_mtime` | Guard applied to whole transaction |

#### `swap` integration tests

| Test | Expected |
|---|---|
| `swap_two_lines` | Contents transposed |
| `swap_same_line_fails` | SwapSameLine error |
| `swap_dry_run` | Both lines shown in diff, no write |
| `swap_then_swap_back` | Byte-identical to original |
| `swap_receipt` | Two Modified changes |

#### `indent` integration tests

| Test | Expected |
|---|---|
| `indent_plus4_spaces` | All lines in range have 4 more leading spaces |
| `indent_minus2_spaces` | All lines dedented |
| `indent_underflow_fails` | IndentUnderflow names offending line |
| `indent_tab_mode` | Tabs added/removed |
| `indent_dry_run` | Full before/after for each line |
| `indent_round_trip` | +N then -N is byte-identical to original |
| `indent_invalid_range_fails` | InvalidIndentRange error |

#### `find-block` integration tests

| Test | Expected |
|---|---|
| `find_block_js_function` | Exact brace-balanced range |
| `find_block_python_function` | Indent-delimited range |
| `find_block_unbalanced_fails` | UnbalancedBlock error |
| `find_block_json` | Parseable JSON with start, end, lines, language |
| `find_block_top_level_python` | Top-level block detected |

#### `stats` integration tests

| Test | Expected |
|---|---|
| `stats_line_count` | Correct |
| `stats_collision_report` | Correct pairs |
| `stats_token_estimate` | Proportional to file size |
| `stats_json` | Valid JSON, stable schema |

#### `from-diff` and `merge-patches` integration tests

| Test | Expected |
|---|---|
| `from_diff_simple_edit` | Emits valid PatchFile JSON |
| `from_diff_round_trip` | Apply via patch → byte-identical to git apply |
| `from_diff_hunk_mismatch` | DiffHunkMismatch error |
| `from_diff_file_mismatch` | DiffFileMismatch error |
| `merge_patches_no_conflicts` | Merged PatchFile JSON |
| `merge_patches_with_conflicts` | Exit 1, all conflicts reported |
| `merge_patches_partial_merge_output` | Non-conflicting ops in output |

#### `watch` integration tests

| Test | Expected |
|---|---|
| `watch_once_detects_write` | Emits change report, exits |
| `watch_once_exits_within_1s` | Wall-clock time < 1 second |
| `watch_json_parses` | NDJSON event parses correctly |
| `watch_nonexistent_file_fails` | Immediate error, exit 1 |

#### `explode` / `implode` integration tests

| Test | Expected |
|---|---|
| `explode_creates_files` | N+1 files (N lines + .meta.json) |
| `explode_filename_format` | Matches `{NNNN}_{hash}.txt` |
| `explode_existing_dir_fails` | ExplodeTargetExists |
| `explode_force_overwrites` | Succeeds |
| `implode_round_trip_lf` | Byte-identical |
| `implode_round_trip_crlf` | Byte-identical |
| `implode_round_trip_no_trailing_newline` | Byte-identical |
| `implode_dirty_dir_fails` | ImplodeDirty error |
| `implode_missing_meta_fails` | Error |

#### Concurrency guard integration tests

| Test | Expected |
|---|---|
| `expect_mtime_correct_passes` | Edit succeeds |
| `expect_mtime_wrong_fails` | StaleFile, file untouched |
| `expect_inode_correct_passes` | Edit succeeds (Linux/macOS) |
| `expect_inode_wrong_fails` | StaleFile, file untouched |
| `no_guard_skips_check` | Edit proceeds without stat |

#### Receipt and audit log integration tests

| Test | Expected |
|---|---|
| `receipt_edit_valid_json` | Receipt prints to stdout |
| `receipt_has_correct_hashes` | before/after hashes verified |
| `audit_log_created` | File exists after edit |
| `audit_log_appended` | Two edits → two JSONL entries |
| `audit_log_no_entry_on_fail` | Failed edit → no entry appended |
| `audit_log_entry_is_valid_json` | Each line parses as JSON |

#### Dry-run integration tests

| Test | Expected |
|---|---|
| `dry_run_edit_no_write` | File stat matches before/after |
| `dry_run_insert_no_write` | File stat matches before/after |
| `dry_run_delete_no_write` | File stat matches before/after |
| `dry_run_patch_no_write` | File stat matches before/after |
| `dry_run_indent_no_write` | File stat matches before/after |
| `dry_run_swap_no_write` | File stat matches before/after |
| `dry_run_json_parses` | Proposed Document JSON is valid |

### Snapshot Tests (via `insta`)

These tests capture the exact terminal output and fail on any change.
Run `cargo insta review` to approve changed snapshots.

```
snapshots/read_pretty__simple_lf_js.snap
snapshots/read_json__simple_lf_js.snap
snapshots/index_pretty__simple_lf_js.snap
snapshots/index_json__simple_lf_js.snap
snapshots/stats_json__large_1000_rs.snap
snapshots/verify_json__all_valid.snap
snapshots/error_hash_not_found.snap
snapshots/error_ambiguous_hash.snap
snapshots/error_stale_anchor.snap
snapshots/error_stale_file.snap
snapshots/error_mixed_newlines.snap
snapshots/error_invalid_utf8.snap
snapshots/error_multiline_content.snap
snapshots/error_patch_failed.snap
snapshots/error_indent_underflow.snap
snapshots/error_unbalanced_block.snap
```

---

## CI/CD Pipeline

### GitHub Actions Workflow

File: `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta]
      fail-fast: false

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust ${{ matrix.rust }}
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
          components: clippy, rustfmt

      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: cargo fmt --check
        run: cargo fmt --all -- --check

      - name: cargo clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: cargo test
        run: cargo test --all-features

      - name: cargo test --release
        run: cargo test --release

  bench:
    name: Benchmarks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: cargo bench
        run: cargo bench -- --output-format bencher | tee bench-output.txt

      - name: Store benchmark results
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: cargo
          output-file-path: bench-output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: ${{ github.ref == 'refs/heads/main' }}

  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview

      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov

      - name: Generate coverage
        run: cargo llvm-cov --all-features --lcov --output-path lcov.info

      - name: Upload to Codecov
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: false

  msrv:
    name: MSRV (Rust 1.85)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.85
      - run: cargo test

  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install cargo-audit
        run: cargo install cargo-audit
      - name: Audit dependencies
        run: cargo audit
```

### Release Workflow

File: `.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags:
      - 'v*.*.*'

jobs:
  build:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: linehash
          - os: ubuntu-latest
            target: x86_64-unknown-linux-musl
            artifact: linehash
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: linehash
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: linehash
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: linehash.exe

    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czf linehash-${{ matrix.target }}.tar.gz ${{ matrix.artifact }}

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: linehash-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/linehash-${{ matrix.target }}.tar.gz

  publish:
    name: Publish to crates.io
    runs-on: ubuntu-latest
    needs: build
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo publish --token ${{ secrets.CARGO_REGISTRY_TOKEN }}

  github-release:
    name: Create GitHub Release
    runs-on: ubuntu-latest
    needs: build
    steps:
      - uses: actions/download-artifact@v4
      - uses: softprops/action-gh-release@v1
        with:
          files: '**/*.tar.gz'
          generate_release_notes: true
```

### Pre-commit Hooks

File: `.pre-commit-config.yaml`

```yaml
repos:
  - repo: local
    hooks:
      - id: cargo-fmt
        name: cargo fmt
        entry: cargo fmt -- --check
        language: system
        pass_filenames: false

      - id: cargo-clippy
        name: cargo clippy
        entry: cargo clippy --all-targets -- -D warnings
        language: system
        pass_filenames: false

      - id: cargo-test
        name: cargo test
        entry: cargo test
        language: system
        pass_filenames: false
```

---

## Security Considerations

### Input Validation

All inputs from CLI arguments pass through structured validation before use.
No `eval`, no shell interpolation, no string concatenation into commands.

**Anchor strings:** Validated against `/^[0-9a-f]{2}$/` (hash) and decimal
unsigned integer (line number). Invalid characters produce `InvalidAnchor` error.

**Content strings:** Validated for absence of `\n` and `\r` before use.
Control characters other than tab are allowed (they are legitimate code content).

**File paths:** Passed directly to `std::fs` APIs. No path traversal protection
beyond what the OS provides. The tool is not a server; users supply their own paths.

**Regex patterns:** Compiled by the `regex` crate which provides a safe, panic-free
API. Invalid patterns produce `InvalidPattern` with the crate's error message.
The `regex` crate uses bounded-time matching (no catastrophic backtracking).

**JSON patch files:** Deserialized by `serde_json`. Unknown fields are ignored.
Missing required fields produce `InvalidPatch` with a helpful message.

### File System Safety

**Atomic writes:** The temp-file-rename strategy ensures the target file is never
in a partially written state. A crash between `write_all` and `persist` leaves
the temp file as orphaned garbage (auto-cleaned on next boot or by the OS).

**Race condition (TOCTOU):** The concurrency guard (`--expect-mtime`) mitigates
the window between reading and writing. Without the guard, two processes can race.
This is a deliberate tradeoff: the tool is a developer productivity tool, not a
database. Document this clearly.

**Symlink handling:** `atomic_write` replaces the symlink target, not the symlink
itself. This is standard Unix semantics. Document it.

**Directory traversal:** `explode --out` creates files only within the specified
directory. `implode` reads only from the specified directory. No `..` traversal.

### Dependency Trust

Dependencies are minimal and well-maintained:
- `xxhash-rust`: widely used, no unsafe beyond the hash function itself
- `clap`: official argument parsing crate for the Rust ecosystem
- `serde`/`serde_json`: de facto standard, highly audited
- `tempfile`: purpose-built for atomic writes, widely used
- `thiserror`: derive macro only, no runtime complexity
- `regex`: Google/Rust safety guarantees, no exponential backtracking
- `notify`: cross-platform, used by cargo-watch and many others
- `walkdir`: simple directory traversal, minimal dependencies

Run `cargo audit` as part of CI to detect known vulnerabilities.

### Audit Log Security

The audit log is append-only from the tool's perspective. It records operations
that succeeded. It is NOT a security audit log — it does not authenticate users,
does not sign entries, and can be modified or deleted by any process with write
access to the directory. It is an operational convenience for undo and debugging,
not a security control.

---

## Documentation Strategy

### README.md Structure

```
# linehash

One-line description.

## Why linehash?

Problem: str_replace failures in agent workflows.
Solution: hash-anchored edits that fail safely instead of silently corrupting.

## Installation

cargo install linehash

## Quick Start

5-step example showing: read → identify anchor → verify → edit → read again

## Command Reference

Auto-generated from --help output? Or manually maintained?
Recommendation: manually maintained in README for clarity; verified by CI.

## Integration with Claude Code

Paste-ready CLAUDE.md block (same as the one in PLAN.md).

## Design Decisions

Link to PLAN.md or embed key decisions here.

## Contributing

Link to CONTRIBUTING.md.

## License

MIT OR Apache-2.0
```

### Per-Command Help Text

Each clap subcommand must have:
- A 1-2 sentence `about` description
- A `long_about` with full behavior, including safety guarantees
- `value_name` on all positional arguments
- `help` text on every flag

Example:
```rust
#[derive(Parser)]
#[command(
    about = "Annotated view of a file with line-hash prefixes",
    long_about = "Read a file and print each line with its anchor prefix.\n\
                  The anchor format is N:hash where N is the 1-based line number\n\
                  and hash is the 2-char xxh32 short hash of the line content.\n\
                  Use these anchors with edit, insert, delete, verify, and patch."
)]
pub struct ReadCmd { ... }
```

### POC.md

The Node.js POC serves as:
1. A working proof of concept for the hash+anchor concept
2. A quick-start alternative for projects not using Rust tooling
3. A reference implementation to diff against for behavior questions

POC.md should document:
- How to run the POC
- Known limitations vs the Rust tool
- Benchmark results (str_replace failures vs linehash success rate)

### CHANGELOG.md

Follow Keep a Changelog format (https://keepachangelog.com).
Each version has: Added, Changed, Deprecated, Removed, Fixed, Security.

### CONTRIBUTING.md

```
# Contributing to linehash

## Development Setup

1. Install Rust 1.85+
2. Clone the repo
3. Run `cargo test` — all tests should pass
4. Run `cargo bench` — no regressions against main

## Coding Standards

- No `unsafe` blocks without a detailed comment explaining why
- Every new public function has a doc comment
- Every new error variant has a recovery hint
- No `unwrap()` in non-test code
- All I/O errors are wrapped in `LinehashError::Io`
- Use `thiserror` for all error types; no `Box<dyn Error>` in public APIs

## Pull Request Checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] New behavior has integration tests in `tests/`
- [ ] New error variants have snapshot tests
- [ ] README updated if CLI behavior changed
- [ ] CHANGELOG.md updated
- [ ] All new public APIs have doc comments

## Commit Message Convention

Prefix: feat | fix | perf | refactor | test | docs | chore
Example: "feat(patch): add --dry-run support"
```

---

## V2 Roadmap — Detailed Planning

Each item in the v2 roadmap is expanded here with enough detail to begin
design work after v1 ships.

---

### V2.1 — Multi-line Insert and Replace

**Problem:** v1 `edit` and `insert` reject content containing `\n`. This prevents
an agent from replacing a block of code with a different number of lines.

**Design:**
```
linehash edit src/auth.js 2:f1..4:9c - << 'EOF'
  const decoded = jwt.verify(token, SECRET_KEY)
  if (!decoded.exp) throw new TokenError('missing expiry')
  logger.debug('token verified')
EOF
```

CLI changes:
- `edit` and `insert` accept `-` as content → read from stdin
- `patch` op schema gains optional `content_lines: Vec<String>` field
- `--receipt` records each inserted/modified line individually

Data model changes:
- `Vec<LineRecord>` mutation: splice-replace slice with multiple new lines
- Renumber all downstream line numbers

**Acceptance criteria:** round-trip test: multi-line replace and then revert to
original is byte-identical to original.

---

### V2.2 — `linehash diff`

**Purpose:** Show pending changes between the current file state and a saved
snapshot (similar to `git diff` but at the hash level).

**Design:**
```
linehash diff src/auth.js --since-snapshot .linehash/snapshots/auth.json
```

Snapshots are created by `linehash read --json > .linehash/snapshots/auth.json`.
`diff` reloads the current file, compares against the snapshot, and shows
which line numbers and hashes have changed.

**Output:**
```
Line 2: f1 → 3a  "  const decoded = jwt.verify(token, SECRET_KEY)"
Line 15: 9c → a1  "  return decoded"
```

**Note:** This is purely informational. It does not modify anything.

---

### V2.3 — `linehash undo`

**Purpose:** Reverse the last N operations from the audit log.

**Design:**
```
linehash undo src/auth.js                    -- undo last 1 op
linehash undo src/auth.js --count 3          -- undo last 3 ops
linehash undo src/auth.js --dry-run          -- show what would be undone
```

**Algorithm:**
1. Read `--audit-log` file (or default `.linehash/audit.jsonl`)
2. Filter receipts for `file` argument
3. Take last N receipts in reverse order
4. For each receipt, build an inverse patch:
   - `Modified` → emit `edit` to restore `before` content using `hash_after` as anchor
   - `Inserted` → emit `delete` using `hash_after` as anchor
   - `Deleted` → emit `insert` using the line_no position
5. Apply inverse patch atomically

**Correctness constraint:** Undo only works if no other changes have occurred
between the logged operation and now. The `hash_after` anchor detects divergence.

---

### V2.4 — `linehash mcp --stdio`

**Purpose:** Expose all linehash commands as an MCP (Model Context Protocol)
server, allowing Claude to call linehash operations as structured tools rather
than spawning a new process per command.

**Protocol:** JSON-RPC over stdin/stdout (MCP standard).

**Tool definitions (approximate):**
```json
[
  {
    "name": "linehash_read",
    "description": "Read a file with hash-annotated line prefixes",
    "inputSchema": {
      "type": "object",
      "properties": {
        "file": { "type": "string" },
        "anchor": { "type": "string" },
        "context": { "type": "integer" }
      }
    }
  },
  {
    "name": "linehash_edit",
    "description": "Edit a single line by hash anchor",
    "inputSchema": { ... }
  },
  ...
]
```

**Implementation:** A thin dispatch layer (~200 lines) over the existing Rust
functions. The `commands/` modules are called directly, with output captured
into a string buffer rather than written to stdout.

**Process model:**
- Single persistent process (no per-call spawn overhead)
- Tools map 1:1 to CLI subcommands
- All existing safety guarantees apply identically

---

### V2.5 — `linehash session`

**Purpose:** Stateful editing sessions that accumulate operations into a
batch, then apply them atomically as a single patch.

**Design:**
```
linehash session start src/auth.js --session myfix
linehash session edit myfix 2:f1 "new content"
linehash session insert myfix 4:9c "new line"
linehash session preview myfix        -- dry-run the accumulated ops
linehash session commit myfix         -- apply as atomic patch
linehash session abort myfix          -- discard all accumulated ops
```

**Storage:** Sessions are stored as PatchFile JSON in `.linehash/sessions/{name}.json`.
`session start` snapshots the file's mtime. `session commit` applies the patch with
`--expect-mtime` from the snapshot, guaranteeing no intervening changes.

**Relationship to `patch`:** Sessions are syntactic sugar over `patch`. Every
`session edit` appends an op to the session's PatchFile. `session commit` is
equivalent to `linehash patch <file> <session-file>`.

---

### V2.6 — Git-Aware Annotation

**Purpose:** Enrich `read` output with git information — mark new lines, recently
modified lines, and lines from the current author as "hot anchors."

**Design:**
```
linehash read src/auth.js --git-aware
  1:a3| function verifyToken(token) {
+ 2:f1|   const decoded = jwt.verify(token, SECRET_KEY)   [new since HEAD]
  3:0e|   if (!decoded.exp) ...
~ 4:9c|   return decoded   [modified in last 3 commits]
```

**Implementation:** Shell out to `git diff HEAD -- <file>` and `git log -n 3 <file>`
to identify new/modified lines. Annotate the output. Fall back silently if not
in a git repo.

---

### V2.7 — Watch Daemon / Persistent Socket Mode

**Purpose:** Eliminate the per-command process spawn overhead for agent workflows
that issue many sequential commands on watched files.

**Design:**
```
linehash daemon start                          -- start background daemon
linehash daemon watch src/auth.js              -- subscribe to a file
linehash daemon status                         -- list watched files
linehash daemon stop                           -- shutdown daemon
```

The daemon listens on a Unix socket (or named pipe on Windows) at a well-known
path (e.g., `$XDG_RUNTIME_DIR/linehash.sock`). Clients connect and issue
commands as JSON-RPC messages. The daemon maintains an in-memory cache of loaded
Documents and reloads them on file change events.

**Benefits:**
- Near-zero latency for cached Documents
- Single inotify/kqueue watcher instead of N per-command watchers
- Enables server-sent events for agent subscription

---

### V2.8 — Cross-File Patch

**Purpose:** A single `PatchFile` can reference multiple files, applied with
a two-phase (prepare + commit) strategy that guarantees atomicity across files.

**Design:**
```json
{
  "version": 2,
  "ops": [
    { "file": "src/auth.js",     "op": "edit",   "anchor": "2:f1", "content": "..." },
    { "file": "src/config.js",   "op": "edit",   "anchor": "5:a3", "content": "..." },
    { "file": "tests/auth.test.js", "op": "insert", "anchor": "10:b2", "content": "..." }
  ]
}
```

**Two-phase algorithm:**
1. Phase 1 (Prepare): Load all files, resolve all anchors, write all temp files
2. If any failure in Phase 1 → abort, no file modified
3. Phase 2 (Commit): Rename all temp files to target paths
4. If any rename fails → log error, remaining files are in their original state
   (best-effort rollback is not possible with rename; document this limitation)

---

### V2.9 — Optional Relaxed Anchor Recovery

**Purpose:** Allow the tool to find a moved line when its qualified anchor is
stale, rather than requiring a full re-read.

**Design:** A new flag `--follow` on resolution:
```
linehash edit src/auth.js 2:f1 "new content" --follow
```

With `--follow`:
- If line 2 has hash `f1` → resolve normally
- If line 2 has a different hash, but `f1` exists at exactly one other line → retarget there
- If `f1` is ambiguous → still fail
- If `f1` not found → still fail

Without `--follow` (default): strict rejection, current behavior.

**Tradeoff:** `--follow` is explicitly opt-in. The default is always strict.
Never silently retarget even with `--follow` if there are multiple candidates.

---

### V2.10 — Longer Hashes

**Purpose:** For large files (>500 lines), 2-char hashes have very high collision
rates. Support 3- and 4-char hashes to reduce ambiguity.

**Design:**
```
linehash read src/large.py --hash-len 3
1:a3f| def some_function():
2:f1b|   ...
```

**Anchor format with longer hashes:**
- 3-char: `2:f1b` (line-qualified), `f1b` (unqualified)
- 4-char: `2:f1b9`, `f1b9`
- Range: `2:f1b..4:9c3`

**Backward compatibility:** The `--hash-len` must be specified at both read-time
(to know which anchors to use) and edit-time (to know how to resolve them).
Default is always 2 for backward compatibility.

---

## Code Review Checklist

Use this checklist for every PR review.

### Correctness

- [ ] Does the change match the PLAN.md spec for the affected module?
- [ ] Are all error paths covered?
- [ ] Is the new behavior tested in `tests/`?
- [ ] Do existing tests still pass?
- [ ] Is atomic write used for all file mutations?
- [ ] Are line numbers consistently 1-based in UX and 0-based internally?
- [ ] Is the concurrency guard called before `Document::load` in mutation commands?

### Safety

- [ ] No `unwrap()` in non-test code (use `?` or explicit match)
- [ ] No `panic!()` in non-test code
- [ ] No `unsafe {}` blocks without a detailed comment
- [ ] No shell command interpolation or `std::process::Command` with user input

### Output

- [ ] Pretty output matches the spec in the Output Formatting section
- [ ] `--json` output parses as valid JSON
- [ ] `--json` schema is stable (checked against snapshots)
- [ ] Error messages include a recovery hint

### Style

- [ ] `cargo fmt` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] New public functions have doc comments
- [ ] No TODOs or FIXMEs in merged code (file as issues instead)

### Performance

- [ ] Does the change introduce any O(N²) behavior?
- [ ] Is BufWriter used for all file writes?
- [ ] Is there any unnecessary cloning of large strings?

---

## Release Checklist

Perform these steps before tagging a release.

### Pre-release

- [ ] All milestone checklists completed (see Phases section)
- [ ] All acceptance criteria satisfied
- [ ] `cargo test` passes on Linux, macOS, Windows
- [ ] `cargo bench` shows no regressions vs previous release
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo audit` shows no known vulnerabilities
- [ ] CHANGELOG.md updated with all changes since last release
- [ ] README.md reflects current CLI behavior
- [ ] All snapshot tests are approved (`cargo insta review`)

### Release

- [ ] Tag: `git tag v0.1.0 -m "v0.1.0 — initial release"`
- [ ] Push tag: `git push origin v0.1.0`
- [ ] CI release workflow completes successfully
- [ ] GitHub release created with binaries
- [ ] `cargo publish` succeeds
- [ ] Test `cargo install linehash` on clean machine

### Post-release

- [ ] Announce in project CLAUDE.md integration note
- [ ] Update any dependent project CLAUDE.md blocks with new version
- [ ] File v2 milestone issues on GitHub

---

## Appendix A — Reference: Anchor Format Grammar

```ebnf
anchor       ::= unqualified | qualified
unqualified  ::= hex_byte
qualified    ::= line_number ":" hex_byte
range        ::= qualified ".." qualified

hex_byte     ::= hex_char hex_char
hex_char     ::= [0-9a-fA-F]
line_number  ::= digit+
digit        ::= [0-9]
```

Normalization: uppercase hex chars are lowercased before storage and display.
Leading zeros in line numbers are rejected (e.g., `02:f1` is invalid; use `2:f1`).

---

## Appendix B — PatchFile Schema (v1)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "LinehashPatchFile",
  "type": "object",
  "required": ["file", "ops"],
  "properties": {
    "file": {
      "type": "string",
      "description": "Path to the file to patch (relative or absolute)"
    },
    "ops": {
      "type": "array",
      "minItems": 1,
      "items": {
        "oneOf": [
          {
            "type": "object",
            "required": ["op", "anchor", "content"],
            "properties": {
              "op":      { "type": "string", "const": "edit" },
              "anchor":  { "type": "string" },
              "content": { "type": "string", "description": "Single-line only in v1" }
            }
          },
          {
            "type": "object",
            "required": ["op", "anchor", "content"],
            "properties": {
              "op":      { "type": "string", "const": "insert" },
              "anchor":  { "type": "string" },
              "content": { "type": "string" }
            }
          },
          {
            "type": "object",
            "required": ["op", "anchor"],
            "properties": {
              "op":     { "type": "string", "const": "delete" },
              "anchor": { "type": "string" }
            }
          }
        ]
      }
    }
  },
  "additionalProperties": false
}
```

---

## Appendix C — Receipt Schema (v1)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "LinehashReceipt",
  "type": "object",
  "required": ["op", "file", "timestamp", "changes", "file_hash_before", "file_hash_after"],
  "properties": {
    "op":               { "type": "string" },
    "file":             { "type": "string" },
    "timestamp":        { "type": "integer", "description": "Unix seconds" },
    "changes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["line_no", "kind"],
        "properties": {
          "line_no":     { "type": "integer" },
          "kind":        { "type": "string", "enum": ["Modified", "Inserted", "Deleted"] },
          "before":      { "type": ["string", "null"] },
          "after":       { "type": ["string", "null"] },
          "hash_before": { "type": ["string", "null"] },
          "hash_after":  { "type": ["string", "null"] }
        }
      }
    },
    "file_hash_before": { "type": "integer" },
    "file_hash_after":  { "type": "integer" }
  }
}
```

---

## Appendix D — Stats Schema (v1)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "LinehashFileStats",
  "type": "object",
  "required": [
    "file", "line_count", "unique_hashes", "collision_count",
    "collision_pairs", "estimated_read_tokens",
    "hash_length_advice", "suggested_context_n"
  ],
  "properties": {
    "file":                   { "type": "string" },
    "line_count":             { "type": "integer" },
    "unique_hashes":          { "type": "integer" },
    "collision_count":        { "type": "integer" },
    "collision_pairs": {
      "type": "array",
      "items": {
        "type": "array",
        "items": { "type": "integer" },
        "minItems": 2,
        "maxItems": 2
      }
    },
    "estimated_read_tokens":  { "type": "integer" },
    "hash_length_advice":     { "type": "integer", "enum": [2, 3, 4] },
    "suggested_context_n":    { "type": "integer" }
  }
}
```

---

## Appendix E — Audit Log Entry Schema (v1)

Each line in a `.linehash/audit.jsonl` file is a compact JSON object matching
the Receipt schema. The file is valid JSONL: each line is independently parseable.

Parsing the audit log for undo:
```python
import json

def read_audit_log(path, file_filter=None):
    with open(path) as f:
        receipts = [json.loads(line) for line in f if line.strip()]
    if file_filter:
        receipts = [r for r in receipts if r['file'] == file_filter]
    return receipts

def build_undo_patch(receipts, target_file):
    ops = []
    for receipt in reversed(receipts):
        for change in reversed(receipt['changes']):
            if change['kind'] == 'Modified':
                ops.append({
                    "op": "edit",
                    "anchor": f"{change['line_no']}:{change['hash_after']}",
                    "content": change['before']
                })
            elif change['kind'] == 'Inserted':
                ops.append({
                    "op": "delete",
                    "anchor": f"{change['line_no']}:{change['hash_after']}"
                })
            elif change['kind'] == 'Deleted':
                # Re-insert before the next line; requires additional state
                # Full implementation in v2
                pass
    return { "file": target_file, "ops": ops }
```

---

## Appendix F — Exploded Directory Format

An exploded directory produced by `linehash explode` has the following structure:

```
.linehash/exploded/auth/
├── .meta.json
├── 0001_a3.txt
├── 0002_f1.txt
├── 0003_0e.txt
├── 0004_9c.txt
└── 0005_b2.txt
```

`.meta.json` content:
```json
{
  "source": "src/auth.js",
  "newline": "lf",
  "trailing_newline": true,
  "line_count": 5,
  "linehash_version": "0.1.0"
}
```

Individual line files:
- Content is the raw line content with NO newline terminator
- File is always UTF-8
- Filename `NNNN` portion is zero-padded to 4 digits in v1
- Hash in filename reflects the short hash at explode time
- After external edits, the filename hash may be stale — `implode` uses the
  filename for ordering only; it re-hashes the content on load

---

## Appendix G — Error Index

Quick reference of all `LinehashError` variants and their recovery hints.

| Variant | When | Hint |
|---|---|---|
| `Io` | Any filesystem operation fails | Check file permissions and disk space |
| `InvalidUtf8` | File has non-UTF-8 bytes | File must be UTF-8 |
| `MixedNewlines` | File has both LF and CRLF | Run `dos2unix` or `unix2dos` |
| `BinaryFile` | File has null bytes | Only text files are supported |
| `InvalidAnchor` | Anchor string is malformed | Use `N:hash` or `hash` format |
| `InvalidRange` | Range string is malformed | Use `N:hash..N:hash` format |
| `HashNotFound` | Hash not in current file | Re-read the file to get current hashes |
| `AmbiguousHash` | Hash matches 2+ lines | Use qualified form `N:hash` |
| `StaleAnchor` | Line N has a different hash | Re-read the file |
| `MultiLineContentUnsupported` | Content contains `\n` or `\r` | Use `patch` for multi-line ops |
| `EmptyFile` | File has zero lines | No anchor to resolve |
| `StaleFile` | mtime/inode mismatch | Re-read the file before editing |
| `PatchFailed` | Op N in patch is invalid | Fix op N and re-run patch |
| `InvalidPatch` | JSON is malformed or missing fields | Validate against patch schema |
| `InvalidPattern` | Regex is invalid | Check regex syntax |
| `SwapSameLine` | anchor-a and anchor-b are the same line | Use two different anchors |
| `InvalidIndentRange` | Range start > range end | Swap start and end |
| `IndentUnderflow` | Dedent by N on a line with <N leading chars | Reduce N or use a narrower range |
| `UnbalancedBlock` | Brace counting didn't close | Check file for unmatched braces |
| `AmbiguousBlockLanguage` | Can't determine brace vs indent | Use an explicit range anchor |
| `DiffHunkMismatch` | Hunk content doesn't match current file | Re-generate the diff from current file |
| `DiffFileMismatch` | Diff targets a different file | Check file argument |
| `PatchConflict` | Two patches target the same anchor | Resolve manually |
| `WatchUnsupported` | Platform doesn't support file watching | Not supported on this OS |
| `ExplodeTargetExists` | Output directory already exists | Use `--force` to overwrite |
| `ImplodeDirty` | Directory has non-linehash files | Clean the directory manually |

---

## Appendix H — Integration with Other Tools

### With `jq`

```bash
# Get just the mtime from a read --json result
linehash read src/auth.js --json | jq .mtime

# Get all collision pairs
linehash stats src/auth.js --json | jq '.collision_pairs[]'

# Get receipt line changes
linehash edit src/auth.js 2:f1 "new content" --receipt | jq '.changes[0].hash_after'
```

### With `git`

```bash
# Generate a linehash patch from a git diff
git diff HEAD src/auth.js | linehash from-diff src/auth.js -

# Preview what a git diff would change using linehash
git diff HEAD src/auth.js | linehash from-diff src/auth.js - | linehash patch src/auth.js - --dry-run

# Stage a specific anchor's line for commit
linehash grep src/auth.js "SECRET_KEY" --json | jq -r '.[0].n' | xargs -I{} git add -p src/auth.js
```

### With `fzf`

```bash
# Interactively pick a line to edit by content
linehash read src/auth.js | fzf | awk '{print $1}' | xargs -I{} linehash edit src/auth.js {} "new content"
```

### With `watch` (system command, not linehash watch)

```bash
# Continuously show hash changes as you edit (uses system watch, not linehash watch)
watch -n 1 'linehash index src/auth.js'
```

### With Claude Code

The primary integration. See the CLAUDE.md block in the Integration section of
this plan. Additional agent tips:

**Token-efficient pattern:**
```
1. linehash stats <file>          → decide: full read or context read?
2. linehash read <file>           → get all anchors (small file)
   OR
   linehash grep <file> "pattern" → get relevant anchors (large file)
3. linehash verify <file> <anchors...> → confirm anchors are live
4. linehash edit <file> <anchor> "new content" --expect-mtime <mtime>
5. linehash read <file> --anchor <new_anchor> --context 3  → confirm change
```

**Batch edit pattern:**
```
# Collect all changes into a patch file, then apply atomically
cat > changes.json << 'EOF'
{
  "file": "src/auth.js",
  "ops": [
    { "op": "edit",   "anchor": "2:f1", "content": "..." },
    { "op": "insert", "anchor": "4:9c", "content": "..." },
    { "op": "delete", "anchor": "7:b2" }
  ]
}
EOF
linehash patch src/auth.js changes.json --dry-run
linehash patch src/auth.js changes.json --receipt
```

---

## Appendix I — Collision Analysis for Common Codebases

To calibrate expectations, here are empirical collision statistics for real
source files:

| File | Lines | Collisions | Collision % | Notes |
|---|---|---|---|---|
| `tokio/src/runtime/mod.rs` | 423 | 142 | 33.6% | Dense Rust code |
| `react/packages/react/src/React.js` | 72 | 12 | 16.7% | Short file |
| CPython `Lib/email/parser.py` | 149 | 38 | 25.5% | Python |
| Linux `kernel/sched/core.c` | 10,063 | ~9,900 | ~98% | Very large file |
| `package.json` (typical) | 30 | 2 | 6.7% | Few collisions |

**Takeaways:**
- For files > 100 lines, expect 15–35% collision rates with 2-char hashes
- Most collisions involve whitespace-only lines or short lines like `}` or `{`
- The `stats` command will identify collision-prone files before editing
- For large files (>1000 lines), `stats.hash_length_advice` will recommend 3-char hashes

**Why collisions aren't catastrophic:**
Ambiguous anchors require the user/agent to qualify with line number.
This is a 1-time cost: once you read the file and note the anchor, you have the
qualified form. The tool never guesses — it always errors conservatively.

---

## Appendix J — Benchmark Results (Target)

These are the performance targets. Actual results should be verified after
implementation and included in the repository's `bench-results/` directory.

| Benchmark | Target | Acceptable |
|---|---|---|
| Hash 10k-line file (load only) | <5ms | <10ms |
| Hash 1k-line file (load only) | <1ms | <2ms |
| `read` command (1k lines, pretty) | <10ms wall clock | <20ms |
| `edit` command (1k lines, single line) | <5ms wall clock | <10ms |
| `patch` command (1k lines, 10 ops) | <10ms wall clock | <20ms |
| `stats` command (1k lines) | <5ms wall clock | <10ms |
| `explode` (100 lines) | <50ms wall clock | <100ms |
| `implode` (100 lines) | <50ms wall clock | <100ms |
| `grep` (1k lines, simple regex) | <5ms wall clock | <10ms |
| `find-block` (1k lines) | <5ms wall clock | <10ms |

Wall clock times include process startup (~2ms on Linux, ~5ms on macOS with dyld).

---

## Appendix K — Glossary

| Term | Definition |
|---|---|
| **Anchor** | A string reference to a line, consisting of a short hash and optionally a line number qualifier |
| **Short hash** | A 2-character lowercase hexadecimal string derived from the lowest byte of the xxh32 hash of a line's raw content |
| **Full hash** | The 32-bit xxh32 hash stored internally; only the short hash is exposed in output |
| **Qualified anchor** | An anchor with an explicit line number prefix, e.g., `2:f1` |
| **Unqualified anchor** | An anchor with no line number, e.g., `f1` |
| **Stale anchor** | A qualified anchor where the line number no longer has the expected hash |
| **Ambiguous anchor** | An unqualified anchor where the hash appears on more than one line |
| **Document** | The in-memory representation of a file: a `Vec<LineRecord>` plus metadata |
| **LineRecord** | A single line: its 1-based line number, raw content, short hash, and full hash |
| **Patch** | A JSON file describing a set of ordered operations (edit/insert/delete) to apply atomically |
| **Receipt** | A JSON record of a completed mutation, including before/after content and hashes |
| **Audit log** | A JSONL file accumulating all receipts; foundation for undo in v2 |
| **Atomic write** | The tempfile+rename strategy that ensures a file is never partially overwritten |
| **Concurrency guard** | The `--expect-mtime` / `--expect-inode` mechanism for detecting intervening modifications |
| **Context read** | A `read --anchor N:hash --context N` call that shows only the neighborhood of an anchor |
| **Exploded directory** | A directory produced by `linehash explode` containing one file per line |
| **Collision** | When two different lines produce the same short hash |
| **Stale read** | Reading a file, then having it modified by another process before editing |
| **Token budget** | The estimated number of LLM tokens required to read a file in full; used by `stats` |

---

## Appendix L — Frequently Asked Questions

**Q: Why xxh32 instead of SHA-256 or MD5?**

A: We only need 2 hex chars (1 byte) of output. xxh32 is extremely fast
(multiple GB/s) and produces 32-bit values, of which we use the lowest byte.
Using SHA-256 for this purpose would be pointless complexity — cryptographic
strength is irrelevant here. The hash is not a security primitive; it's a
change-detection tag.

**Q: Why 2-char hashes instead of 4-char?**

A: Two chars (256 possible values) minimizes the text added to each line in
the output. The display overhead of `N:hash| ` is ~8 chars per line. With 4
chars it would be ~10 chars. For a 1000-line file, that's 2000 extra chars
(~500 tokens) in the context window. Collisions are handled by requiring
qualification; they're not errors. `stats` helps users understand their
collision situation before committing to a hash length.

**Q: Why not trim whitespace before hashing?**

A: Because then `"  return x"` and `"return x"` would have the same hash, and
the tool would silently "succeed" when an agent edits the wrong indentation level.
This is especially dangerous in Python. The hash must reflect the line exactly.

**Q: What happens if the same line appears twice in a file?**

A: Those lines get the same short hash, creating a collision. The unqualified
anchor `f1` resolves to an `AmbiguousHash` error. The user must use `2:f1` or
`7:f1` to disambiguate. This is the correct behavior — silently choosing one of
two identical-looking lines would be dangerous.

**Q: Does `linehash` work with binary files?**

A: No. The tool reads files as UTF-8 and rejects non-UTF-8 bytes. For binary
files, there are better tools. Binary content in a source file (e.g., embedded
images in a JS bundle) is a sign that the wrong file is being edited.

**Q: Is linehash safe to use in a CI pipeline?**

A: Yes. All commands are stateless and produce deterministic output. The `verify`
command is particularly useful in CI for asserting that specific lines haven't
changed unintentionally.

**Q: What happens to line numbers in --receipt after an insert?**

A: The receipt records the `line_no` at the time of the write (post-mutation).
So an insert after line 5 reports `line_no: 6` in the receipt (the new line's
final position). This is intentional: the receipt should reflect where the change
ended up, not where it started.

**Q: Can linehash be used on files with Windows-style paths?**

A: Yes. All path handling uses `std::path::PathBuf`, which is platform-aware.
Path separators in `--audit-log` paths and explode `--out` paths are normalized
by the OS.

**Q: What is the maximum file size linehash supports?**

A: There is no enforced limit. The tool reads the entire file into memory. For
files >10MB, memory usage may become noticeable. For files >100MB, a warning is
printed to stderr. The practical limit is available RAM. For very large generated
files, use `grep` or `annotate` to avoid full reads.