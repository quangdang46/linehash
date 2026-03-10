#!/usr/bin/env node
'use strict'

const fs = require('fs')
const crypto = require('crypto')

function shortHash(content) {
  return crypto.createHash('md5').update(content, 'utf8').digest('hex').slice(-2)
}

function detectNewline(content) {
  const hasCrlf = content.includes('\r\n')
  const normalized = content.replace(/\r\n/g, '')
  const hasBareCr = normalized.includes('\r')
  if (hasBareCr) {
    throw new Error('unsupported newline style: bare CR')
  }
  const hasLf = normalized.includes('\n')
  if (hasCrlf && hasLf) {
    throw new Error('mixed newline styles are not supported')
  }
  return hasCrlf ? '\r\n' : '\n'
}

function splitDocument(content) {
  const newline = detectNewline(content)
  const trailingNewline = content.endsWith(newline)
  const body = trailingNewline ? content.slice(0, -newline.length) : content
  const lines = body.length === 0 ? [] : body.split(newline)
  return { newline, trailingNewline, lines }
}

function joinDocument(lines, newline, trailingNewline) {
  const body = lines.join(newline)
  if (lines.length === 0) {
    return trailingNewline ? newline : ''
  }
  return trailingNewline ? body + newline : body
}

function readDocument(filePath) {
  const content = fs.readFileSync(filePath, 'utf8')
  const parsed = splitDocument(content)
  const lines = parsed.lines.map((line, index) => ({
    n: index + 1,
    hash: shortHash(line),
    content: line,
  }))
  return {
    file: filePath,
    newline: parsed.newline === '\r\n' ? 'crlf' : 'lf',
    trailing_newline: parsed.trailingNewline,
    lines,
  }
}

function formatPretty(doc) {
  const width = String(doc.lines.length || 1).length
  return doc.lines
    .map((line) => `${String(line.n).padStart(width, ' ')}:${line.hash}| ${line.content}`)
    .join('\n')
}

function parseAnchor(anchorText) {
  const normalized = String(anchorText).trim().toLowerCase()
  if (!normalized) {
    throw new Error(`Invalid anchor '${anchorText}'`)
  }
  if (normalized.includes('..')) {
    throw new Error(`Invalid anchor '${anchorText}'`)
  }
  const qualified = normalized.match(/^(\d+):([0-9a-f]{2})$/)
  if (qualified) {
    const line = Number(qualified[1])
    if (line <= 0) {
      throw new Error(`Invalid anchor '${anchorText}'`)
    }
    return { type: 'line-hash', line, hash: qualified[2] }
  }
  if (/^[0-9a-f]{2}$/.test(normalized)) {
    return { type: 'hash', hash: normalized }
  }
  throw new Error(`Invalid anchor '${anchorText}'`)
}

function parseRange(rangeText) {
  const normalized = String(rangeText).trim().toLowerCase()
  const parts = normalized.split('..')
  if (parts.length !== 2) {
    throw new Error(`Invalid range '${rangeText}'`)
  }
  return {
    start: parseAnchor(parts[0]),
    end: parseAnchor(parts[1]),
  }
}

function resolveAnchor(doc, anchorText) {
  const anchor = typeof anchorText === 'string' ? parseAnchor(anchorText) : anchorText
  if (anchor.type === 'line-hash') {
    const line = doc.lines[anchor.line - 1]
    if (!line) {
      throw new Error(`Invalid anchor '${anchor.line}:${anchor.hash}'`)
    }
    if (line.hash === anchor.hash) {
      return line
    }
    const relocated = doc.lines.filter((entry) => entry.hash === anchor.hash).map((entry) => entry.n)
    const suffix = relocated.length > 0 ? `; hash still exists at line(s) ${relocated.join(', ')}` : ''
    throw new Error(
      `line ${anchor.line} content changed since last read in ${doc.file} (expected hash ${anchor.hash}, got ${line.hash})${suffix}`
    )
  }

  const matches = doc.lines.filter((line) => line.hash === anchor.hash)
  if (matches.length === 0) {
    throw new Error(`hash '${anchor.hash}' not found in ${doc.file}`)
  }
  if (matches.length > 1) {
    throw new Error(
      `hash '${anchor.hash}' matches ${matches.length} lines in ${doc.file} (lines ${matches.map((line) => line.n).join(', ')})`
    )
  }
  return matches[0]
}

