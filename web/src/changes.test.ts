import { describe, expect, it } from 'vitest'

import { feedState, missing, shortSource, took } from './admin'
import { risingState, splitReleases, versionDate, type Release, type Rising } from './changes'

const star = { id: 1, name: 'Star', x: 0, y: 0 }

describe('what the rising list has to say', () => {
  const rising = (through: string | null, stars: Rising['stars']): Rising => ({ through, days_counted: 4, stars })

  it('names the risers when there are any', () => {
    expect(risingState(rising('2026-10-15', [{ ...star, recent: 20, earlier: 2 }]))).toBe('rising')
  })

  it('tells a sky still counting apart from a quiet week', () => {
    // Nothing rising because nothing could be called rising yet is not the
    // same news as a week in which nothing rose.
    expect(risingState(rising(null, []))).toBe('counting')
    expect(risingState(rising('2026-10-15', []))).toBe('quiet')
  })
})

describe('the week of releases', () => {
  const release = (out_on: string): Release => ({ title: out_on, credit: 'Star', kind: 'Album', out_on, star })

  it('splits at today in UTC, with today counted as out', () => {
    // 23:30 in São Paulo on the 16th is already the 17th in UTC.
    const now = new Date('2026-10-17T02:30:00Z')
    const { out, coming } = splitReleases([release('2026-10-16'), release('2026-10-17'), release('2026-10-18')], now)
    expect(out.map(r => r.out_on)).toEqual(['2026-10-16', '2026-10-17'])
    expect(coming.map(r => r.out_on)).toEqual(['2026-10-18'])
  })
})

describe('reading a source version as a date', () => {
  it('reads the stamps the importers record', () => {
    expect(versionDate('20260815-002140')?.toISOString()).toBe('2026-08-15T00:00:00.000Z')
    expect(versionDate('20260801')?.toISOString()).toBe('2026-08-01T00:00:00.000Z')
    expect(versionDate('listenbrainz-dump-2723-20261015-000002-full')?.toISOString()).toBe('2026-10-15T00:00:00.000Z')
    expect(versionDate('listenbrainz-dump-2723-2026101-000002-full')).toBeNull()
    expect(versionDate('2026-08-15 00:21:41.385745+00')?.toISOString().slice(0, 10)).toBe('2026-08-15')
    expect(versionDate('2020-08-13T12:41:07.693781+00:00')?.toISOString().slice(0, 10)).toBe('2020-08-13')
  })

  it('leaves a version that is a word as it is', () => {
    expect(versionDate('latest')).toBeNull()
    expect(versionDate('test-mini')).toBeNull()
    // Eight digits that are not a date are not one.
    expect(versionDate('20261399')).toBeNull()
  })
})

describe('a feed as its record tells it', () => {
  const run = (finished_at: string | null) => ({
    source: 's',
    started_at: '2026-10-16T10:00:00Z',
    finished_at,
    span_from: null,
    span_to: null,
    rows_read: null,
    rows_kept: null,
    error: null,
  })

  it('is failing only when the last thing it did was fail', () => {
    const done = '2026-10-16T10:00:00Z'
    const failed = '2026-10-16T11:00:00Z'
    expect(feedState({ last_done: done, last_failed: failed, runs: [run(failed)] })).toBe('failing')
    // A failure that a later run recovered from is history, not a state.
    expect(feedState({ last_done: failed, last_failed: done, runs: [run(failed)] })).toBe('ok')
    expect(feedState({ last_done: null, last_failed: failed, runs: [run(failed)] })).toBe('failing')
    expect(feedState({ last_done: null, last_failed: null, runs: [] })).toBe('never')
  })

  it('is reading while its newest run has not finished', () => {
    expect(feedState({ last_done: '2026-10-16T10:00:00Z', last_failed: null, runs: [run(null)] })).toBe('running')
  })

  it('times a finished run and not a running one', () => {
    expect(took(run('2026-10-16T10:00:55.400Z'))).toBe(55)
    expect(took(run(null))).toBeNull()
  })

  it('calls a day missing when ListenBrainz all but skipped it', () => {
    // A skipped day still gets the seconds of the empty dump cut at midnight.
    expect(missing({ day: '2026-10-03', covered_seconds: 3, listeners: 9, closed: true })).toBe(true)
    expect(missing({ day: '2026-10-04', covered_seconds: 0, listeners: 0, closed: true })).toBe(true)
    expect(missing({ day: '2026-10-05', covered_seconds: 86_397, listeners: 20_605, closed: true })).toBe(false)
    // A day still open is not missing: it is not over.
    expect(missing({ day: '2026-10-16', covered_seconds: 4, listeners: null, closed: false })).toBe(false)
  })

  it('shortens a daily dump to its number and date', () => {
    expect(shortSource('listenbrainz-dump-2724-20261016-000002-incremental')).toBe('2724 · 2026-10-16')
    expect(shortSource('2026-10-16')).toBe('2026-10-16')
  })
})
