/**
 * Reading the tile pyramid.
 *
 * The format is specified in `reference/tile-format`: a 16-byte header, then
 * 16-byte records of (artist id, x, y, brightness), little-endian. Nothing
 * here parses — a tile becomes a typed-array view and goes to the GPU.
 */

import type { Star } from './renderer'

const HEADER = 16
const RECORD = 16
const MAGIC = 'LYST'

/** The world the tiles cover, from `sky.json`. */
export interface Sky {
  min_x: number
  min_y: number
  max_x: number
  max_y: number
  max_level: number
  record_bytes: number
}

/** One tile's stars, packed for the GPU and readable for the UI. */
export interface Tile {
  /** (x, y, brightness) triples, ready for `SkyRenderer.upload`. */
  packed: Float32Array
  /** The same stars with their ids, for hit testing and search. */
  stars: Star[]
}

export async function fetchSky(root = '/tiles', signal?: AbortSignal): Promise<Sky> {
  const response = await fetch(`${root}/sky.json`, { signal })
  if (!response.ok) throw new Error('the sky has not been built yet: run `lyrid layout --tiles`')
  const body: unknown = await response.json()
  if (!isSky(body)) throw new Error('unexpected sky.json payload')
  return body
}

/**
 * Reads one tile, or `null` when there is none.
 *
 * **A tile is identified by its magic, never by the status code.** A dev
 * server answers a missing file with 200 and an HTML fallback page, and a
 * CDN or a single-page host may do the same; trusting the status turns that
 * page into a tile and throws on the first zoom.
 */
export async function fetchTile(level: number, col: number, row: number, root = '/tiles', signal?: AbortSignal): Promise<Tile | null> {
  const response = await fetch(`${root}/${level}/${col}/${row}.bin`, { signal })
  if (!response.ok) return null
  const buffer = await response.arrayBuffer()
  if (buffer.byteLength < HEADER) return null

  const view = new DataView(buffer)
  if (String.fromCharCode(...new Uint8Array(buffer, 0, 4)) !== MAGIC) return null

  const count = view.getUint32(8, true)
  if (buffer.byteLength < HEADER + count * RECORD) return null

  const packed = new Float32Array(count * 3)
  const stars: Star[] = new Array<Star>(count)
  for (let i = 0; i < count; i++) {
    const at = HEADER + i * RECORD
    const x = view.getFloat32(at + 4, true)
    const y = view.getFloat32(at + 8, true)
    const brightness = view.getFloat32(at + 12, true)
    packed[i * 3] = x
    packed[i * 3 + 1] = y
    packed[i * 3 + 2] = brightness
    stars[i] = { artistId: view.getInt32(at, true), x, y, brightness }
  }
  return { packed, stars }
}

/** Every tile of one level, concatenated. */
export async function fetchLevel(level: number, root = '/tiles', signal?: AbortSignal): Promise<Tile> {
  const side = 2 ** level
  const requests: Promise<Tile | null>[] = []
  for (let col = 0; col < side; col++) {
    for (let row = 0; row < side; row++) {
      requests.push(fetchTile(level, col, row, root, signal))
    }
  }
  const tiles = (await Promise.all(requests)).filter((tile): tile is Tile => tile !== null)

  const total = tiles.reduce((sum, tile) => sum + tile.packed.length, 0)
  const packed = new Float32Array(total)
  const stars: Star[] = []
  let at = 0
  for (const tile of tiles) {
    packed.set(tile.packed, at)
    at += tile.packed.length
    stars.push(...tile.stars)
  }
  return { packed, stars }
}

/**
 * Which pyramid level suits a zoom.
 *
 * Levels filter by brightness rather than resolution, so this is "how much of
 * the sky is on screen" turned into "how many stars to admit".
 */
/**
 * What the render loop should do about the level it wants to show.
 *
 * Pulled out of the loop because the rule has a trap in it, and a trap in a
 * render loop is invisible: the placeholder that marks a level as *being
 * fetched* must never reach the renderer. Uploading it would draw nothing and
 * record the level as shown, so the real tiles arriving a moment later would
 * find the level already "current" and never replace them — an empty sky, for
 * good.
 */
export type TileAction = { do: 'fetch' } | { do: 'upload' } | { do: 'wait' }

export function tileAction(tile: Tile | undefined, wanted: number, shown: number): TileAction {
  if (!tile) return { do: 'fetch' }
  // Still a placeholder: keep drawing whatever is already uploaded.
  if (tile.packed.length === 0) return { do: 'wait' }
  return wanted === shown ? { do: 'wait' } : { do: 'upload' }
}

export function levelFor(sky: Sky, visibleSpan: number): number {
  const span = sky.max_x - sky.min_x
  const fraction = Math.max(visibleSpan / span, 1e-6)
  const level = Math.round(Math.log2(1 / fraction))
  return Math.max(0, Math.min(level, sky.max_level))
}

function isSky(value: unknown): value is Sky {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return (
    typeof candidate.min_x === 'number' &&
    typeof candidate.min_y === 'number' &&
    typeof candidate.max_x === 'number' &&
    typeof candidate.max_y === 'number' &&
    typeof candidate.max_level === 'number'
  )
}
