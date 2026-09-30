// Downloads one month of the Discogs dumps the canon is built from, and checks
// every file against the dump's own CHECKSUM.txt.
//
//   node tools/fetch-discogs.mjs 20260801 [directory]
//
// Why a script and not four curl lines. data.discogs.com sits behind a CDN
// that ignores Range -- a request for the rest of a file gets the whole file
// again -- so a cut transfer cannot be resumed; and HTTP/1.1 transfers of the
// 11 GB releases file were cut after six to eight minutes, three times out of
// three. Over HTTP/2 with a large flow-control window the same file arrives
// whole in about nine minutes (measured 30.09.2026, ~20 MB/s). The default
// window of 64 KB caps one stream at a few MB/s, which is why it is raised.
//
// Files go one after another, never in parallel, and a failed file is not
// retried in a loop: the CDN answers a burst of requests with 429 for about an
// hour, and each retry extends it. Run the script again later instead; files
// that already match their checksum are skipped.
import { createHash } from 'node:crypto'
import { createReadStream, createWriteStream, existsSync, mkdirSync, renameSync, statSync } from 'node:fs'
import { connect } from 'node:http2'
import { join } from 'node:path'

const [version, directory = '.local/discogs'] = process.argv.slice(2)
if (!/^\d{8}$/.test(version ?? '')) {
  console.error('usage: node tools/fetch-discogs.mjs YYYYMMDD [directory]')
  process.exit(2)
}

const FILES = ['labels.xml.gz', 'artists.xml.gz', 'masters.xml.gz', 'releases.xml.gz']
const WINDOW = 32 * 1024 * 1024
const year = version.slice(0, 4)

mkdirSync(directory, { recursive: true })
const session = connect('https://data.discogs.com', { settings: { initialWindowSize: WINDOW } })
session.on('connect', () => session.setLocalWindowSize(WINDOW))
session.on('error', error => console.error(`connection failed: ${error.message}`))

try {
  const checksums = await fetchChecksums()
  for (const file of FILES) {
    const name = `discogs_${version}_${file}`
    const want = checksums.get(name)
    if (!want) throw new Error(`CHECKSUM.txt does not list ${name}`)
    const path = join(directory, name)
    if (existsSync(path) && (await sha256(path)) === want) {
      console.log(`${name}: already here and whole`)
      continue
    }
    await download(name, path, want)
  }
  console.log('every file is here and matches its checksum')
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error))
  process.exitCode = 1
} finally {
  session.close()
}

async function fetchChecksums() {
  const name = `discogs_${version}_CHECKSUM.txt`
  const chunks = []
  const { status } = await get(name, chunk => chunks.push(chunk))
  if (status !== 200) throw new Error(`${name}: HTTP ${String(status)}`)
  const text = Buffer.concat(chunks).toString('utf8')
  // An index page instead of the file is the classic mistake with this host:
  // it answers 200 with HTML for a path it does not serve.
  if (!/^[0-9a-f]{64} /m.test(text)) throw new Error(`${name} is not a checksum list; was the version right?`)
  return new Map(text.split('\n').filter(Boolean).map(line => {
    const [hash, file] = line.trim().split(/\s+\*?/)
    return [file, hash]
  }))
}

async function download(name, path, want) {
  const part = `${path}.part`
  const file = createWriteStream(part)
  const hash = createHash('sha256')
  const started = Date.now()
  let received = 0
  let reported = started
  const { status, length } = await get(name, chunk => {
    file.write(chunk)
    hash.update(chunk)
    received += chunk.length
    if (Date.now() - reported > 30_000) {
      reported = Date.now()
      const seconds = (reported - started) / 1000
      console.log(`${name}: ${(received / 1e9).toFixed(2)} of ${(length / 1e9).toFixed(2)} GB, ${(received / seconds / 1e6).toFixed(1)} MB/s`)
    }
  })
  await new Promise(resolve => file.end(resolve))
  if (status !== 200) throw new Error(`${name}: HTTP ${String(status)}${status === 429 ? ' - the CDN is rate-limiting; wait an hour' : ''}`)
  const got = hash.digest('hex')
  if (received !== length || got !== want) {
    throw new Error(`${name}: ${String(received)} of ${String(length)} bytes, checksum ${got === want ? 'matches' : 'does not match'} - the transfer was cut; run again later`)
  }
  renameSync(part, path)
  console.log(`${name}: ${(statSync(path).size / 1e9).toFixed(2)} GB in ${((Date.now() - started) / 1000).toFixed(0)} s, checksum matches`)
}

/** One GET through the shared session; resolves when the body has ended. */
function get(name, onChunk) {
  return new Promise((resolve, reject) => {
    const request = session.request({
      ':method': 'GET',
      ':path': `/?download=${encodeURIComponent(`data/${year}/${name}`)}`,
      'user-agent': 'lyrid-canon-import (+https://github.com/lacodda/lyrid)',
    })
    let status = 0
    let length = 0
    request.on('response', headers => {
      status = Number(headers[':status'])
      length = Number(headers['content-length'] ?? 0)
    })
    request.on('data', onChunk)
    request.on('end', () => resolve({ status, length }))
    request.on('error', reject)
  })
}

async function sha256(path) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest('hex')
}
