import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Station } from '@/api'
import { follows, HEARD_AFTER_MS, heardClock, localDay, RECENT, RECENT_UPLOADS, refill, sounding, stepRadio, type RadioListening, uploadIndex } from './listening'

const station = (id: number): Station => ({ id, mbid: `mbid-${String(id)}`, name: `Star ${String(id)}`, x: id, y: -id, uploads: `UU${String(id)}` })

const radio = (ids: number[], at: number): RadioListening => ({
  kind: 'radio',
  nebula: { name: 'Soul', kind: 'style' },
  stations: ids.map(station),
  at,
  seed: 1,
})

describe('what the player is sounding', () => {
  it('is the station a radio is at', () => {
    expect(sounding(radio([1, 2, 3], 1))?.id).toBe(2)
    expect(sounding(null)).toBeNull()
  })
})

describe('stepping through a radio', () => {
  it('moves one station at a time and stops at either end', () => {
    expect(stepRadio(radio([1, 2, 3], 0), 1)?.at).toBe(1)
    expect(stepRadio(radio([1, 2, 3], 2), 1)).toBeNull()
    expect(stepRadio(radio([1, 2, 3], 0), -1)).toBeNull()
  })
})

describe('carrying a spent radio into a fresh queue', () => {
  it('drops the stars that have just played', () => {
    const next = refill(radio([1, 2, 3], 2), [3, 4, 2, 5].map(station), 9)
    expect(next.stations.slice(3).map(s => s.id)).toEqual([4, 5])
    // The first of the fresh stations is the one that plays next.
    expect(sounding(next)?.id).toBe(4)
    expect(next.seed).toBe(9)
  })

  it('forgets what played long enough ago', () => {
    // A star heard more than a window ago may come round again: a radio of a
    // modest nebula would otherwise run dry.
    const played = Array.from({ length: RECENT + 5 }, (_, index) => index + 1)
    const next = refill(radio(played, played.length - 1), [1, played.length].map(station), 2)
    expect(next.stations.slice(played.length).map(s => s.id)).toEqual([1])
  })

  it('repeats rather than falling silent in a nebula smaller than the window', () => {
    const next = refill(radio([1, 2], 1), [2, 1].map(station), 3)
    expect(next.stations.slice(2).map(s => s.id)).toEqual([2, 1])
  })
})

describe('whether the view follows the radio', () => {
  it('follows only a listener whose open card is the star that was playing', () => {
    expect(follows(7, station(7))).toBe(true)
    expect(follows(8, station(7))).toBe(false)
    expect(follows(null, station(7))).toBe(false)
    expect(follows(7, null)).toBe(false)
  })
})

describe('which upload the radio plays', () => {
  it('stays within the latest few, and within what the channel has', () => {
    for (let seed = 0; seed < 200; seed += 1) {
      expect(uploadIndex(seed, 54, 200)).toBeLessThan(RECENT_UPLOADS)
      expect(uploadIndex(seed, 54, 3)).toBeLessThan(3)
      expect(uploadIndex(seed, 54, 1)).toBe(0)
      expect(uploadIndex(seed, 54, 0)).toBe(0)
    }
  })

  it('is fixed by the seed and the star, and moves when either does', () => {
    expect(uploadIndex(5, 54, 50)).toBe(uploadIndex(5, 54, 50))
    const bySeed = new Set(Array.from({ length: 40 }, (_, seed) => uploadIndex(seed, 54, 50)))
    const byStar = new Set(Array.from({ length: 40 }, (_, star) => uploadIndex(5, star, 50)))
    // Not the newest upload every time, which is what a constant would give.
    expect(bySeed.size).toBeGreaterThan(5)
    expect(byStar.size).toBeGreaterThan(5)
  })
})

describe("the listener's day", () => {
  it('is the local calendar date, padded', () => {
    expect(localDay(new Date(2026, 8, 5, 23, 59))).toBe('2026-09-05')
    expect(localDay(new Date(2026, 11, 31, 0, 1))).toBe('2026-12-31')
  })
})

describe('the clock that says a star was heard', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('fires once the star has sounded for long enough, and only once', () => {
    const fire = vi.fn()
    const clock = heardClock(fire)
    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS - 1)
    expect(fire).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(fire).toHaveBeenCalledOnce()

    // A video that ends and starts again is the same star, already heard.
    clock.paused()
    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS * 2)
    expect(fire).toHaveBeenCalledOnce()
  })

  it('counts sound and not time: a pause holds it', () => {
    const fire = vi.fn()
    const clock = heardClock(fire)
    clock.playing()
    vi.advanceTimersByTime(10_000)
    clock.paused()
    vi.advanceTimersByTime(HEARD_AFTER_MS * 10)
    expect(fire).not.toHaveBeenCalled()

    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS - 10_000 - 1)
    expect(fire).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(fire).toHaveBeenCalledOnce()
  })

  it('does not start over when the player says it is playing twice', () => {
    const fire = vi.fn()
    const clock = heardClock(fire)
    clock.playing()
    vi.advanceTimersByTime(20_000)
    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS - 20_000)
    expect(fire).toHaveBeenCalledOnce()

    // And it has not left a second timer behind to fire later.
    vi.advanceTimersByTime(HEARD_AFTER_MS)
    expect(fire).toHaveBeenCalledOnce()
  })

  it('never fires for a player that never plays', () => {
    const fire = vi.fn()
    const clock = heardClock(fire)
    clock.paused()
    vi.advanceTimersByTime(HEARD_AFTER_MS * 10)
    expect(fire).not.toHaveBeenCalled()
  })

  it('never fires once the player is gone', () => {
    const fire = vi.fn()
    const clock = heardClock(fire)
    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS - 1)
    clock.stop()
    vi.advanceTimersByTime(HEARD_AFTER_MS)
    expect(fire).not.toHaveBeenCalled()

    clock.playing()
    vi.advanceTimersByTime(HEARD_AFTER_MS)
    expect(fire).not.toHaveBeenCalled()
  })
})
