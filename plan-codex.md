# linehash Rust Implementation Plan

## Goal

Build a small, dependable Rust CLI that lets an agent read a text file with short per-line hashes and then edit, insert, or delete lines by hash anchor instead of reproducing exact old text.

The first release should optimize for:

- **safety**: reject stale or ambiguous edits instead of guessing
- **predictability**: simple, explicit CLI behavior
- **low integration friction**: easy for Claude Code to adopt
- **small surface area**: no parser, no AST, no daemon, no persistent service

---

## V1 scope

### Must ship

- `linehash read <file>`
- `linehash index <file>`
- `linehash edit <file> <anchor> <new_content>`
- `linehash edit <file> <start>..<end> <new_content>`
- `linehash insert <file> <anchor> <new_content>`
- `linehash delete <file> <anchor>`
- pretty output and `--json`
- atomic writes
- clear ambiguity and stale-read errors
- tests for core resolution and file rewrite behavior

### Explicitly out of scope for v1

- `diff`
- `undo`
- multi-line block insert/replacement
- persistent read snapshots
- move-tolerant anchor recovery
- non-UTF-8 support
- editor plugins

---

## Recommended spec decisions

These decisions should be frozen before much code is written.

### 1) Hash the raw line bytes, excluding only the newline terminator

**Recommendation:** hash the exact line content as stored in the file, excluding `\n` or `\r\n`.

Example:

- file bytes: `"  return decoded\n"`
- hashed content: `"  return decoded"`

Do **not** trim leading or trailing whitespace in v1.

**Why:** trimming weakens stale-read detection for whitespace-only edits, which is especially risky in indentation-sensitive formats like Python and YAML.

### 2) Preserve file formatting exactly where possible

For each file read:

- detect newline style: LF or CRLF
- detect whether the file ends with a trailing newline
- preserve both when writing back

If the file mixes newline styles, treat that as an error in v1 unless you intentionally choose a normalization rule.

**Recommendation:** fail on mixed newline styles in v1 with a helpful message.

### 3) UTF-8 only in v1

Read the file as UTF-8 text.

- valid UTF-8: supported
- invalid UTF-8: return a clear error

This keeps the implementation small and predictable. If you later need arbitrary bytes, redesign around `bstr` or byte slices.

### 4) Canonical anchor display is `line:hash`

Display anchors in pretty output as:

```text
2:f1|   const decoded = jwt.verify(token, SECRET)
```

Accepted input forms:

- `f1` → unqualified short hash
- `2:f1` → line-qualified short hash
- `2:f1..4:9c` → inclusive range

### 5) Safety-first anchor resolution in v1

#### Unqualified anchor: `f1`

- 0 matches → `hash not found`
- 1 match → resolve to that line
- 2+ matches → `ambiguous hash`, show candidate lines

#### Qualified anchor: `2:f1`

- if line 2 currently has hash `f1` → resolve
- if line 2 does not have hash `f1`, return a **stale anchor** error
- if `f1` also exists elsewhere, include that in the error message, but do not silently retarget

This is intentionally conservative. It rejects moved lines instead of guessing.

### 6) Single logical line content only in v1

`edit` and `insert` should reject `new_content` containing `\n` or `\r`.

That means:

- single-line replacement is supported
- range replacement with exactly one line is supported
- multi-line insert/replacement is deferred

### 7) Read-whole-file approach is acceptable

For v1, read the entire file into memory, transform it, and write it back atomically.

That is fine for normal source files and keeps the code simple.

---

## CLI contract

### Pretty output

```bash
linehash read src/auth.js
1:a3| function verifyToken(token) {
2:f1|   const decoded = jwt.verify(token, SECRET)
3:0e|   if (!decoded.exp) throw new TokenError('missing expiry')
4:9c|   return decoded
5:b2| }
```

### JSON output

Recommended schema:

```json
{
  "file": "src/auth.js",
  "newline": "lf",
  "trailing_newline": true,
  "lines": [
    { "n": 1, "hash": "a3", "content": "function verifyToken(token) {" },
    { "n": 2, "hash": "f1", "content": "  const decoded = jwt.verify(token, SECRET)" }
  ]
}
```

### Recommended error messages

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

---

## Rust crate choices

### Runtime dependencies

- `clap` with `derive` → CLI parsing
- `xxhash-rust` → xxh32 hashing
- `serde` with `derive` → JSON serialization
- `serde_json` → `--json` output
- `thiserror` → domain errors
- `tempfile` → atomic file rewrite in same directory
- `anyhow` → thin top-level error handling in `main`

### Dev dependencies

