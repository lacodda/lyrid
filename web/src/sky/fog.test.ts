import { describe, expect, it } from 'vitest'

import { COVERAGE_SIDE, LIGHT_RADIUS, OPEN_FLOOR, READABLE, buildFog, chartLabels, chartedAt, listInFog, litAt, pickInFog, placeOpenNames, type Echo, type OpenStar, type PersonalSky } from './fog'
import type { Label } from './labels'
import type { Star } from './renderer'
import type { Sky } from './tiles'

const sky: Sky = { min_x: -1000, min_y: -1000, max_x: 1000, max_y: 1000, max_level: 3, record_bytes: 20 }

const open = (artistId: number, x: number, y: number, brightness = 0.6): OpenStar => ({
  artistId,
  x,
  y,
  brightness,
  year: 1990,
  name: `Star ${String(artistId)}`,
  openedAt: '2026-11-05T10:00:00Z',
})

const echo = (artistId: number, x: number, y: number, of: number[]): Echo => ({ artistId, name: `Echo ${String(artistId)}`, x, y, of })

const fogOf = (personal: Partial<PersonalSky>) => buildFog(sky, { opened: [], echoes: [], ...personal })

const field = (artistId: number, x: number, y: number): Star => ({ artistId, x, y, brightness: 0.5, year: 1990 })

describe('the coverage', () => {
  it('lights an open star fully, and nothing past its radius', () => {
    const fog = fogOf({ opened: [open(1, 100, 200)] })
    expect(litAt(fog, 100, 200)).toBe(1)
    expect(litAt(fog, 100 + LIGHT_RADIUS * 0.3, 200)).toBe(1)
    expect(litAt(fog, 100 + LIGHT_RADIUS * 1.5, 200)).toBe(0)
  })

  it('fades rather than cuts', () => {
    const fog = fogOf({ opened: [open(1, 0, 0)] })
    const edge = litAt(fog, LIGHT_RADIUS * 0.75, 0)
    expect(edge).toBeGreaterThan(0)
    expect(edge).toBeLessThan(1)
  })

  it('keeps x and y apart', () => {
    // A texture written row-first and read column-first would light the mirror
    // image of the sky, which looks exactly as plausible.
    const fog = fogOf({ opened: [open(1, 400, -300)] })
    expect(litAt(fog, 400, -300)).toBe(1)
    expect(litAt(fog, -300, 400)).toBe(0)
  })

  it('takes the stronger light rather than adding', () => {
    // Halfway between two stars, in the soft edge of both: a sum would call this
    // fully lit, which is how every dense neighbourhood would become a white blob.
    const one = fogOf({ opened: [open(1, 0, 0)] })
    const two = fogOf({ opened: [open(1, 0, 0), open(2, LIGHT_RADIUS * 1.4, 0)] })
    const between = LIGHT_RADIUS * 0.7
    expect(litAt(one, between, 0)).toBeLessThan(0.8)
    expect(litAt(two, between, 0)).toBeCloseTo(litAt(one, between, 0), 1)
  })

  it('does not wrap the light of a star at the edge round to the other side', () => {
    const fog = fogOf({ opened: [open(1, 999, 0)] })
    expect(litAt(fog, -999, 0)).toBe(0)
    expect(litAt(fog, -999, fog.span / COVERAGE_SIDE)).toBe(0)
  })

  it('says nothing is lit outside the sky rather than reading past the texture', () => {
    const fog = fogOf({ opened: [open(1, 0, 0)] })
    expect(litAt(fog, 5000, 0)).toBe(0)
    expect(litAt(fog, 0, -5000)).toBe(0)
  })

  it('covers a sky that is taller than it is wide', () => {
    const tall = buildFog({ ...sky, min_x: -100, max_x: 100 }, { opened: [open(1, 0, 900)], echoes: [] })
    expect(tall.span).toBe(2000)
    expect(litAt(tall, 0, 900)).toBe(1)
  })
})

describe('the open stars', () => {
  it('are packed as the renderer reads them, and no dimmer than the floor', () => {
    const fog = fogOf({ opened: [open(1, 10, 20, 0.05), open(2, 30, 40, 0.9)] })
    expect([...fog.packed]).toEqual([10, 20, expect.closeTo(OPEN_FLOOR), 1990, 30, 40, expect.closeTo(0.9), 1990])
  })
})

describe('the names on the sky', () => {
  const genre: Label = { name: 'Shoegaze', kind: 'genre', x: 0, y: 0, members: 10 }
  const style: Label = { name: 'Dream Pop', kind: 'style', x: 0, y: 0, members: 5 }

  it('are known where someone has been', () => {
    const fog = fogOf({ opened: [open(1, 10, 10)] })
    expect(chartLabels([genre, style], fog, 'uncharted')).toEqual([genre, style])
  })

  it('are uncharted for a genre and not written for a style everywhere else', () => {
    const fog = fogOf({ opened: [open(1, 900, 900)] })
    expect(chartLabels([genre, style], fog, 'uncharted')).toEqual([{ ...genre, name: 'uncharted' }])
  })

  it('reach a neighbouring cell, so a star just across a boundary still counts', () => {
    const fog = fogOf({ opened: [open(1, 60, 0)] })
    expect(chartedAt(fog, 70, 0)).toBe(true)
    expect(chartedAt(fog, 500, 0)).toBe(false)
  })
})

