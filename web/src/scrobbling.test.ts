import { describe, expect, it } from 'vitest'

import { ago, historyRead, youthFrom, type ListenBrainzLink } from './scrobbling'

const at = (iso: string) => new Date(iso)
const now = at('2026-10-10T12:00:00Z')

describe('ago', () => {
  it('calls anything under a minute now, not a count of seconds', () => {
    expect(ago(at('2026-10-10T11:59:01Z'), now)).toEqual([0, 'second'])
  })

  it('takes the largest unit that is still a whole number', () => {
    expect(ago(at('2026-10-10T11:55:00Z'), now)).toEqual([-5, 'minute'])
    expect(ago(at('2026-10-10T10:59:00Z'), now)).toEqual([-1, 'hour'])
    expect(ago(at('2026-10-08T11:00:00Z'), now)).toEqual([-2, 'day'])
  })

  it('rounds down, so a read 119 minutes ago is one hour and not two', () => {
    expect(ago(at('2026-10-10T10:01:00Z'), now)).toEqual([-1, 'hour'])
  })

  it('reads a moment slightly in the future as now', () => {
    // A browser clock a little behind the server's makes a fresh read look
    // like it finishes in a few seconds' time.
    expect(ago(at('2026-10-10T12:00:20Z'), now)).toEqual([0, 'second'])
  })
})

describe('historyRead', () => {
  const link = (from: string | null, through: string | null): ListenBrainzLink => ({
    name: 'fixture',
    linked_at: '2026-10-10T00:00:00Z',
    read_at: null,
    failure: null,
    history: { from, through },
  })

  it('is the share of the time from the first listen to the link that is behind the cursor', () => {
    expect(historyRead(link('2006-10-10T00:00:00Z', '2016-10-10T00:00:00Z'))).toBeCloseTo(0.5, 2)
  })

  it('is nothing yet before the first read says where the history begins', () => {
    expect(historyRead(link(null, null))).toBeNull()
    expect(historyRead(link('2006-10-10T00:00:00Z', null))).toBe(0)
  })

  it('stays between none and all, whatever the clocks say', () => {
    expect(historyRead(link('2006-10-10T00:00:00Z', '2027-01-01T00:00:00Z'))).toBe(1)
    expect(historyRead(link('2026-10-10T00:00:00Z', null))).toBeNull()
  })
})

describe('youthFrom', () => {
  it('is the year someone turned fifteen', () => {
    expect(youthFrom(1983)).toBe(1998)
  })
})
