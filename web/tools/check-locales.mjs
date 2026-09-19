// Holds every locale to the same shape as the source language.
//
// English is the source, never a fallback: a missing Russian key must fail the
// build rather than quietly render in English, because a half-translated screen
// is the kind of thing nobody reports and everybody notices.
//
//   node tools/check-locales.mjs
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'

// fileURLToPath, not URL.pathname: on Windows the latter yields `/C:/...`,
// which joins into `C:\C:\...`.
const LOCALES = fileURLToPath(new URL('../src/i18n/locales/', import.meta.url))
const SOURCE = 'en'

/** Every leaf as a dotted path, so two files can be compared key by key. */
function flatten(value, prefix = '') {
  const out = new Map()
  for (const [key, child] of Object.entries(value)) {
    const path = prefix === '' ? key : `${prefix}.${key}`
    if (child !== null && typeof child === 'object' && !Array.isArray(child)) {
      for (const [nested, leaf] of flatten(child, path)) out.set(nested, leaf)
    } else {
      out.set(path, child)
    }
  }
  return out
}

/** `{{name}}` and `{{count}}` — the parts a translation may not invent or drop. */
function placeholders(text) {
  if (typeof text !== 'string') return new Set()
  return new Set([...text.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)].map((match) => match[1]))
}

// i18next appends a plural category to the key; those are alternates of one
// message, not keys the other locale has to mirror one for one. English has two
// forms, Russian has four.
const PLURAL_SUFFIX = /_(zero|one|two|few|many|other)$/

const stem = (key) => key.replace(PLURAL_SUFFIX, '')

function read(locale) {
  const file = join(LOCALES, `${locale}.json`)
  try {
    return flatten(JSON.parse(readFileSync(file, 'utf8')))
  } catch (cause) {
    console.error(`✗ ${locale}.json could not be read: ${cause.message}`)
    process.exit(1)
  }
}

const locales = readdirSync(LOCALES)
  .filter((file) => file.endsWith('.json'))
  .map((file) => basename(file, '.json'))

if (!locales.includes(SOURCE)) {
  console.error(`✗ the source locale ${SOURCE}.json is missing`)
  process.exit(1)
}

const source = read(SOURCE)
const sourceStems = new Set([...source.keys()].map(stem))
const problems = []

for (const locale of locales.filter((name) => name !== SOURCE)) {
  const target = read(locale)
  const targetStems = new Set([...target.keys()].map(stem))

  for (const key of sourceStems) {
    if (!targetStems.has(key)) problems.push(`${locale}: missing  ${key}`)
  }

  // An extra key is a rename that only landed on one side — dead weight at best,
  // and a sign the other locale lost a message at worst.
  for (const key of targetStems) {
    if (!sourceStems.has(key)) problems.push(`${locale}: unknown  ${key}`)
  }

  for (const [key, text] of target) {
    if (typeof text !== 'string') {
      problems.push(`${locale}: not a string  ${key}`)
      continue
    }
    if (text.trim() === '') problems.push(`${locale}: empty  ${key}`)

    // Compare against the source message this one translates, plural or not.
    const original = source.get(key) ?? source.get(`${stem(key)}_other`) ?? source.get(stem(key))
    if (original === undefined) continue

    const expected = placeholders(original)
    const actual = placeholders(text)

    for (const name of expected) {
      if (!actual.has(name)) problems.push(`${locale}: drops {{${name}}}  ${key}`)
    }
    for (const name of actual) {
      if (!expected.has(name)) problems.push(`${locale}: invents {{${name}}}  ${key}`)
    }
  }
}

// Every key the code asks for has to exist in the source locale.
//
// The comparison above holds the locales to each other, which cannot see a key
// that is missing from all of them — that reads as consistent. It reaches a
// person as a raw `charter.lede` on screen, and only if someone happens
// to look at that screen. So the code is read too.
const SOURCE_DIR = fileURLToPath(new URL('../src/', import.meta.url))

/** Every `.ts`/`.tsx` file under src/. */
function sources(dir) {
  const found = []
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry)
    if (statSync(path).isDirectory()) found.push(...sources(path))
    else if (/\.tsx?$/.test(entry)) found.push(path)
  }
  return found
}

const used = new Map()
for (const file of sources(SOURCE_DIR)) {
  const text = readFileSync(file, 'utf8')
  // Only literal keys: `t(\`card.tab.${tab}\`)` is built at runtime from a
  // union the type checker already constrains, and guessing its arms here
  // would be a second, worse type checker.
  for (const match of text.matchAll(/\bt\(\s*'([A-Za-z][\w.]*)'/g)) {
    if (!used.has(match[1])) used.set(match[1], basename(file))
  }
}

for (const [key, file] of used) {
  // A plural message is stored under its forms, never under the bare key.
  const known =
    source.has(key) || source.has(`${key}_other`) || source.has(`${key}_one`)
  if (!known) problems.push(`${SOURCE}: used in ${file} but missing  ${key}`)
}

if (problems.length > 0) {
  console.error(`✗ ${String(problems.length)} locale problem(s):\n`)
  for (const problem of problems.sort()) console.error(`  ${problem}`)
  console.error('\nEnglish is the source. Add the missing message rather than')
  console.error('letting it fall back — a screen half in English is a bug.')
  process.exit(1)
}

const others = locales.filter((name) => name !== SOURCE)
console.log(
  others.length === 0
    ? `✓ ${String(source.size)} keys in ${SOURCE} (no other locale yet)`
    : `✓ ${String(source.size)} keys, ${others.join(', ')} complete against ${SOURCE}`,
)