describe('picking in the fog', () => {
  it('refuses a silhouette, however close the click', () => {
    const fog = fogOf({ opened: [open(1, 500, 500)] })
    expect(pickInFog(fog, [field(9, 0, 0)], { x: 0, y: 0 }, 10)).toBeNull()
  })

  it('takes a star the light reaches', () => {
    const fog = fogOf({ opened: [open(1, 0, 0)] })
    const near = field(9, 3, 0)
    expect(litAt(fog, 3, 0)).toBeGreaterThanOrEqual(READABLE)
    expect(pickInFog(fog, [near], { x: 3, y: 0 }, 1)?.artistId).toBe(9)
  })

  it('takes an open star this level of the pyramid does not hold', () => {
    const fog = fogOf({ opened: [open(1, 0, 0)] })
    expect(pickInFog(fog, [], { x: 0, y: 0 }, 5)?.artistId).toBe(1)
  })

  it('takes an echo in the dark', () => {
    const fog = fogOf({ opened: [open(1, 500, 500)], echoes: [echo(2, 0, 0, [1])] })
    expect(pickInFog(fog, [], { x: 0, y: 0 }, 5)?.artistId).toBe(2)
  })

  it('takes the nearest of them', () => {
    const fog = fogOf({ opened: [open(1, 0, 0)], echoes: [echo(2, 4, 0, [1])] })
    expect(pickInFog(fog, [field(3, 8, 0)], { x: 3.5, y: 0 }, 10)?.artistId).toBe(2)
  })
})

describe('the list of the view', () => {
  const view = { minX: -100, minY: -100, maxX: 100, maxY: 100 }

  it('holds the open stars in view, brightest first, and then their echoes without their names', () => {
    const fog = fogOf({
      opened: [open(1, 0, 0, 0.2), open(2, 10, 10, 0.9), open(3, 900, 900, 1)],
      echoes: [echo(4, 5, 5, [2, 1]), echo(5, 900, 0, [3])],
    })
    expect(listInFog(fog, view)).toEqual([
      { id: 2, x: 10, y: 10, name: 'Star 2', echoOf: null },
      { id: 1, x: 0, y: 0, name: 'Star 1', echoOf: null },
      { id: 4, x: 5, y: 5, name: null, echoOf: 'Star 2' },
    ])
  })

  it('is empty where nothing is lit', () => {
    expect(listInFog(fogOf({ opened: [open(1, 900, 900)] }), view)).toEqual([])
  })

  it('stays short however much is in view', () => {
    const many = Array.from({ length: 50 }, (_, index) => open(index + 1, 0, 0))
    const reflected = Array.from({ length: 50 }, (_, index) => echo(index + 100, 0, 0, [1]))
    expect(listInFog(fogOf({ opened: many, echoes: reflected }), view)).toHaveLength(16)
  })
})

describe('the names of open stars on the sky', () => {
  const camera = { x: 0, y: 0, scale: 1 }
  const viewport = { width: 1000, height: 800 }
  const measure = (name: string) => name.length * 8

  it('are written beside their stars, on the screen the way the sky is', () => {
    const fog = fogOf({ opened: [open(1, 100, 50)] })
    // y grows upward in the sky and downward on the screen.
    expect(placeOpenNames(fog, camera, viewport, measure, 20)).toEqual([{ name: 'Star 1', x: 600 + 14, y: 350 }])
  })

  it('leave out what is off the screen, or would run off its edge', () => {
    const fog = fogOf({ opened: [open(1, 900, 0), open(2, 495, 0), open(3, 0, -395)] })
    expect(placeOpenNames(fog, camera, viewport, measure, 20)).toEqual([])
  })

  it('give way to a brighter star rather than overlap it', () => {
    const fog = fogOf({ opened: [open(1, 0, 0, 0.2), open(2, 5, 0, 0.9), open(3, 0, 200, 0.5)] })
    expect(placeOpenNames(fog, camera, viewport, measure, 20).map(placed => placed.name)).toEqual(['Star 2', 'Star 3'])
  })

  it('stop at two dozen however many fit', () => {
    const many = Array.from({ length: 200 }, (_, index) => open(index + 1, -450 + (index % 10) * 90, -380 + Math.floor(index / 10) * 38))
    expect(placeOpenNames(fogOf({ opened: many }), camera, viewport, () => 10, 12)).toHaveLength(24)
  })
})

describe('the echoes of an open star', () => {
  it('are found from the star, and one silhouette can belong to two', () => {
    const shared = echo(3, 0, 0, [1, 2])
    const fog = fogOf({ opened: [open(1, 0, 0), open(2, 10, 0)], echoes: [shared, echo(4, 5, 5, [1])] })
    expect(fog.echoesOf.get(1)?.map(found => found.artistId)).toEqual([3, 4])
    expect(fog.echoesOf.get(2)?.map(found => found.artistId)).toEqual([3])
    expect(fog.echoesOf.get(99)).toBeUndefined()
  })
})
