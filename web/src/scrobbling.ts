/**
 * Scrobbling, as the browser sees it: the linked ListenBrainz account, what
 * the listening has opened and gathered, and the stars it has lit.
 *
 * Every call is behind the session cookie, like the account's. None of it is
 * on the way to drawing the sky: a card asks what you have heard of its star
 * only after it has opened, and only when there is listening to ask about.
 */

import { messageOf } from '@/account'

/**
 * Why the last read of ListenBrainz failed, as the server stores it. A code
 * rather than a sentence so the panel can say it in the reader's language.
 */
export type ReadFailure = 'unreachable' | 'unknown_user' | 'throttled' | 'malformed'

/** The ListenBrainz account being read. */
export interface ListenBrainzLink {
  name: string
  linked_at: string
  /** When the last read that worked finished; `null` before the first. */
  read_at: string | null
  failure: ReadFailure | null
}

/** A star the listening named, and when it first did. */
export interface Opening {
  id: number
  name: string
  opened_at: string
}

/** What a person's listening has come to. */
export interface Listening {
  link: ListenBrainzLink | null
  /** All the light gathered. */
  light: number
  listens: number
  /** Stars on the current sky a listen has named. */
  stars: number
  /** The latest of them, newest first. */
  recent: Opening[]
}

/** What a read asked for by hand added, beside the summary it left. */
export interface ReadNow {
  listens: number
  light: number
  opened: number
  listening: Listening
}

/** What the listening says about one star. */
export interface Heard {
  listens: number
  first_at: string | null
  last_at: string | null
}

/** A heard star where the current sky puts it: `[id, x, y]`. */
export type HeardStar = [number, number, number]

/** Where a person finds the token ListenBrainz asks them to paste. */
export const TOKEN_PAGE = 'https://listenbrainz.org/settings/'

const same = { credentials: 'same-origin' } as const

export async function fetchListening(signal?: AbortSignal): Promise<Listening> {
  const response = await fetch('/api/me/listening', { ...same, signal })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Listening
}

/**
 * Links the ListenBrainz account a token belongs to.
 *
 * The token goes to lyrid's server once, which asks ListenBrainz whose it is
 * and then drops it. It is not kept here either: the caller clears the field.
 */
export async function linkListenBrainz(token: string): Promise<Listening> {
  const response = await fetch('/api/me/listenbrainz', {
    ...same,
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ token }),
  })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Listening
}

/** Stops reading. What the listening gathered stays with the account. */
export async function unlinkListenBrainz(): Promise<Listening> {
  const response = await fetch('/api/me/listenbrainz', { ...same, method: 'DELETE' })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Listening
}

/** Reads ListenBrainz now rather than at the next quarter hour. */
export async function readNow(): Promise<ReadNow> {
  const response = await fetch('/api/me/listenbrainz/read', { ...same, method: 'POST' })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as ReadNow
}

export async function fetchHeardStars(signal?: AbortSignal): Promise<HeardStar[]> {
  const response = await fetch('/api/me/heard', { ...same, signal })
  if (!response.ok) throw new Error(await messageOf(response))
  const body = (await response.json()) as { stars: HeardStar[] }
  return body.stars
}

export async function fetchHeard(id: number, signal?: AbortSignal): Promise<Heard> {
  const response = await fetch(`/api/me/heard/${String(id)}`, { ...same, signal })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Heard
}

/** A unit `Intl.RelativeTimeFormat` can say. */
export type AgoUnit = 'second' | 'minute' | 'hour' | 'day'

/**
 * How long ago a moment was, in the largest unit that still reads as a whole
 * number, as `[value, unit]` for `Intl.RelativeTimeFormat` -- negative, since
 * it is in the past.
 *
 * Under a minute is "now" rather than a count of seconds: a read that finished
 * while the panel was opening is not news measured in seconds. A moment in the
 * future -- a clock a little ahead of the server's -- reads as now too, rather
 * than as "in 3 seconds".
 */
export function ago(then: Date, now: Date): [number, AgoUnit] {
  const seconds = Math.max(0, Math.round((now.getTime() - then.getTime()) / 1000))
  if (seconds < 60) return [0, 'second']
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return [-minutes, 'minute']
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return [-hours, 'hour']
  return [-Math.floor(hours / 24), 'day']
}
