/**
 * Telling someone that a star has opened: the server, when anyone is signed in,
 * and the browser's own memory until then.
 *
 * Someone listening to the sky without an account has done the thing the fog is
 * about, and signing up must not cost them it. So what they heard is kept here
 * by MusicBrainz id, which is what outlives a rebuilt canon, and moved into the
 * account at the next sign-in of any kind. It is not on any server until then:
 * the charter says so, and it is true of this file and nothing else.
 */

import type { Echo, OpenStar, PersonalSky } from './fog'

export type How = 'played' | 'followed'

/** A star a browser remembers opening. */
export interface Remembered {
  mbid: string
  /** Milliseconds since the epoch. */
  at: number
}

const KEY = 'lyrid.opened'

/**
 * The most a browser keeps, and the most the server takes at once. Past what
 * anyone listens to without an account; a star past it is simply not remembered,
 * which beats filling someone's storage.
 */
export const MOST_REMEMBERED = 5000

function isRemembered(value: unknown): value is Remembered {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.mbid === 'string' && typeof candidate.at === 'number'
}

/**
 * What this browser remembers. Empty when it remembers nothing, when storage is
 * refused, or when what is stored is not what this code wrote: this is one of
 * the few inputs a person can edit by hand, and a bad entry must not stop the sky.
 */
export function remembered(): Remembered[] {
  try {
    const stored = window.localStorage.getItem(KEY)
    if (stored === null) return []
    const parsed: unknown = JSON.parse(stored)
    return Array.isArray(parsed) ? parsed.filter(isRemembered) : []
  } catch {
    return []
  }
}

/** Remembers a star. `true` when it is one this browser did not hold yet. */
export function remember(mbid: string, now = Date.now()): boolean {
  const held = remembered()
  if (held.length >= MOST_REMEMBERED || held.some(star => star.mbid === mbid)) return false
  try {
    window.localStorage.setItem(KEY, JSON.stringify([...held, { mbid, at: now }]))
    return true
  } catch {
    // Nothing to do and nothing to say: the music still played.
    return false
  }
}

/** Forgets them, once the account holds them. */
export function forget(): void {
  try {
    window.localStorage.removeItem(KEY)
  } catch {
    // Storage that refuses to be written to holds nothing to forget.
  }
}

async function refused(response: Response, what: string): Promise<void> {
  if (!response.ok) throw new Error(what)
  await response.body?.cancel()
}

/** A star was heard, or followed out to be heard. Safe to say twice. */
export async function openStar(mbid: string, how: How): Promise<void> {
  const response = await fetch(`/api/me/openings/${encodeURIComponent(mbid)}`, {
    method: 'PUT',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ how }),
  })
  await refused(response, 'the star could not be opened')
}

/** Moves what this browser remembers into the account. Safe to send twice. */
export async function bringOpenings(stars: readonly Remembered[]): Promise<void> {
  const response = await fetch('/api/me/openings/brought', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ stars }),
  })
  await refused(response, 'the stars could not be brought')
}

type OpenRow = [number, number, number, number, number, string, string]
type EchoRow = [number, number, number, string, number[]]

/** The stars that are this person's, in the sky being shown now. */
export async function fetchPersonalSky(signal?: AbortSignal): Promise<PersonalSky> {
  const response = await fetch('/api/me/sky', { signal, credentials: 'same-origin' })
  if (!response.ok) throw new Error('your sky could not be read')
  const body = (await response.json()) as { opened: OpenRow[]; echoes: EchoRow[] }
  return {
    opened: body.opened.map(
      ([artistId, x, y, brightness, year, name, openedAt]): OpenStar => ({ artistId, x, y, brightness, year, name, openedAt })
    ),
    echoes: body.echoes.map(([artistId, x, y, name, of]): Echo => ({ artistId, x, y, name, of })),
  }
}
