/**
 * The fog of war, as the browser works it out.
 *
 * The server knows which stars someone has opened and nothing about light. How
 * far an open star's light reaches is decided here, from where that star is in
 * the sky being shown, and lands in a small texture the star shader samples
 * (ADR 0003, ADR 0026). So the field stays one draw call, the tiles stay the same
 * for everybody, and a sky laid out again lights the same stars in their new
 * places with nothing stored and nothing to migrate.
 *
 * A star is in one of four states, and the rest of the interface asks this file
 * which: **open**, because the person listened to it; **lit**, near enough to an
 * open star to be seen and reached; an **echo**, one of the closest unheard stars
 * of an open one, marked wherever it is; or **dark**, a grey silhouette with no
 * name that cannot be picked.
 */

import type { Label } from './labels'
import type { Camera, Star } from './renderer'
import type { Bounds } from './Sky'
import { STRIDE, type Sky } from './tiles'

/** A star this person has opened. */
export interface OpenStar extends Star {
  name: string
  /** RFC 3339: when it was first opened, by any record. */
  openedAt: string
}

/** A star not heard yet that open stars reflect. */
export interface Echo {
  artistId: number
  name: string
  x: number
  y: number
  /** The open stars that reflect it. */
  of: number[]
}

/** What the server says about one person's sky. */
export interface PersonalSky {
  opened: OpenStar[]
  echoes: Echo[]
}

export const EMPTY_SKY: PersonalSky = { opened: [], echoes: [] }

/**
 * The texture's side, in texels. A square of the sky's longest side: 2.7 layout
 * units a texel on a sky 2,700 across, a fifth of the light's radius, and a
 * megabyte that is uploaded when a star opens rather than every frame.
 */
export const COVERAGE_SIDE = 1024

/**
 * How far an open star's light reaches, in layout units.
 *
 * Measured on the slice of 100,000 stars: the average star has 9 others within
 * 14 units and 39 within 30, three times that on the whole canon. Fourteen is
 * enough to see what lives next door without opening the street.
 */
export const LIGHT_RADIUS = 14

/** How much of the light a star must catch to be read and picked rather than a silhouette. */
export const READABLE = 0.5

/**
 * The dimmest an open star is drawn. The sky's brightness is how many people
 * listen, and most of what someone finds for themselves is dim by that measure:
 * in their own sky it is a star they lit, and it has to read as one from afar.
 */
export const OPEN_FLOOR = 0.5

/** The side of a cell of the coarse map that decides whether a region's name is known, in layout units. */
const CHART_CELL = 64

/** One person's fog over one sky, ready for the renderer and the interface. */
export interface Fog {
  /** `side * side` values, 0..255; row 0 is the sky's lowest y. */
  coverage: Uint8Array
  side: number
  originX: number
  originY: number
  /** Layout units across the texture. */
  span: number
  /** The open stars as (x, y, brightness, year) quadruples, for a pass of their own. */
  packed: Float32Array
  echoPoints: readonly { x: number; y: number }[]
  opened: ReadonlyMap<number, OpenStar>
  /** The same stars, brightest first: whose names are written when not all of them fit. */
  brightest: readonly OpenStar[]
  echoes: ReadonlyMap<number, Echo>
  /** Which echoes each open star reflects. */
  echoesOf: ReadonlyMap<number, readonly Echo[]>
  /** Coarse cells holding an open star. */
  charted: ReadonlySet<number>
}

export function buildFog(sky: Sky, personal: PersonalSky): Fog {
  const span = Math.max(sky.max_x - sky.min_x, sky.max_y - sky.min_y)
  const side = COVERAGE_SIDE
  const fog = {
    coverage: new Uint8Array(side * side),
    side,
    originX: sky.min_x,
    originY: sky.min_y,
    span,
  }
  paint(fog, personal.opened)

  const packed = new Float32Array(personal.opened.length * STRIDE)
  const charted = new Set<number>()
  personal.opened.forEach((star, index) => {
    packed[index * STRIDE] = star.x
    packed[index * STRIDE + 1] = star.y
    packed[index * STRIDE + 2] = Math.max(star.brightness, OPEN_FLOOR)
    packed[index * STRIDE + 3] = star.year
    charted.add(cellOf(fog, star.x, star.y))
  })

  const echoesOf = new Map<number, Echo[]>()
  for (const echo of personal.echoes) {
    for (const star of echo.of) {
      const list = echoesOf.get(star)
      if (list) list.push(echo)
      else echoesOf.set(star, [echo])
    }
  }

  return {
    ...fog,
    packed,
    echoPoints: personal.echoes.map(({ x, y }) => ({ x, y })),
    opened: new Map(personal.opened.map(star => [star.artistId, star])),
    brightest: [...personal.opened].sort((a, b) => b.brightness - a.brightness || a.artistId - b.artistId),
    echoes: new Map(personal.echoes.map(echo => [echo.artistId, echo])),
    echoesOf,
    charted,
  }
}

