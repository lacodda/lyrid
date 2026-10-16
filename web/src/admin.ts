/**
 * The stand's own state, as `GET /api/admin` gives it to an admin: the canon
 * it holds, the health of its feeds, and the counts of its accounts and
 * mechanics. Counts only -- nobody is named.
 */

import { messageOf } from '@/account'

export interface AdminEdition {
  number: number
  sealed_at: string
  layout: string
  artists: number
  stars: number
  new: number
  merged: number
  gone: number
}

export interface Import {
  source: string
  version: string
  started_at: string
  finished_at: string | null
  rows: number | null
}

export interface Run {
  source: string
  started_at: string
  finished_at: string | null
  span_from: string | null
  span_to: string | null
  rows_read: number | null
  rows_kept: number | null
  error: string | null
}

export interface Health {
  last_done: string | null
  last_failed: string | null
  failed_this_week: number
  runs: Run[]
}

export interface Coverage {
  day: string
  covered_seconds: number
  listeners: number | null
  closed: boolean
}

export interface Overview {
  version: string
  canon: {
    artists: number
    stars: number
    layout: string | null
    laid_out_at: string | null
    slice: boolean
    editions: AdminEdition[]
    imports: Import[]
  }
  feeds: {
    listening: Health & {
      days: Coverage[]
      waiting: number
      counted: number
      rising_through: string | null
      risers: number
    }
    releases: Health & { known: number; out_this_week: number }
  }
  accounts: {
    total: number
    confirmed: number
    joined_this_week: number
    joined_this_month: number
    admins: number
    sessions: number
    listenbrainz: {
      linked: number
      reading_history: number
      failing: { failure: string; links: number }[]
      last_read: string | null
      listens: number
    }
    lastfm: number
    letters: { subscribed: number; last_week: string | null }
  }
  mechanics: { mechanic: string; total: number; days: { day: string; uses: number }[] }[]
}

/** Why the page shows nothing: not signed in, or not an admin. */
export class Refused extends Error {
  readonly status: 401 | 403

  constructor(status: 401 | 403) {
    super(status === 401 ? 'not signed in' : 'not an admin')
    this.status = status
  }
}

export async function fetchOverview(signal?: AbortSignal): Promise<Overview> {
  const response = await fetch('/api/admin', { credentials: 'same-origin', signal })
  if (response.status === 401 || response.status === 403) throw new Refused(response.status)
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Overview
}

/**
 * How a feed is doing, from its record.
 *
 * `failing` when the last thing it did was fail -- a failure after a success,
 * not merely a failure some time this week, which a feed that recovered does
 * not need flagged. `never` for a feed that has not run on this stand.
 */
export function feedState(health: Pick<Health, 'last_done' | 'last_failed' | 'runs'>): 'ok' | 'failing' | 'running' | 'never' {
  const newest = health.runs[0]
  if (newest && newest.finished_at === null) return 'running'
  if (health.last_done === null && health.last_failed === null) return 'never'
  if (health.last_failed === null) return 'ok'
  if (health.last_done === null) return 'failing'
  return Date.parse(health.last_failed) > Date.parse(health.last_done) ? 'failing' : 'ok'
}

/** How long a run took, in whole seconds, or `null` while it runs. */
export function took(run: Run): number | null {
  if (run.finished_at === null) return null
  return Math.max(0, Math.round((Date.parse(run.finished_at) - Date.parse(run.started_at)) / 1000))
}

/** The share of a day the dumps covered, from 0 to 1. */
export function covered(day: Coverage): number {
  return Math.min(1, Math.max(0, day.covered_seconds / 86_400))
}

/**
 * A closed day ListenBrainz all but skipped. Not only the days with no second
 * covered: when it publishes no dump for a day, the few seconds of the empty
 * dump cut around midnight still land on it, so a skipped day reads as three
 * seconds, not as nothing. Less than half a day is a day missing.
 */
export function missing(day: Coverage): boolean {
  return day.closed && covered(day) < 0.5
}

/**
 * The short name of a daily dump: its running number and date out of
 * `listenbrainz-dump-2724-20261016-000002-incremental`. Anything else -- the
 * releases feed's days -- is shown as it is.
 */
export function shortSource(source: string): string {
  const match = /^listenbrainz-dump-(\d+)-(\d{4})(\d{2})(\d{2})-\d{6}-incremental$/.exec(source)
  if (!match) return source
  const [, sequence, year, month, day] = match
  return `${sequence ?? ''} · ${year ?? ''}-${month ?? ''}-${day ?? ''}`
}
