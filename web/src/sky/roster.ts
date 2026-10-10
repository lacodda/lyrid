/**
 * The arithmetic of a dossier's roster: the span of years its chronology is
 * drawn over, where the axis is marked, where the roster sits on the sky, and
 * which of its stars a click on the small map meant.
 *
 * Kept apart from the component for the reason the minimap's is: these are
 * properties to test, not to eyeball — a span bar that lands a decade off its
 * histogram reads as a different history.
 */

import type { YearCount } from '@/api'
import { toMap } from './minimap'
import type { Bounds, View } from './Sky'
import type { Sky } from './tiles'

/** A first and a last year, either of which may be unknown. */
export interface Span {
  first: number | null
  last: number | null
}

/**
 * The years a chronology and its roster's spans are drawn across, or `null`
 * when nothing is dated. One domain for both, so a roster member's bar sits
 * under the years of the histogram it belongs to.
 */
export function yearDomain(counts: readonly YearCount[], spans: readonly Span[]): [number, number] | null {
  let low = Infinity
  let high = -Infinity
  const take = (year: number | null) => {
    if (year === null) return
    low = Math.min(low, year)
    high = Math.max(high, year)
  }
  for (const entry of counts) take(entry.year)
  for (const span of spans) {
    take(span.first)
    take(span.last)
  }
  return Number.isFinite(low) ? [low, high] : null
}

/**
 * The years the axis is marked at: round decades, spaced so a century fits
 * without the labels colliding.
 */
export function yearTicks([low, high]: [number, number]): number[] {
  const span = high - low
  const step = span <= 40 ? 10 : span <= 100 ? 20 : 50
  const ticks: number[] = []
  for (let year = Math.ceil(low / step) * step; year <= high; year += step) ticks.push(year)
  return ticks
}

/** Where a year falls across `width`, the domain's ends at the two edges. */
export function yearX([low, high]: [number, number], year: number, width: number): number {
  // One year wide at least: a single-year domain would divide by zero.
  const span = Math.max(1, high - low + 1)
  return ((year - low) / span) * width
}

/** The busiest year, the earliest of a tie, or `null` for no years at all. */
export function peak(counts: readonly YearCount[]): YearCount | null {
  let best: YearCount | null = null
  for (const entry of counts) {
    if (!best || entry.count > best.count) best = entry
  }
  return best
}

/** A roster member on the map: `[id, x, y, brightness]`. */
export type Member = [number, number, number, number]

/**
 * Anything placed on the sky as `[id, x, y, ...]`: a roster member, or a star
 * someone has heard. Framing a set of them asks for nothing more.
 */
export type Placed = readonly [number, number, number, ...number[]]

/**
 * The view that shows a roster on the big sky: centred on its middle and wide
 * enough for most of it.
 *
 * The outer tenth on each side is left out of the frame. A roster almost
 * always has a few members far from the rest — a soul label's one techno act —
 * and framing them would zoom the sky out until the cluster is a dot again.
 * The scale is worked out the way the compass works out the whole sky's:
 * `visible` and `view` together say how many world units the canvas is wide.
 */
export function fitView(members: readonly Placed[], visible: Bounds, view: View): View | null {
  if (members.length === 0) return null
  const xs = members.map(member => member[1]).sort((a, b) => a - b)
  const ys = members.map(member => member[2]).sort((a, b) => a - b)
  const [left, right] = [quantile(xs, 0.1), quantile(xs, 0.9)]
  const [bottom, top] = [quantile(ys, 0.1), quantile(ys, 0.9)]

  const canvasWidth = (visible.maxX - visible.minX) * view.scale
  const canvasHeight = (visible.maxY - visible.minY) * view.scale
  // A roster of one, or of stars in one spot, still needs a frame around it.
  const width = Math.max(right - left, MIN_FRAME) * MARGIN
  const height = Math.max(top - bottom, MIN_FRAME) * MARGIN
  return {
    x: (left + right) / 2,
    y: (bottom + top) / 2,
    scale: Math.min(canvasWidth / width, canvasHeight / height),
  }
}

/** Room around the framed roster, so its edge is not the screen's. */
const MARGIN = 1.3

/** The narrowest frame, in world units: a few stars' worth of sky. */
const MIN_FRAME = 20

function quantile(sorted: readonly number[], q: number): number {
  const at = Math.min(sorted.length - 1, Math.max(0, Math.round(q * (sorted.length - 1))))
  return sorted[at] ?? 0
}

/**
 * The member under a point of a square map `size` pixels across, the nearest
 * within `reach` pixels, or `null` when the point is empty sky.
 */
export function memberAt(members: readonly Member[], sky: Sky, size: number, point: { x: number; y: number }, reach: number): number | null {
  let best: number | null = null
  let bestDistance = Infinity
  for (const [id, x, y] of members) {
    const at = toMap(sky, size, x, y)
    const distance = Math.hypot(at.x - point.x, at.y - point.y)
    if (distance <= reach && distance < bestDistance) {
      best = id
      bestDistance = distance
    }
  }
  return best
}