type Coverage = Pick<Fog, 'coverage' | 'side' | 'originX' | 'originY' | 'span'>

/**
 * Lays each open star's light into the texture.
 *
 * The strongest light wins rather than light adding up: two stars side by side
 * are not twice as lit, and a sum would show every dense neighbourhood as a
 * hard white blob. One kernel, worked out once, stamped per star: a long history
 * is tens of thousands of stars and this runs each time one opens.
 */
function paint(fog: Coverage, stars: readonly { x: number; y: number }[]): void {
  const { coverage, side, originX, originY, span } = fog
  const perUnit = side / span
  const radius = LIGHT_RADIUS * perUnit
  const reach = Math.ceil(radius)
  const width = reach * 2 + 1
  const kernel = new Uint8Array(width * width)
  for (let dy = -reach; dy <= reach; dy++) {
    for (let dx = -reach; dx <= reach; dx++) {
      kernel[(dy + reach) * width + dx + reach] = Math.round(255 * falloff(Math.hypot(dx, dy) / radius))
    }
  }

  for (const star of stars) {
    const col = Math.round((star.x - originX) * perUnit)
    const row = Math.round((star.y - originY) * perUnit)
    for (let dy = -reach; dy <= reach; dy++) {
      const y = row + dy
      if (y < 0 || y >= side) continue
      for (let dx = -reach; dx <= reach; dx++) {
        const x = col + dx
        // Checked per axis rather than by the flat index, or the light of a
        // star on the right edge would wrap round onto the left.
        if (x < 0 || x >= side) continue
        const at = y * side + x
        const value = kernel[(dy + reach) * width + dx + reach] ?? 0
        if (value > (coverage[at] ?? 0)) coverage[at] = value
      }
    }
  }
}

/** Full to about half the radius, then a soft edge: the fog of the reference is soft, "not pixels being cut". */
function falloff(distance: number): number {
  const t = Math.min(Math.max((distance - 0.45) / 0.55, 0), 1)
  return 1 - t * t * (3 - 2 * t)
}

/** How much light reaches a place, 0..1. */
export function litAt(fog: Coverage, x: number, y: number): number {
  const perUnit = fog.side / fog.span
  const col = Math.round((x - fog.originX) * perUnit)
  const row = Math.round((y - fog.originY) * perUnit)
  if (col < 0 || col >= fog.side || row < 0 || row >= fog.side) return 0
  return (fog.coverage[row * fog.side + col] ?? 0) / 255
}

function cellOf(fog: Pick<Fog, 'originX' | 'originY'>, x: number, y: number): number {
  return cellKey(Math.floor((x - fog.originX) / CHART_CELL), Math.floor((y - fog.originY) / CHART_CELL))
}

function cellKey(col: number, row: number): number {
  return col * 100_000 + row
}

/** Whether someone has opened anything around a place: what makes its name known. */
export function chartedAt(fog: Pick<Fog, 'originX' | 'originY' | 'charted'>, x: number, y: number): boolean {
  const col = Math.floor((x - fog.originX) / CHART_CELL)
  const row = Math.floor((y - fog.originY) / CHART_CELL)
  for (let dy = -1; dy <= 1; dy++) {
    for (let dx = -1; dx <= 1; dx++) {
      if (fog.charted.has(cellKey(col + dx, row + dy))) return true
    }
  }
  return false
}

/**
 * The names on the sky, as this person may read them.
 *
 * A region nobody has been to keeps its place and loses its name: a genre reads
 * "uncharted" where it would be lettered, so the dark still has a geography. A
 * style is a detail inside a place someone has not been, and is not written.
 */
export function chartLabels(labels: readonly Label[], fog: Pick<Fog, 'originX' | 'originY' | 'charted'>, uncharted: string): Label[] {
  const shown: Label[] = []
  for (const label of labels) {
    if (chartedAt(fog, label.x, label.y)) shown.push(label)
    else if (label.kind === 'genre') shown.push({ ...label, name: uncharted })
  }
  return shown
}

/** A name of an open star, placed on the screen, in device pixels from the top-left. */
export interface PlacedName {
  name: string
  x: number
  y: number
}

