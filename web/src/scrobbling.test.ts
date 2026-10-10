import { describe, expect, it } from 'vitest'

import { ago } from './scrobbling'

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
