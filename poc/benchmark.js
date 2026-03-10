#!/usr/bin/env node
'use strict'

const fs = require('fs')
const os = require('os')
const path = require('path')
const { editCommand, readDocument } = require('./linehash')

function strReplace(filePath, oldStr, newStr) {
  const content = fs.readFileSync(filePath, 'utf8')
  if (!content.includes(oldStr)) {
    throw new Error('String to replace not found')
  }
  fs.writeFileSync(filePath, content.replace(oldStr, newStr), 'utf8')
}

function main() {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'linehash-poc-'))
  const filePath = path.join(workspace, 'demo.js')

  const original = [
    'function hello() {',
    '  return "world"',
    '}',
    '',
  ].join('\n')

  const extraSpace = [
    'function hello() {',
    '   return "world"',
    '}',
    '',
  ].join('\n')

  try {
    fs.writeFileSync(filePath, original, 'utf8')

    try {
      strReplace(filePath, extraSpace, 'function hello() {\n  return "universe"\n}\n')
      console.log('str_replace: ✓ success')
    } catch (error) {
      console.log(`str_replace: ✗ FAILED — ${error.message}`)
    }

    fs.writeFileSync(filePath, original, 'utf8')
    const doc = readDocument(filePath)
    const anchor = `${doc.lines[1].n}:${doc.lines[1].hash}`
    editCommand(filePath, anchor, '  return "universe"')

    const updated = fs.readFileSync(filePath, 'utf8')
    if (!updated.includes('  return "universe"')) {
      throw new Error('linehash edit did not update the file as expected')
    }

    console.log('linehash:    ✓ success (anchor-based edit works)')
    console.log(`anchor used: ${anchor}`)
  } finally {
    fs.rmSync(workspace, { recursive: true, force: true })
  }
}

if (require.main === module) {
  try {
    main()
  } catch (error) {
    console.error(`Error: ${error.message}`)
    process.exit(1)
  }
}

module.exports = {
  strReplace,
}
