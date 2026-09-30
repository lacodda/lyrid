import { describe, expect, it } from 'vitest'

import { fitView, memberAt, peak, yearDomain, yearTicks, yearX, type Member } from './roster'
import { toMap } from './minimap'
import type { Sky } from './tiles'

const sky = { min_x: -100, max_x: 100, min_y: -100, max_y: 100, max_level: 3 } as unknown as Sky

describe('yearDomain', () => {
  it('spans the chronology and every dated end of the roster', () => {
    const counts = [
      { year: 1961, count: 4 },
      { year: 1972, count: 30 },
    ]
    expect(yearDomain(counts, [{ first: 1959, last: 1982 }])).toEqual([1959, 1982])
  })

  it('skips what is undated and is null when nothing is dated', () => {
    expect(yearDomain([], [{ first: null, last: 1990 }])).toEqual([1990, 1990])
    expect(yearDomain([], [{ first: null, last: null }])).toBeNull()
  })
})

describe('yearTicks', () => {
  it('marks round decades inside the domain', () => {
    expect(yearTicks([1959, 1988])).toEqual([1960, 1970, 1980])
  })

  it('spaces them out as the domain grows, so labels do not collide', () => {
    expect(yearTicks([1941, 2020])).toEqual([1960, 1980, 2000, 2020])
    expect(yearTicks([1858, 2024])).toEqual([1900, 1950, 2000])
  })
})

describe('yearX', () => {
  it('puts the first year at the left edge and a year per slot after it', () => {
    expect(yearX([1990, 1999], 1990, 100)).toBe(0)
    expect(yearX([1990, 1999], 1995, 100)).toBe(50)
  })

  it('survives a domain of one year', () => {
    expect(yearX([1990, 1990], 1990, 100)).toBe(0)
  })
})

describe('peak', () => {
  it('finds the busiest year, the earlier one on a tie', () => {
    expect(
      peak([
        { year: 1970, count: 5 },
        { year: 1972, count: 9 },
        { year: 1975, count: 9 },
      ])
    ).toEqual({ year: 1972, count: 9 })
    expect(peak([])).toBeNull()
  })
})

describe('fitView', () => {
  // A canvas 1000 px wide showing 100 world units: 10 px to the unit.
  const visible = { minX: -50, minY: -30, maxX: 50, maxY: 30 }
  const view = { x: 0, y: 0, scale: 10 }

  it('centres on the roster and leaves its stragglers out of the frame', () => {
    const cluster: Member[] = Array.from({ length: 18 }, (_, i) => [i, 10 + (i % 6) * 2, 20 + Math.floor(i / 6) * 2, 1])
    const stragglers: Member[] = [
      [100, -90, -90, 1],
      [101, 95, 95, 1],
    ]
    const fitted = fitView([...cluster, ...stragglers], visible, view)
    expect(fitted).not.toBeNull()
    const { x, y, scale } = fitted as { x: number; y: number; scale: number }
    expect(x).toBeGreaterThan(10)
    expect(x).toBeLessThan(20)
    expect(y).toBeGreaterThan(20)
    expect(y).toBeLessThan(24)
    // Zoomed in on the cluster, not out to the stragglers.
    expect(scale).toBeGreaterThan(view.scale)
  })

  it('frames a roster of one without zooming to infinity', () => {
    const fitted = fitView([[1, 5, 5, 1]], visible, view)
    expect(fitted?.scale).toBeCloseTo(600 / (20 * 1.3))
  })

  it('has nothing to frame for an empty roster', () => {
    expect(fitView([], visible, view)).toBeNull()
  })
})

describe('memberAt', () => {
  const members: Member[] = [
    [1, 0, 0, 1],
    [2, 50, 50, 1],
  ]

  it('picks the member under the pointer', () => {
    const at = toMap(sky, 200, 50, 50)
    expect(memberAt(members, sky, 200, at, 8)).toBe(2)
  })

  it('picks the nearer of two within reach', () => {
    const at = toMap(sky, 200, 2, 2)
    expect(memberAt(members, sky, 200, at, 100)).toBe(1)
  })

  it('is null over empty sky', () => {
    const at = toMap(sky, 200, -80, 80)
    expect(memberAt(members, sky, 200, at, 8)).toBeNull()
  })
})
