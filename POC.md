# POC: linehash

> Proof of concept — hash-tagged line reading + hash-anchored editing.
> Demonstrates why anchor-based edits are more reliable than `str_replace` when whitespace differs.

---

## POC Goal

Prove:
1. Line hashing is stable and fast enough for a simple CLI workflow
2. Edit-by-hash produces correct file output
3. Stale hash detection works reliably
4. Bare-hash ambiguity is detectable and recoverable
5. Claude Code can be instructed to use this workflow

---

## POC Scope

Pure Node.js — no dependencies except built-ins.
This POC is intentionally small and isolated from the Rust workspace.

Implemented files:
- `poc/linehash.js`
- `poc/benchmark.js`

---

## POC Behavior

The implemented Node POC supports:
- `read <file> [--json]`
- `edit <file> <hash-or-line:hash> <new content>`
- `edit <file> <start-line:hash>..<end-line:hash> <new content>`
- `insert <file> <hash-or-line:hash> <new line>`

### Hash semantics

The POC uses a 2-character MD5-derived hash of the raw line content:

```javascript
crypto.createHash('md5').update(content, 'utf8').digest('hex').slice(-2)
```

Important notes:
- Hashes are **whitespace-sensitive**
- Leading and trailing spaces affect the hash
- This matches the current tool behavior better than the older `trim()`-based draft
- The production Rust tool uses `xxhash-rust`, not MD5

### Safety behavior

The POC implements the same core safety properties as the main tool:
- qualified anchors like `2:92` fail if line 2 changed
- bare hashes like `92` fail if they match multiple lines
- range edits require qualified anchors on both ends
- newline style and trailing newline are preserved when rewriting files

---

## POC Code

### `poc/linehash.js`

Key responsibilities:
1. Read a file and assign each line a short content hash
2. Print `line:hash| content` output for agents/humans
3. Resolve anchors safely before editing
4. Reject stale qualified anchors
5. Reject ambiguous bare hashes
6. Support single-line edit, range edit, and insert-after

Current implementation summary:

```javascript
#!/usr/bin/env node
'use strict'

const fs = require('fs')
const crypto = require('crypto')

function shortHash(content) {
  return crypto.createHash('md5').update(content, 'utf8').digest('hex').slice(-2)
}

function readDocument(filePath) {
  // read file, preserve newline style, return:
  // { file, newline, trailing_newline, lines: [{ n, hash, content }] }
}

function resolveAnchor(doc, anchorText) {
  // supports:
  // - bare hash: "92"
  // - qualified hash: "2:92"
  // rejects:
  // - missing hash
  // - ambiguous bare hash
  // - stale qualified hash
}

function editCommand(filePath, anchorOrRange, newContent) {
  // supports single-line and qualified range edits
}

function insertCommand(filePath, anchorText, newContent) {
  // inserts after resolved anchor line
}
```

The actual implementation lives in:
- `poc/linehash.js`

---

## Run the POC

```bash
# No npm install needed — pure Node.js built-ins

# Read a file with hashes
node poc/linehash.js read /tmp/demo.txt

# Read JSON metadata + lines
node poc/linehash.js read /tmp/demo.txt --json

# Example file
printf 'alpha\nbeta\ngamma\n' > /tmp/demo.txt

# Read anchors
node poc/linehash.js read /tmp/demo.txt
# → 1:f9| alpha
# → 2:92| beta
# → 3:ea| gamma

# Edit line by qualified anchor
node poc/linehash.js edit /tmp/demo.txt 2:92 "BETA"

# Insert after a line
node poc/linehash.js insert /tmp/demo.txt 2:46 "inserted"

# Replace a range with one line
node poc/linehash.js edit /tmp/demo.txt 2:46..3:19 "merged"
```

---

## Verified Safety Checks

### Stale qualified anchor

```bash
printf 'alpha\nbeta\ngamma\n' > /tmp/stale.txt
node poc/linehash.js read /tmp/stale.txt
# line 2 is 2:92

python - <<'PY'
from pathlib import Path
p = Path('/tmp/stale.txt')
p.write_text('alpha\nBETA\ngamma\n', encoding='utf-8')
PY

node poc/linehash.js edit /tmp/stale.txt 2:92 "updated"
# → Error: line 2 content changed since last read in /tmp/stale.txt (expected hash 92, got 46)
```

### Ambiguous bare hash

```bash
printf 'line-7\nline-30\n' > /tmp/ambiguous.txt
node poc/linehash.js edit /tmp/ambiguous.txt 1e changed
# → Error: hash '1e' matches 2 lines in /tmp/ambiguous.txt (lines 1, 2)
```

---

## POC Benchmark: `str_replace` vs linehash

The benchmark is a deterministic demo, not a throughput benchmark.

It compares:
1. `str_replace`-style matching by exact old text
2. linehash editing by current anchor

### `poc/benchmark.js`

Implemented behavior:
- create a temporary file
- intentionally use the wrong whitespace for the `str_replace` case
- derive the real anchor from `readDocument(...)`
- edit by anchor
- confirm the file changed as expected
- clean up the temp directory

Core idea:

```javascript
function strReplace(filePath, oldStr, newStr) {
  const content = fs.readFileSync(filePath, 'utf8')
  if (!content.includes(oldStr)) {
    throw new Error('String to replace not found')
  }
  fs.writeFileSync(filePath, content.replace(oldStr, newStr), 'utf8')
}

const doc = readDocument(filePath)
const anchor = `${doc.lines[1].n}:${doc.lines[1].hash}`
editCommand(filePath, anchor, '  return "universe"')
```

---

## Expected Benchmark Output

```text
str_replace: ✗ FAILED — String to replace not found
Edited line 2.
linehash:    ✓ success (anchor-based edit works)
anchor used: 2:26
```

Note:
- the exact anchor value is content-dependent
- `2:26` is an example from the verified run, not a hard-coded constant

---

## Key Insight for Claude Code

With a `CLAUDE.md` instruction like:

```markdown
Use `linehash read <file>` instead of reproducing old content by memory.
Use `linehash edit <file> <line:hash> <content>` instead of fragile text replacement.
Prefer qualified anchors when possible.
If an edit fails as stale or ambiguous, re-read and retry with a fresh anchor.
```

Claude Code edit reliability improves because:
1. It does not need to reproduce exact old text just to locate the edit target
2. Qualified anchors detect stale reads before corruption
3. Bare-hash ambiguity can be surfaced explicitly
4. The edit payload is just the anchor plus new content

---

## Relationship to the Rust tool

This Node POC is intentionally minimal.

Differences from the Rust implementation:
- Node POC uses MD5-derived short hashes
- Rust uses `xxhash-rust`
- Rust includes many more commands (`delete`, `verify`, `grep`, `patch`, `watch`, etc.)
- Rust has fuller guard and receipt support

Shared ideas:
- `line:hash| content` read format
- short content hashes as anchors
- stale qualified-anchor rejection
- ambiguous bare-hash rejection
- range replacement collapsing multiple lines into one line

---

## Next Steps (Production Rust)

The main Rust tool already covers far more than this POC. Remaining production-oriented work belongs there rather than in `poc/`.

Potential future work:
1. `linehash undo` via `.linehash/history/` backup
2. More real-world integration testing against larger codebases
3. Additional agent workflow guidance in `CLAUDE.md`
4. Performance measurement against larger files and patch workflows
