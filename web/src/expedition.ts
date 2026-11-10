/**
 * The expedition, as the browser sees it: the home port a person set out from,
 * and the contracts the game hands them.
 *
 * The server works out every number here from the openings (ADR 0028); this
 * file only reads it, and decides what the panel shows.
 */

import { messageOf } from '@/account'
import type { Echo, PersonalSky } from '@/sky/fog'

/** A star the expedition names: always by MusicBrainz id, placed when the sky holds it. */
export interface PortStar {
  mbid: string
  id: number | null
  name: string | null
  x: number | null
  y: number | null
}

/** The home port, with the channel the player lands on when it has one. */
export interface Home extends PortStar {
  uploads: string | null
}

export interface Contract {
  id: number
  /** `home_sector`: open new stars of the home port's neighbourhood. */
  kind: string
  star: PortStar
  goal: number
  progress: number
  /** RFC 3339. */
  issued_at: string
  /** RFC 3339; `null` while it is still to do. */
  done_at: string | null
}

export interface Expedition {
  /** `null` until it has begun. */
  home: Home | null
  contracts: Contract[]
}

export const NO_EXPEDITION: Expedition = { home: null, contracts: [] }

export async function fetchExpedition(signal?: AbortSignal): Promise<Expedition> {
  const response = await fetch('/api/me/expedition', { signal, credentials: 'same-origin' })
  if (!response.ok) throw new Error('the expedition could not be read')
  return (await response.json()) as Expedition
}

/**
 * Sets out from the artist with this MusicBrainz id. Rejects with the server's
 * own words when it refuses: a star off the sky, an expedition already begun.
 */
export async function beginExpedition(mbid: string): Promise<Expedition> {
  const response = await fetch('/api/me/expedition', {
    method: 'PUT',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ mbid }),
  })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Expedition
}

/**
 * The contract the panel is about: the oldest one still to do, or else one
 * fulfilled while this page was open and not yet put away -- the moment a
 * contract is fulfilled deserves to be seen, and a contract fulfilled last week
 * does not need to be seen again on every visit.
 */
export function contractToShow(expedition: Expedition, fulfilledHere: ReadonlySet<number>, putAway: ReadonlySet<number>): Contract | null {
  const open = expedition.contracts.find(contract => contract.done_at === null)
  if (open) return open
  return expedition.contracts.find(contract => fulfilledHere.has(contract.id) && !putAway.has(contract.id)) ?? null
}

/**
 * The contracts that were open in one answer and are fulfilled in the next:
 * the ones fulfilled while this page was open.
 */
export function newlyFulfilled(before: Expedition, after: Expedition): number[] {
  const wasOpen = new Set(before.contracts.filter(contract => contract.done_at === null).map(contract => contract.id))
  return after.contracts.filter(contract => contract.done_at !== null && wasOpen.has(contract.id)).map(contract => contract.id)
}

/**
 * The echoes of one star in this person's sky: the next steps a contract about
 * that star's neighbourhood is fulfilled by, closest first as the server sends
 * them.
 */
export function echoesOf(sky: PersonalSky, starId: number | null): Echo[] {
  if (starId === null) return []
  return sky.echoes.filter(echo => echo.of.includes(starId))
}

/** Whether the question "which artist do you love?" is still to be asked of this profile. */
export function asksForHome(me: { mode: string | null; home_port: string | null } | null): boolean {
  return me !== null && me.mode === 'explore' && me.home_port === null
}
