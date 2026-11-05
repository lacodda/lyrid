import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { MOST_REMEMBERED, forget, remember, remembered } from './openings'

/** A storage that holds strings, as the browser's does. The tests run without a DOM. */
function storage(): Storage {
  const held = new Map<string, string>()
  return {
    get length() {
      return held.size
    },
    clear: () => held.clear(),
    getItem: key => held.get(key) ?? null,
    key: index => [...held.keys()][index] ?? null,
    removeItem: key => {
      held.delete(key)
    },
    setItem: (key, value) => {
      held.set(key, value)
    },
  }
}

describe('what a browser remembers before anyone signs in', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { localStorage: storage() })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('holds a star once, from the first time', () => {
    expect(remember('a', 1000)).toBe(true)
    expect(remember('b', 2000)).toBe(true)
    expect(remember('a', 3000)).toBe(false)
    expect(remembered()).toEqual([
      { mbid: 'a', at: 1000 },
      { mbid: 'b', at: 2000 },
    ])
  })

  it('forgets everything once the account holds it', () => {
    remember('a')
    forget()
    expect(remembered()).toEqual([])
  })

  it('reads a hand-edited entry as nothing rather than failing the sky', () => {
    const { localStorage } = window
    localStorage.setItem('lyrid.opened', 'not json')
    expect(remembered()).toEqual([])
    localStorage.setItem('lyrid.opened', '{"mbid":"a"}')
    expect(remembered()).toEqual([])
    localStorage.setItem('lyrid.opened', '[{"mbid":"a","at":1},{"mbid":2},null,"b"]')
    expect(remembered()).toEqual([{ mbid: 'a', at: 1 }])
  })

  it('keeps what it has when it is full', () => {
    const full = Array.from({ length: MOST_REMEMBERED }, (_, index) => ({ mbid: String(index), at: index }))
    window.localStorage.setItem('lyrid.opened', JSON.stringify(full))
    expect(remember('one more')).toBe(false)
    expect(remembered()).toHaveLength(MOST_REMEMBERED)
  })

  it('carries on when storage is refused outright', () => {
    vi.stubGlobal('window', {
      get localStorage(): Storage {
        throw new Error('SecurityError')
      },
    })
    expect(remembered()).toEqual([])
    expect(remember('a')).toBe(false)
    expect(() => forget()).not.toThrow()
  })
})