function resolveRange(doc, rangeText) {
  const range = typeof rangeText === 'string' ? parseRange(rangeText) : rangeText
  const start = resolveAnchor(doc, range.start)
  const end = resolveAnchor(doc, range.end)
  if (start.n > end.n) {
    throw new Error(`Invalid range '${rangeText}'`)
  }
  return { start, end }
}

function writeLines(filePath, lines, originalDoc) {
  const newline = originalDoc.newline === 'crlf' ? '\r\n' : '\n'
  const content = joinDocument(lines, newline, originalDoc.trailing_newline)
  fs.writeFileSync(filePath, content, 'utf8')
}

function readCommand(filePath, jsonMode) {
  const doc = readDocument(filePath)
  if (jsonMode) {
    console.log(JSON.stringify(doc, null, 2))
    return
  }
  if (doc.lines.length === 0) {
    return
  }
  console.log(formatPretty(doc))
}

function editCommand(filePath, anchorOrRange, newContent) {
  const doc = readDocument(filePath)
  const nextLines = doc.lines.map((line) => line.content)

  if (anchorOrRange.includes('..')) {
    const { start, end } = resolveRange(doc, anchorOrRange)
    nextLines.splice(start.n - 1, end.n - start.n + 1, newContent)
    writeLines(filePath, nextLines, doc)
    console.log(`Edited lines ${start.n}-${end.n}.`)
    return
  }

  const target = resolveAnchor(doc, anchorOrRange)
  nextLines[target.n - 1] = newContent
  writeLines(filePath, nextLines, doc)
  console.log(`Edited line ${target.n}.`)
}

function insertCommand(filePath, anchorText, newContent) {
  const doc = readDocument(filePath)
  const target = resolveAnchor(doc, anchorText)
  const nextLines = doc.lines.map((line) => line.content)
  nextLines.splice(target.n, 0, newContent)
  writeLines(filePath, nextLines, doc)
  console.log(`Inserted line ${target.n + 1}.`)
}

function usage() {
  console.log('Usage:')
  console.log('  node poc/linehash.js read <file> [--json]')
  console.log('  node poc/linehash.js edit <file> <hash-or-range> <new content>')
  console.log('  node poc/linehash.js insert <file> <hash> <new line>')
}

function main(argv) {
  const [command, ...args] = argv
  if (!command) {
    usage()
    return
  }

  if (command === 'read') {
    const [filePath, maybeJson] = args
    if (!filePath) {
      usage()
      process.exitCode = 1
      return
    }
    readCommand(filePath, maybeJson === '--json')
    return
  }

  if (command === 'edit') {
    const [filePath, anchorOrRange, ...contentParts] = args
    if (!filePath || !anchorOrRange || contentParts.length === 0) {
      usage()
      process.exitCode = 1
      return
    }
    editCommand(filePath, anchorOrRange, contentParts.join(' '))
    return
  }

  if (command === 'insert') {
    const [filePath, anchorText, ...contentParts] = args
    if (!filePath || !anchorText || contentParts.length === 0) {
      usage()
      process.exitCode = 1
      return
    }
    insertCommand(filePath, anchorText, contentParts.join(' '))
    return
  }

  usage()
  process.exitCode = 1
}

if (require.main === module) {
  try {
    main(process.argv.slice(2))
  } catch (error) {
    console.error(`Error: ${error.message}`)
    process.exit(1)
  }
}

module.exports = {
  shortHash,
  readDocument,
  formatPretty,
  parseAnchor,
  parseRange,
  resolveAnchor,
  resolveRange,
  readCommand,
  editCommand,
  insertCommand,
}