/** How many names of open stars are written at once, and how many are tried to get there. */
const MOST_NAMES = 24
const MOST_TRIED = 300

/**
 * The names of the open stars that fit on the screen, brightest first, none
 * overlapping.
 *
 * Only the stars someone opened have a name on the sky, which is most of what
 * makes it theirs. Runs per frame, so the search stops early: the first two dozen
 * that fit are as good as any, and a long history must not be measured star by
 * star. `measure` is a parameter so this can be tested without a canvas.
 */
export function placeOpenNames(
  fog: Pick<Fog, 'brightest'>,
  camera: Camera,
  viewport: { width: number; height: number },
  measure: (name: string) => number,
  lineHeight: number
): PlacedName[] {
  const placed: PlacedName[] = []
  const boxes: { left: number; top: number; right: number; bottom: number }[] = []
  // Beside the star rather than on it, the way a chart letters one.
  const gap = lineHeight * 0.7
  let tried = 0
  for (const star of fog.brightest) {
    const x = viewport.width / 2 + (star.x - camera.x) * camera.scale
    const y = viewport.height / 2 - (star.y - camera.y) * camera.scale
    if (x < 0 || y < 0 || x > viewport.width || y > viewport.height) continue
    tried += 1
    if (tried > MOST_TRIED) break
    const box = { left: x, top: y - lineHeight / 2, right: x + gap + measure(star.name), bottom: y + lineHeight / 2 }
    if (box.right > viewport.width || box.top < 0 || box.bottom > viewport.height) continue
    if (boxes.some(other => box.left < other.right && box.right > other.left && box.top < other.bottom && box.bottom > other.top)) continue
    boxes.push(box)
    placed.push({ name: star.name, x: x + gap, y })
    if (placed.length >= MOST_NAMES) break
  }
  return placed
}

/** What the star under a click is, in the fog. */
export function pickInFog(
  fog: Pick<Fog, 'coverage' | 'side' | 'originX' | 'originY' | 'span' | 'opened' | 'echoes'>,
  field: readonly Star[],
  at: { x: number; y: number },
  radius: number
): Star | null {
  let best: Star | null = null
  let bestDistance = radius * radius
  const consider = (star: Star) => {
    const dx = star.x - at.x
    const dy = star.y - at.y
    const distance = dx * dx + dy * dy
    if (distance < bestDistance) {
      bestDistance = distance
      best = star
    }
  }

  // Open stars and echoes are drawn whatever the zoom, so they can be picked
  // whether or not this level of the pyramid holds them.
  for (const star of fog.opened.values()) consider(star)
  for (const echo of fog.echoes.values()) consider({ artistId: echo.artistId, x: echo.x, y: echo.y, brightness: 0, year: 0 })

  // A silhouette in the dark is not there to be read: that it is the only way
  // to learn a dark star's name is the fog.
  for (const star of field) {
    if (litAt(fog, star.x, star.y) >= READABLE) consider(star)
  }
  return best
}

/** A line of the list of stars in view. `name` is null for an echo: a silhouette has none until it is reached. */
export interface FogListed {
  id: number
  x: number
  y: number
  name: string | null
  /** For an echo, the name of a star that reflects it. */
  echoOf: string | null
}

/** How many open stars and how many echoes the list carries. */
const LIST_OPEN = 12
const LIST_ECHOES = 4

/**
 * What can be listed of the view: the open stars, brightest first, and then the
 * echoes, which are the next step from them and the keyboard's way to reach one.
 */
export function listInFog(fog: Pick<Fog, 'opened' | 'echoes'>, view: Bounds): FogListed[] {
  const inside = (star: { x: number; y: number }) => star.x >= view.minX && star.x <= view.maxX && star.y >= view.minY && star.y <= view.maxY
  const open = [...fog.opened.values()]
    .filter(inside)
    .sort((a, b) => b.brightness - a.brightness || a.artistId - b.artistId)
    .slice(0, LIST_OPEN)
    .map<FogListed>(star => ({ id: star.artistId, x: star.x, y: star.y, name: star.name, echoOf: null }))
  const echoes = [...fog.echoes.values()]
    .filter(inside)
    .sort((a, b) => a.artistId - b.artistId)
    .slice(0, LIST_ECHOES)
    .map<FogListed>(echo => ({
      id: echo.artistId,
      x: echo.x,
      y: echo.y,
      name: null,
      echoOf: echo.of.map(id => fog.opened.get(id)?.name).find(name => name !== undefined) ?? null,
    }))
  return [...open, ...echoes]
}
