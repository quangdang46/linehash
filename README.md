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

# Edit by hash anchor
linehash edit <file> <hash> <new_content>
linehash edit <file> <hash_start>..<hash_end> <new_content>
linehash insert <file> <hash> <new_line>     # insert AFTER hash line
linehash delete <file> <hash>

# View just line numbers + hashes (no content) — for orientation
linehash index src/auth.js
```

## Integration with Claude Code

Add to your project's `CLAUDE.md`:

```markdown
## File Editing Rules

ALWAYS use linehash instead of str_replace or write for editing existing files:

1. Read: `linehash read <file>` — note the hash tags on each line
2. Edit: `linehash edit <file> <hash> "<new content>"`
3. Never reproduce old content. Reference hash only.

Example:
  linehash read src/auth.js
  # see line 2:f1 has the jwt.verify call
  linehash edit src/auth.js 2:f1 "  const decoded = jwt.verify(token, env.SECRET)"
```

## Output Modes

```bash
# Pretty (default) — for Claude to read
linehash read src/auth.js
  1:a3| function verifyToken(token) {
  2:f1|   const decoded = jwt.verify(token, SECRET)
  ...

# JSON — for scripts
linehash read src/auth.js --json
{
  "file": "src/auth.js",
  "lines": [
    { "n": 1, "hash": "a3", "content": "function verifyToken(token) {" },
    { "n": 2, "hash": "f1", "content": "  const decoded = jwt.verify(token, SECRET)" },
    ...
  ]
}
```

## Error Handling

```bash
# Hash not found
linehash edit src/auth.js xx "new content"
Error: hash 'xx' not found in src/auth.js
Hint: run `linehash read src/auth.js` to get current hashes

# Ambiguous hash (collision)
linehash edit src/auth.js f1 "new content"
Error: hash 'f1' matches 3 lines (lines 2, 14, 67)
Use line-qualified hash: 2:f1, 14:f1, or 67:f1

# File changed since read (stale hash)
linehash edit src/auth.js 2:f1 "new content"
Error: line 2 content changed since last read (expected hash f1, got 3a)
Hint: re-read the file with `linehash read src/auth.js`
```

---

## Roadmap

- [ ] `linehash diff` — show pending edits before applying
- [ ] `linehash undo` — revert last edit
- [ ] Multi-line insert block support
- [ ] Integration test suite against real codebases
