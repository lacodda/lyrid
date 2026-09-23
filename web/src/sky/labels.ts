/**
 * Names on the sky, and which of them to write at a given distance.
 *
 * `labels.json` sits beside the tiles and is cut with them (see
 * `reference/tile-format`): each genre and style anchored at its densest
 * place. This file decides what the client does with it — which layer suits
 * the zoom, and which names fit on the screen without landing on each other.
 * What to call the place the camera is looking at is a different question,
 * answered by the stars in view rather than by these anchors — see `Compass`.
 */

import type { Camera } from './renderer'
import { stamped, type Sky } from './tiles'

export type LabelKind = 'genre' | 'style'

export interface Label {
  name: string
  kind: LabelKind
  x: number
  y: number
  /** How many placed artists carry this as their main genre or style. */
  members: number
}

export async function fetchLabels(sky: Sky, root = '/tiles', signal?: AbortSignal): Promise<Label[]> {
  // Stamped like the tiles: the names belong to one cut, and yesterday's
  // names over today's stars would label the wrong places.
  const response = await fetch(stamped(sky, root, 'labels.json'), { signal })
  // A sky cut before names existed has no file, and a dev server answers a
  // missing one with its HTML fallback: neither is an error worth showing,
  // both are a sky without names.
  if (!response.ok) return []
  let body: unknown
  try {
    body = await response.json()
  } catch {
    return []
  }
  if (typeof body !== 'object' || body === null) return []
  const labels = (body as { labels?: unknown }).labels
  return Array.isArray(labels) ? labels.filter(isLabel) : []
}

/**
 * Which layer of names suits a view, by how much of the sky is on screen.
 *
 * Genres are continents and read from far away; styles are constellations and
 * only mean something once the view has closed in. Closer still, the names of
 * the stars themselves matter more than the name of the patch they sit in, so
 * nothing is written at all.
 */
export function layerFor(sky: Sky, visibleSpan: number): LabelKind | null {
  const fraction = visibleSpan / (sky.max_x - sky.min_x)
  if (fraction > WIDE) return 'genre'
  if (fraction > CLOSE) return 'style'
  return null
}

/**
 * Above this share of the sky on screen, genres are written.
 *
 * Set by where the pyramid draws everything. A style's constellation is
 * mostly faint stars, and the wide levels hold only the bright ones: measured
 * on the slice, styles written at a third of the sky landed on empty space,
 * because the stars under them were not drawn yet. Below this share the
 * deepest level is on screen and every name has its stars beneath it.
 */
export const WIDE = 0.15
/** Below this share, no names at all. */
export const CLOSE = 0.015

/** A name placed on the screen, in device pixels from the top-left. */
export interface Placed {
  label: Label
  x: number
  y: number
}

/**
 * The names that fit on the screen, largest first, none overlapping.
 *
 * Greedy by size: the largest genre in view is written first and a smaller one
 * that would touch it is dropped. That keeps the names people know — a sky
 * with "Rock" missing because "Garage Rock" got there first would be wrong in
 * a way anyone notices.
 *
 * `measure` returns a name's width in device pixels; it is a parameter so this
 * can be tested without a canvas.
 */
export function placeLabels(
  labels: readonly Label[],
  kind: LabelKind,
  camera: Camera,
  viewport: { width: number; height: number },
  measure: (name: string) => number,
  lineHeight: number
): Placed[] {
  const boxes: { left: number; top: number; right: number; bottom: number }[] = []
  const placed: Placed[] = []
  const candidates = labels.filter(label => label.kind === kind).sort((a, b) => b.members - a.members)
  const pad = lineHeight * 0.4

  for (const label of candidates) {
    const x = viewport.width / 2 + (label.x - camera.x) * camera.scale
    const y = viewport.height / 2 - (label.y - camera.y) * camera.scale
    const half = measure(label.name) / 2
    const box = { left: x - half - pad, top: y - lineHeight / 2 - pad, right: x + half + pad, bottom: y + lineHeight / 2 + pad }
    // Off screen, or only partly on it: a name cut by the edge reads as a
    // different, shorter word.
    if (box.left < 0 || box.top < 0 || box.right > viewport.width || box.bottom > viewport.height) continue
    if (boxes.some(other => box.left < other.right && box.right > other.left && box.top < other.bottom && box.bottom > other.top)) continue
    boxes.push(box)
    placed.push({ label, x, y })
  }
  return placed
}

function isLabel(value: unknown): value is Label {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return (
    typeof candidate.name === 'string' &&
    (candidate.kind === 'genre' || candidate.kind === 'style') &&
    typeof candidate.x === 'number' &&
    typeof candidate.y === 'number' &&
    typeof candidate.members === 'number'
  )
}