- `assert_cmd` → CLI integration tests
- `predicates` → output assertions
- `insta` → snapshot tests for `read` and `index`
- `tempfile` → temporary fixtures in tests

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
cargo add --dev assert_cmd predicates insta tempfile
```

---

## Suggested project layout

Keep the codebase small and testable.

```text
src/
  main.rs
  cli.rs
  error.rs
  hash.rs
  anchor.rs
  document.rs
  output.rs
  writeback.rs
  commands/
    mod.rs
    read.rs
    index.rs
    edit.rs
    insert.rs
    delete.rs
tests/
  read_cli.rs
  index_cli.rs
  edit_cli.rs
  insert_cli.rs
  delete_cli.rs
  fixtures/
```

### Layout notes

- keep CLI parsing separate from business logic
- keep file parsing and rewrite logic separate from command dispatch
- make `document`, `anchor`, and `writeback` unit-testable without invoking the CLI

---

## Core data model

```rust
pub struct LineRecord {
    pub number: usize,
    pub content: String,
    pub short_hash: String,
    pub full_hash: u32,
}

pub enum NewlineStyle {
    Lf,
    Crlf,
}

pub struct Document {
    pub path: std::path::PathBuf,
    pub newline: NewlineStyle,
    pub trailing_newline: bool,
    pub lines: Vec<LineRecord>,
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
    pub index: usize,
    pub line_no: usize,
    pub short_hash: String,
}
```

### Internal guidance

- keep `full_hash` internal even if you only display 2 chars
- `short_hash` should always be lowercase hex
- `index` is zero-based for internal mutations
- `line_no` is one-based for UX and messages

---

## File loading and hashing

### Load algorithm

1. Read file bytes
2. Decode as UTF-8
3. Detect newline style:
   - only `\n` → LF
   - only `\r\n` → CRLF
   - mixed → error in v1
4. Split into logical lines without newline terminators
5. Detect trailing newline
6. Compute `xxh32` over each line's raw content
7. Store full hash and 2-char short hash

### Hash function

Recommended implementation shape:

```rust
pub fn full_hash(line: &str) -> u32;
pub fn short_hash(line: &str) -> String;
```

Behavior:

- full hash → `xxh32(line.as_bytes(), 0)`
- short hash → first 2 lowercase hex characters of the full hash

---

## Anchor parsing and resolution

### Parsing

Support:

- `f1`
- `2:f1`
- `2:f1..4:9c`

Validation rules:

- line number must be >= 1
- short hash must be exactly 2 hex chars in v1
- normalize uppercase to lowercase
- range syntax must contain exactly one `..`

### Resolution data structure

Build an index on read:

```rust
HashMap<String, Vec<usize>> // short_hash -> line indexes
```

This keeps lookups simple.

### Resolution algorithm

#### Unqualified anchor

1. look up short hash in the map
2. no matches → not found
3. one match → resolve
4. many matches → ambiguous

#### Qualified anchor

1. verify the referenced line exists
2. compare the current line's short hash
3. exact match → resolve
4. mismatch → stale anchor error

Do not silently fall back to another line in v1.

### Range resolution

1. resolve start anchor
2. resolve end anchor
3. ensure start index <= end index
4. replace the inclusive slice with one new line

---

## Command behavior

### `read`

- load document
- print each line as `N:hh| content`
- `--json` prints structured output

### `index`

- load document
- print only `N:hh`
- `--json` prints line numbers and hashes without content

### `edit` (single line)

Input:

```bash
linehash edit <file> <anchor> <new_content>
```

Algorithm:

1. load document
2. reject `new_content` if it contains newline characters
3. parse anchor
4. resolve line
5. replace line content
6. rewrite file atomically

### `edit` (range)

Input:

```bash
linehash edit <file> <start>..<end> <new_content>
```

Algorithm:

1. load document
2. reject multi-line `new_content`
3. parse range
4. resolve both anchors
5. replace inclusive range with one new line
6. rewrite file atomically

### `insert`

Input:

```bash
linehash insert <file> <anchor> <new_content>
```

Algorithm:

1. load document
2. reject multi-line `new_content`
3. resolve anchor
4. insert one line **after** the resolved line
5. rewrite file atomically

### `delete`

Input:

```bash
linehash delete <file> <anchor>
```

Algorithm:

1. load document
2. resolve anchor
3. remove the line
4. rewrite file atomically

### Empty-file behavior

Recommended v1 behavior:

- `read` and `index` work on empty files
- `edit`, `insert`, and `delete` return a clear error if there is no resolvable anchor

---

## Atomic write strategy

### Requirements

- never partially overwrite the target file
- preserve permissions where practical
- write temp file in the same directory as the target
- rename into place only after successful write and flush

### Recommended approach

1. load original metadata
2. create `NamedTempFile` in the target directory
3. render the updated file contents using the original newline style and trailing-newline state
4. write all bytes
5. `flush` and `sync_all`
6. apply original permissions where relevant
7. `persist` over the target path

### Rendering rules

- join logical lines with the original newline style
- append a final newline only if the original file had one and there is at least one line remaining
- if the resulting file is empty, write zero bytes

---

## Error model

Use a dedicated error enum with `thiserror`.

Suggested variants:

```rust
pub enum LinehashError {
    Io(std::io::Error),
    InvalidUtf8,
    MixedNewlines,
    InvalidAnchor(String),
    InvalidRange(String),
    HashNotFound { hash: String },
    AmbiguousHash { hash: String, lines: Vec<usize> },
    StaleAnchor { line: usize, expected: String, actual: String },
    EmptyFile,
    MultiLineContentUnsupported,
}
```

### Exit behavior

- usage / parse errors → normal `clap` behavior
- business logic errors → print concise error + hint, exit non-zero

---

## Test strategy

### Unit tests

Cover these in pure Rust tests:

- short hash formatting
- newline detection
- trailing newline detection
- anchor parsing
- unqualified resolution
- qualified resolution
- ambiguity detection
- stale anchor detection
- range validation
- content rendering with LF and CRLF

### Integration tests

Use `assert_cmd` and temp files for:

- `read` pretty output
- `read --json`
- `index` pretty output
- edit single line
- edit range
- insert after line
- delete line
- not found error
- ambiguous hash error
- stale qualified anchor error
- CRLF preservation
- no trailing newline preservation
- duplicate line handling
- empty file behavior
- invalid UTF-8 rejection

### Snapshot tests

Use `insta` for stable CLI output from:

- `read`
- `index`
- JSON output shapes

---

## Milestone plan

## Milestone 0 — freeze the spec

**Deliverable:** written decisions for hashing, newline preservation, anchor resolution, and v1 content limits.

**Done when:** there is no unresolved ambiguity about what `edit`, `insert`, and `delete` should do.

## Milestone 1 — bootstrap the crate

**Deliverable:** compiles, parses subcommands, prints placeholders.

**Done when:**

- all CLI commands exist
- arguments parse cleanly
- `cargo test` runs

## Milestone 2 — document loading and hashing

**Deliverable:** `Document::load` plus hash generation.

**Done when:**

- LF and CRLF work
- mixed newline files fail clearly
- hashes are deterministic

## Milestone 3 — `read` and `index`

**Deliverable:** first user-visible commands.

**Done when:**

- pretty output matches the spec
- `--json` works
- snapshot tests pass

## Milestone 4 — anchor parsing and resolution

**Deliverable:** robust anchor handling with good errors.

**Done when:**

- `f1` and `2:f1` resolve correctly
- ambiguous and stale cases are distinct
- range resolution works

## Milestone 5 — mutation commands

**Deliverable:** `edit`, `insert`, and `delete` with atomic writes.

**Done when:**

- file content updates correctly
- newline style is preserved
- trailing newline behavior is preserved
- no partial writes occur

## Milestone 6 — polish and hardening

**Deliverable:** better errors, hints, and docs.

**Done when:**

- every user-facing error suggests the next step
- README examples work exactly as written
- integration tests cover failure paths

## Milestone 7 — release prep

**Deliverable:** first publishable version.

**Done when:**

- `cargo fmt --check` passes
- `cargo clippy --all-targets -- -D warnings` passes
- `cargo test` passes
- install and smoke-test instructions are verified

---

## Suggested implementation order

1. create crate and dependency scaffold
2. implement `Document::load`
3. implement hashing helpers
4. implement pretty and JSON output for `read`
5. implement `index`
6. implement anchor parser
7. implement anchor resolution
8. implement atomic writer
9. implement `edit`
10. implement `insert`
11. implement `delete`
12. add integration and snapshot tests
13. tighten errors and hints
14. validate README examples end-to-end

---

## Acceptance criteria for v1

Ship v1 only when all of the following are true:

- a unique unqualified hash resolves correctly
- a qualified anchor edits the intended line only when the current line still matches
- ambiguous hashes never silently choose a target
- stale anchors never silently choose a target
- CRLF files stay CRLF after edits
- files without trailing newline keep that state after edits
- read/index output is stable enough for agent consumption
- mutation commands are atomic and leave the file intact on failure

---

## Post-v1 roadmap

Once v1 is stable, the next most valuable additions are:

1. multi-line insert and replace
2. `linehash diff`
3. `linehash undo`
4. optional relaxed anchor resolution for moved lines
5. support for longer hashes when ambiguity is common
6. benchmark harness against `str_replace`

---

## One important product note

The current README text says hashes are computed from **trimmed** line content. If you want strong stale-read protection, change that before implementation and document the new rule explicitly.

The safest rule for Rust v1 is:

> hash the exact line bytes, excluding only the newline terminator.

That keeps the tool small while avoiding a major correctness footgun.
