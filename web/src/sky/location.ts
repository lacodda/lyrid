/**
 * The address bar as the map's state.
 *
 * Two things are worth pointing at: a star, and a view. They are kept in
 * different halves of the URL because they answer different questions and
 * behave differently in a browser.
 *
 *   /star/54#-59.2,-69.5,12
 *   ^ path: which star is open   ^ fragment: where the camera is
 *
 * The **path** is what a link is about — "look at this artist" — so it is a
 * real route the server answers with the SPA, and it is what a crawler or a
 * chat preview reads.
 *
 * The **fragment** is the camera, and it is deliberately not a route: it
 * changes on every pan and zoom, and putting that in the path would fill the
 * session history with hundreds of entries nobody wants to press Back through.
 * A fragment is also never sent to the server, which is the honest description
 * of what it is — a client-side bookmark.
 */

import type { View } from './Sky'

/** What a URL says about what to show. */
export interface Location {
  /** The artist whose card is open, if any. */
  artistId: number | null
  /** Where the camera was pointed, if the URL says. */
  view: View | null
}

/** Reads the current address. */
export function readLocation(): Location {
  return {
    artistId: readArtistId(window.location.pathname),
    view: readView(window.location.hash),
  }
}

/** `/star/54` -> 54; anything else -> null. */
export function readArtistId(pathname: string): number | null {
  const match = /^\/star\/(\d+)\/?$/.exec(pathname)
  if (!match) return null
  const id = Number(match[1])
  return Number.isSafeInteger(id) && id > 0 ? id : null
}

/**
 * `#-59.2,-69.5,12` -> a view.
 *
 * Rejects anything that is not three finite numbers with a positive scale: a
 * hand-edited or truncated fragment should open the whole sky rather than a
 * camera at NaN, which draws nothing at all and looks like a broken product.
 */
export function readView(hash: string): View | null {
  const parts = hash.replace(/^#/, '').split(',')
  if (parts.length !== 3) return null
  // An empty part must not pass: Number('') is 0, not NaN, so "#1,,3" would
  // otherwise read as a camera at y=0 rather than as a broken fragment.
  if (parts.some(part => part.trim() === '')) return null
  const numbers = parts.map(Number)
  if (!numbers.every(Number.isFinite)) return null
  const [x, y, scale] = numbers as [number, number, number]
  if (scale <= 0) return null
  return { x, y, scale }
}

/**
 * The address for a state, as a path plus fragment.
 *
 * Coordinates are rounded to what the eye can tell apart — a hundredth of a
 * world unit and three significant figures of zoom. A raw float would make the
 * URL twice as long for digits nobody can see, and a shared link is read by
 * people.
 */
export function writeLocation({ artistId, view }: Location): string {
  const path = artistId === null ? '/' : `/star/${String(artistId)}`
  if (!view) return path
  const hash = `${round(view.x)},${round(view.y)},${Number(view.scale.toPrecision(3))}`
  return `${path}#${hash}`
}

function round(value: number): number {
  return Math.round(value * 100) / 100
}
