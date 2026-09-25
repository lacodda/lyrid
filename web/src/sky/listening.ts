/**
 * What the one player is playing, and the rules for what plays next.
 *
 * There is one player in the product, not one per card: a channel started from
 * a card keeps playing while the listener flies to another star, which is what
 * listening to the sky means. Three things can be on it — one artist's
 * channel, a nebula's radio, or the signal of the day — and they differ only in
 * what happens around the music, so they are one state with three kinds.
 */

import type { Nebula, Station } from '@/api'

export type Listening = { kind: 'channel'; station: Station } | RadioListening | SignalListening

export interface RadioListening {
  kind: 'radio'
  nebula: Nebula
  /** Every station queued so far, played or not, in order. */
  stations: Station[]
  /** The one playing. */
  at: number
  /** The seed of the queue being played, which also picks each upload. */
  seed: number
}

export interface SignalListening {
  kind: 'signal'
  /**
   * The day's signal and the stars behind it, the same list for everyone:
   * when a channel will not play, the next one in the list is the signal.
   */
  stations: Station[]
  at: number
  day: string
  /** Whether the listener has reached the star, which is what names it. */
  found: boolean
}

/** The star the player is sounding, if anything is playing. */
export function sounding(listening: Listening | null): Station | null {
  if (!listening) return null
  if (listening.kind === 'channel') return listening.station
  return listening.stations[listening.at] ?? null
}

/**
 * The radio one station along, or `null` past either end.
 *
 * Past the end is not a stop: it is the moment to ask for more (see
 * `refill`). Before the start is simply nothing — the first station of a radio
 * has no previous one.
 */
export function stepRadio(radio: RadioListening, by: 1 | -1): RadioListening | null {
  const at = radio.at + by
  return at >= 0 && at < radio.stations.length ? { ...radio, at } : null
}

/** How many of the latest stations a fresh queue must not bring straight back. */
export const RECENT = 12

/**
 * A spent radio carried on into a fresh queue.
 *
 * The fresh queue is drawn with a new seed, so it may open on a star that has
 * just played; those are dropped from it. A nebula smaller than that window
 * would drop everything, and then repeating is the only way to keep playing —
 * so the fresh queue is taken whole rather than the radio falling silent.
 */
export function refill(radio: RadioListening, fresh: readonly Station[], seed: number): RadioListening {
  const recent = new Set(radio.stations.slice(Math.max(0, radio.at + 1 - RECENT), radio.at + 1).map(station => station.id))
  const unheard = fresh.filter(station => !recent.has(station.id))
  const next = unheard.length > 0 ? unheard : [...fresh]
  return { ...radio, stations: [...radio.stations, ...next], at: radio.at + 1, seed }
}

/**
 * Whether the view follows the radio to its next station.
 *
 * Only when the card on screen is the star that was playing: a listener
 * reading along goes on to the next one, and a listener who went off to look at
 * something else is not pulled away from it. There is no setting for this —
 * where the card is says which of the two the listener is doing.
 */
export function follows(openCard: number | null, playing: Station | null): boolean {
  return openCard !== null && playing !== null && openCard === playing.id
}

/** How far back into a channel's uploads a radio reaches for one song. */
export const RECENT_UPLOADS = 10

/**
 * Which of a channel's uploads the radio plays: one of the latest few,
 * fixed by the queue's seed and the star.
 *
 * Not always the newest: an artist's newest upload is as often a teaser or an
 * interview as a song, and every radio of the same nebula would play the same
 * one. Not from deep in the channel either — a catalogue uploaded ten years ago
 * is where dead videos gather.
 */
export function uploadIndex(seed: number, starId: number, available: number): number {
  const span = Math.min(available, RECENT_UPLOADS)
  if (span <= 1) return 0
  let hash = Math.imul(seed ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(starId, 0xc2b2ae35)
  hash = Math.imul(hash ^ (hash >>> 15), 0x2c1b3c6d)
  hash ^= hash >>> 13
  return (hash >>> 0) % span
}

/**
 * The listener's own calendar day, `YYYY-MM-DD`.
 *
 * Local rather than UTC: the signal of the day turns over at the listener's
 * midnight. A listener in the evening of the Americas asking in UTC would be
 * handed tomorrow's signal.
 */
export function localDay(now: Date): string {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${String(now.getFullYear())}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

/** A seed for a new queue: 32 bits, well inside what a URL and a u64 carry. */
export function newSeed(): number {
  return Math.floor(Math.random() * 2 ** 32)
}
