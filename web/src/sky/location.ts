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
 *
 * A **route** is a third thing worth pointing at: several stars in an order,
 * `/route/54,962,120`. It lives in the path because, like a star, it is what
 * the link is about — and it is the seed of the playlists-as-routes the plan
 * holds for later, so its address is part of the contract from the start.
 *
 * Three paths are not the map at all — `/charter`, `/confirm` and `/reset`.
 * They are pages, and `readPage` tells them apart from a star. Two of them
 * carry a token in the query string rather than in the fragment, because a
 * fragment never reaches the server and these are links printed in a letter:
 * a mail client that rewrites them must leave something the page can read.
 */

import type { View } from './Sky'

/** What a URL says about what to show. */
export interface Location {
  /** The artist whose card is open, if any. */
  artistId: number | null
  /** Where the camera was pointed, if the URL says. */
  view: View | null
  /** A route's stops in order, when the address is a route. */
  route?: number[] | null
}

/**
 * How many stops a route may have.
 *
 * The server holds the same bound for the request that names them. Fifty is a
 * long evening of listening; beyond it an address stops being something a
 * person sends and becomes an export.
 */
export const MAX_ROUTE = 50

/** Reads the current address. */
export function readLocation(): Location {
  return {
    artistId: readArtistId(window.location.pathname),
    view: readView(window.location.hash),
    route: readRoute(window.location.pathname),
  }
}

/**
 * `/route/54,962,120` -> [54, 962, 120]; anything else -> null.
 *
 * All or nothing, as the server reads it: one garbled stop is a garbled link,
 * and dropping it quietly would send the recipient along a different route
 * than the one they were sent. A stop may repeat — a route can come back.
 */
export function readRoute(pathname: string): number[] | null {
  const match = /^\/route\/([^/]+)\/?$/.exec(pathname)
  if (!match?.[1]) return null
  const parts = match[1].split(',')
  if (parts.length > MAX_ROUTE) return null
  const ids = parts.map(part => (/^\d+$/.test(part) ? Number(part) : NaN))
  return ids.every(id => Number.isSafeInteger(id) && id > 0) ? ids : null
}

/** A page that is not the map. */
export type Page =
  | { kind: 'charter' }
  | { kind: 'confirm'; token: string }
  | { kind: 'reset'; token: string }
  | { kind: 'embed'; artistId: number }
  | null

/**
 * Which standalone page the address asks for, if any.
 *
 * A missing token is not the same as a wrong one: `/confirm` with nothing
 * after it is a link that was truncated somewhere between the letter and the
 * browser, and the page says so rather than posting an empty token and
 * showing the server's refusal as though the link had expired.
 */
export function readPage(pathname: string, search: string): Page {
  const path = pathname.replace(/\/$/, '')
  if (path === '/charter') return { kind: 'charter' }

  // An embed is a star seen through someone else's page. It reuses the star
  // reader so `/embed/star/54` and `/star/54` cannot disagree about what a
  // valid id looks like.
  if (path.startsWith('/embed')) {
    const artistId = readArtistId(path.slice('/embed'.length))
    return artistId === null ? null : { kind: 'embed', artistId }
  }

  if (path !== '/confirm' && path !== '/reset') return null
  const token = new URLSearchParams(search).get('token') ?? ''
  return { kind: path === '/confirm' ? 'confirm' : 'reset', token }
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
export function writeLocation({ artistId, view, route }: Location): string {
  // A route owns the path while it is open; the card on screen is one of its
  // stops, and the route is what the link is about.
  const path =
    route && route.length > 0
      ? `/route/${route.join(',')}`
      : artistId === null
        ? '/'
        : `/star/${String(artistId)}`
  if (!view) return path
  const hash = `${round(view.x)},${round(view.y)},${Number(view.scale.toPrecision(3))}`
  return `${path}#${hash}`
}

function round(value: number): number {
  return Math.round(value * 100) / 100
}
