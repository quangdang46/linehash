# linehash

> Stable line-addressed file reading and editing for Claude Code.
> Every line gets a 2-char content hash. Edit by hash, not by reproducing whitespace.

---

## The Problem

Claude Code uses `str_replace` to edit files — the model must reproduce the **exact** old text,
character by character, including whitespace and indentation.

The "String to replace not found in file" error has its own GitHub issues megathread
with 27+ related issues. It's not the model being dumb — it's the format demanding perfect recall.

From Can Bölük's harness benchmark across 16 models:
- `str_replace` failure rate: up to **50.7%** on some models
- Root cause: models can't reliably reproduce exact whitespace

## The Fix: Content-Hashed Lines

When Claude reads a file via `linehash read`, every line gets a stable 2-char hash:

```
1:a3| function verifyToken(token) {
2:f1|   const decoded = jwt.verify(token, process.env.SECRET)
3:0e|   if (!decoded.exp) throw new TokenError('missing expiry')
4:9c|   return decoded
5:b2| }
```

When Claude edits, it references hashes as anchors:

```bash
# Replace a single line
linehash edit src/auth.js 2:f1 "  const decoded = jwt.verify(token, env.SECRET)"

# Replace a range
linehash edit src/auth.js 2:f1..4:9c "  return jwt.verify(token, env.SECRET)"

# Insert after a line
linehash insert src/auth.js 3:0e "  if (!decoded.iat) throw new TokenError('missing iat')"

# Delete a line
linehash delete src/auth.js 3:0e
```

If the file changed since last read, hashes won't match → edit **rejected** before corruption.

## Why This Is Better Than str_replace

| | str_replace | linehash |
|---|---|---|
| Model must reproduce whitespace | ✅ required | ❌ not needed |
| Stable after file changes | ❌ line numbers shift | ✅ hash tied to content |
| Edit failure rate | Up to 50% | Near 0% |
| Detects stale reads | ❌ | ✅ hash mismatch = reject |
| Token cost | High (full old content) | Low (just hash + new line) |

## How Hashes Work

Each hash is a **2-char truncated xxHash** of the line content (trimmed):

```
line content → xxhash32 → take first 2 hex chars
"  return decoded" → 0x9c4f... → "9c"
```

- Same content = same hash (stable across reads)
- Different content = different hash (edit safety)
- 2 chars = 256 possible values — good enough for line-level anchoring
- Collisions are rare and recoverable (linehash detects ambiguity)

## Tech Stack

| Crate | Purpose |
|---|---|
| `xxhash-rust` | Fast content hashing per line |
| `clap` | CLI |
| `serde_json` | `--json` output for scripts |

Pure Rust. No tree-sitter. No LLM. No external dependencies.
Simplest tool in the suite.

## Installation

```bash
cargo install linehash
```

## Usage

```bash
# Read file with hash tags
linehash read src/auth.js

# Read just the neighborhood around one or more anchors
linehash read src/auth.js --anchor 2:f1 --context 2

# View just line numbers + hashes (no content) — for orientation
linehash index src/auth.js

# Edit by hash anchor
linehash edit <file> <hash-or-line:hash> <new_content>
linehash edit <file> <start-line:hash>..<end-line:hash> <new_content>
linehash insert <file> <hash-or-line:hash> <new_line>     # insert AFTER anchor line
linehash insert <file> <hash-or-line:hash> <new_line> --before
linehash delete <file> <hash-or-line:hash>

# Inspect collision/token-budget guidance for large files
linehash stats src/auth.js

# Watch for live hash changes (v1 defaults to one change event, then exit)
linehash watch src/auth.js
linehash watch src/auth.js --continuous
```

## Integration with Claude Code

Add to your project's `CLAUDE.md`:

```markdown
## File Editing Rules

When editing an existing file with linehash:

1. Read: `linehash read <file>`
2. Copy the anchor as `line:hash` (for example `2:f1`) — do not include the trailing `|`
3. Edit using the anchor only; never reproduce old content just to locate the line
4. If the file may have changed, prefer `linehash read <file> --json` first and carry `mtime` / `inode` into mutation commands with `--expect-mtime` / `--expect-inode`
5. If an edit is rejected as stale or ambiguous, re-read and retry with a fresh qualified anchor

Example:
  linehash read src/auth.js
  # line 2 shows as `2:f1|   const decoded = ...`
  linehash edit src/auth.js 2:f1 "  const decoded = jwt.verify(token, env.SECRET)"
```

### Recommended agent workflow

- Use `read` for the full file view.
- Use `read --anchor ... --context N` when you already know the target anchor and want a smaller local window.
- Use `index` for fast orientation when content is not needed.
- Use `stats` when a file is large, collisions are likely, or you want guidance on whether short hashes and small context windows are still ergonomic.
- Use qualified anchors like `12:ab` whenever possible; they are safer than bare `ab` when collisions or stale reads matter.

## Output Modes

```bash
# Pretty (default) — for Claude to read
linehash read src/auth.js
  1:a3| function verifyToken(token) {
  2:f1|   const decoded = jwt.verify(token, SECRET)
  ...

# JSON — for scripts and stale-guard workflows
linehash read src/auth.js --json
{
  "file": "src/auth.js",
  "newline": "lf",
  "trailing_newline": true,
  "mtime": 1714001321,
  "mtime_nanos": 0,
  "inode": 12345,
  "lines": [
    { "n": 1, "hash": "a3", "content": "function verifyToken(token) {" },
    { "n": 2, "hash": "f1", "content": "  const decoded = jwt.verify(token, SECRET)" },
    ...
  ]
}

# NDJSON event stream for agents / scripts
linehash watch src/auth.js --json
{"timestamp":1714001321,"event":"changed","path":"src/auth.js","changes":[...],"total_lines":847}
```

## Error Handling

```bash
# Hash not found
linehash edit src/auth.js xx "new content"
Error: hash 'xx' not found in src/auth.js
Hint: run `linehash read <file>` to get current hashes

# Ambiguous hash (collision)
linehash edit src/auth.js f1 "new content"
Error: hash 'f1' matches 3 lines in src/auth.js (lines 2, 14, 67)
Hint: use a line-qualified hash like '2:f1' to disambiguate

# File changed since read (stale qualified anchor)
linehash edit src/auth.js 2:f1 "new content"
Error: line 2 content changed since last read in src/auth.js (expected hash f1, got 3a)
Hint: re-read the file with `linehash read <file>` and retry the edit

# File metadata changed since JSON read / guard capture
linehash edit src/auth.js 2:f1 "new content" --expect-mtime 1714001321 --expect-inode 12345
Error: file 'src/auth.js' changed since the last read
Hint: re-read the file metadata and retry with fresh --expect-mtime/--expect-inode values
```

## Recovery loops

- **Stale anchor:** re-run `linehash read <file>` or `linehash read <file> --json`, then retry with a fresh `line:hash` anchor.
- **Ambiguous hash:** switch from bare `ab` to qualified `12:ab`.
- **Large file / too much output:** use `index`, `stats`, or `read --anchor ... --context N` instead of a full read.
- **Concurrent edits:** treat a stale-anchor or stale-file rejection as success of the safety system, not as something to bypass.

---

## Roadmap

- [ ] `linehash diff` — show pending edits before applying
- [ ] `linehash undo` — revert last edit
- [ ] Multi-line insert block support
- [ ] Integration test suite against real codebases
