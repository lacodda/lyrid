/**
 * What changed in the sky: the edition of the canon it is, the stars rising
 * this week, and the releases out around now -- `GET /api/sky/changes`.
 *
 * Every star in here has a place on the current sky: the server leaves out
 * whatever this sky cannot fly to, and gives counts for the rest.
 */

import { messageOf } from '@/account'
import type { PlacedStar } from '@/scrobbling'

export interface EditionSource {
  source: string
  version: string
  imported_at: string
}

export interface Merge {
  /** The name the star had before MusicBrainz merged it. */
  from: string
  into: PlacedStar
}

export interface Changed {
  new: number
  merged: number
  gone: number
  new_stars: PlacedStar[]
  merges: Merge[]
  gone_names: string[]
}

export interface Edition {
  number: number
  sealed_at: string
  stars: number
  /** False when the sky on screen was laid out again after this edition was sealed. */
  current: boolean
  sources: EditionSource[]
  previous: { number: number; sealed_at: string } | null
  /** `null` for the first edition: nothing came before it to change. */
  changed: Changed | null
}

export interface Riser extends PlacedStar {
  /** Listeners a day this week. */
  recent: number
  /** Listeners a day over the four weeks before. */
  earlier: number
}

export interface Rising {
  /** The last day of the week; `null` until enough days are counted. */
  through: string | null
  days_counted: number
  stars: Riser[]
}

export interface Release {
  title: string
  credit: string
  kind: string | null
  out_on: string
  star: PlacedStar
}

export interface ReleaseWeek {
  from: string
  to: string
  releases: Release[]
}

export interface SkyChanges {
  edition: Edition | null
  rising: Rising
  releases: ReleaseWeek
}

export async function fetchChanges(signal?: AbortSignal): Promise<SkyChanges> {
  const response = await fetch('/api/sky/changes', { signal })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as SkyChanges
}

/**
 * What the rising list has to say.
 *
 * `counting` while lyrid has not counted enough days to call anything rising
 * -- the first weeks of a stand -- which is not the same as `quiet`, a week in
 * which it could have and nothing rose.
 */
export function risingState(rising: Rising): 'rising' | 'counting' | 'quiet' {
  if (rising.stars.length > 0) return 'rising'
  return rising.through === null ? 'counting' : 'quiet'
}

/**
 * The week's releases split at today: out already, and still to come.
 *
 * Today is the UTC day, as the server's window is, so a release dated today
 * is out everywhere on the same day. Dates are ISO strings and compare as
 * text.
 */
export function splitReleases(releases: Release[], now: Date): { out: Release[]; coming: Release[] } {
  const today = now.toISOString().slice(0, 10)
  return {
    out: releases.filter(release => release.out_on <= today),
    coming: releases.filter(release => release.out_on > today),
  }
}

/**
 * A source's version as a date, when it is one.
 *
 * The importers record what upstream calls its version, verbatim: a fullexport
 * stamp (`20260815-002140`), a dump date (`20260801`), a timestamp, or a word.
 * The dated ones read better as dates; the rest are shown as they are, with
 * the day they were imported beside them.
 */
export function versionDate(version: string): Date | null {
  const stamp = /^(\d{4})(\d{2})(\d{2})(?:-\d{6})?$/.exec(version.trim())
  if (stamp) {
    const [, year, month, day] = stamp.map(Number)
    if (year === undefined || month === undefined || day === undefined) return null
    const date = new Date(Date.UTC(year, month - 1, day))
    // Date.UTC rolls a thirteenth month into the next year rather than
    // refusing it; a stamp that does not come back as itself was not a date.
    return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day ? date : null
  }
  if (/^\d{4}-\d{2}-\d{2}[T ]/.test(version.trim())) {
    const date = new Date(version.trim().replace(' ', 'T').replace(/\+00$/, 'Z'))
    return Number.isNaN(date.getTime()) ? null : date
  }
  return null
}
