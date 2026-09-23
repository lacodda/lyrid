import { afterEach, describe, expect, it, vi } from 'vitest'

import { ERAS, ERA_COLOURS, UNDATED_WEIGHT, eraIndex, paletteUniform, timeWeight } from './decades'
import { fetchLabels, layerFor, placeLabels, type Label } from './labels'
import { fromMap, keyMove, middle, toMap } from './minimap'
import { addStop, removeStop, step } from './route'
import { MAX_ROUTE, readRoute, writeLocation } from './location'
import { fetchTile, stamped, type Sky } from './tiles'
import { reasons } from './why'

const sky: Sky = { min_x: -100, min_y: -100, max_x: 100, max_y: 100, max_level: 4, record_bytes: 20 }

describe('the era lens', () => {
  it('puts a year in its era, and early years in the first', () => {
    expect(eraIndex(1925)).toBe(0)
    expect(eraIndex(1959)).toBe(0)
    expect(eraIndex(1960)).toBe(1)
    expect(eraIndex(1979)).toBe(2)
    expect(eraIndex(2024)).toBe(ERAS.length - 1)
  })

  it('keeps an unknown year out of every era', () => {
    expect(eraIndex(0)).toBe(-1)
  })

  it('hands the shader one colour per era and one for undated', () => {
    // The shader's array length is written from the same list; a palette one
    // colour short would read garbage for the newest era.
    expect(paletteUniform().length).toBe((ERA_COLOURS.length + 1) * 3)
    expect(ERA_COLOURS.length).toBe(ERAS.length)
  })
})

describe('the time machine', () => {
  it('has no star before its act began', () => {
    expect(timeWeight(1991, 1990)).toBe(0)
  })

  it('lights a star up in its year, then settles', () => {
    const debut = timeWeight(1990, 1990)
    const later = timeWeight(1990, 2010)
    expect(debut).toBeGreaterThan(2)
    expect(later).toBeGreaterThan(1)
    expect(later).toBeLessThan(1.01)
  })

  it('draws an undated star faint in every year rather than hiding it', () => {
    expect(timeWeight(0, 1920)).toBe(UNDATED_WEIGHT)
    expect(timeWeight(0, 2020)).toBe(UNDATED_WEIGHT)
    expect(UNDATED_WEIGHT).toBeGreaterThan(0)
  })
})

describe('names on the sky', () => {
  const label = (name: string, kind: Label['kind'], x: number, y: number, members: number): Label => ({ name, kind, x, y, members })
  const camera = { x: 0, y: 0, scale: 2 }
  const viewport = { width: 400, height: 400 }
  const measure = (name: string) => name.length * 8

  it('writes genres from far away, styles closer in, and nothing closest', () => {
    expect(layerFor(sky, 200)).toBe('genre')
    expect(layerFor(sky, 40)).toBe('genre')
    expect(layerFor(sky, 20)).toBe('style')
    expect(layerFor(sky, 2)).toBe(null)
  })

  it('drops the smaller of two names that would overlap', () => {
    // "Rock" must survive "Garage Rock": a sky missing its best-known name
    // because a smaller one got there first is wrong in a way anyone sees.
    const placed = placeLabels(
      [label('Garage Rock', 'genre', 1, 0, 10), label('Rock', 'genre', 0, 0, 900)],
      'genre',
      camera,
      viewport,
      measure,
      16
    )
    expect(placed.map(p => p.label.name)).toEqual(['Rock'])
  })

  it('keeps names that do not touch', () => {
    const placed = placeLabels([label('Jazz', 'genre', -60, 0, 5), label('Rock', 'genre', 60, 0, 9)], 'genre', camera, viewport, measure, 16)
    expect(placed).toHaveLength(2)
  })

  it('writes no name cut by the edge of the screen', () => {
    // 95 world units at scale 2 is 390 px from centre: past the 200 px edge.
    const placed = placeLabels([label('Jazz', 'genre', 95, 0, 5)], 'genre', camera, viewport, measure, 16)
    expect(placed).toHaveLength(0)
  })

  it('writes only the layer asked for', () => {
    const placed = placeLabels([label('Techno', 'style', 0, 0, 5)], 'genre', camera, viewport, measure, 16)
    expect(placed).toHaveLength(0)
  })

  it('reads a sky with no names file as a sky without names', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response('<html>', { status: 200 }))))
    await expect(fetchLabels(sky)).resolves.toEqual([])
  })
})

describe('the minimap', () => {
  it('lands a click where it was made', () => {
    // The two projections must be exact inverses, or the camera flies
    // somewhere near the click rather than to it.
    for (const [x, y] of [
      [-100, 100],
      [0, 0],
      [37.5, -62.25],
    ] as const) {
      const back = fromMap(sky, 168, toMap(sky, 168, x, y))
      expect(back.x).toBeCloseTo(x, 6)
      expect(back.y).toBeCloseTo(y, 6)
    }
  })

  it('draws the top of the sky at the top of the map', () => {
    expect(toMap(sky, 100, 0, 100).y).toBe(0)
    expect(toMap(sky, 100, 0, -100).y).toBe(100)
  })

  it('moves the view by a share of itself, whatever the zoom', () => {
    const view = { x: 0, y: 0, scale: 4 }
    const visible = { minX: -10, minY: -5, maxX: 10, maxY: 5 }
    expect(keyMove('ArrowRight', view, visible, sky, 1)).toEqual({ x: 5, y: 0, scale: 4 })
    expect(keyMove('ArrowUp', view, visible, sky, 1)).toEqual({ x: 0, y: 2.5, scale: 4 })
    expect(keyMove('+', view, visible, sky, 1)?.scale).toBe(6)
    expect(keyMove('Home', view, visible, sky, 1)).toEqual({ x: 0, y: 0, scale: 1 })
  })

  it('asks where am I about the middle of the view, not its edges', () => {
    expect(middle({ minX: -8, minY: -4, maxX: 8, maxY: 4 })).toEqual({ minX: -4, minY: -2, maxX: 4, maxY: 2 })
  })

  it('leaves keys it does not use to the page', () => {
    expect(keyMove('Tab', { x: 0, y: 0, scale: 1 }, { minX: 0, minY: 0, maxX: 1, maxY: 1 }, sky, 1)).toBe(null)
  })
})

describe('a route', () => {
  it('grows at the end and may come back to a star', () => {
    expect(addStop([1, 2], 1)).toEqual([1, 2, 1])
  })

  it('does not add the stop it already ends on', () => {
    expect(addStop([1, 2], 2)).toEqual([1, 2])
  })

  it('stops growing at what an address can carry', () => {
    const full = Array.from({ length: MAX_ROUTE }, (_, i) => i + 1)
    expect(addStop(full, 999)).toHaveLength(MAX_ROUTE)
  })

  it('loses exactly the stop removed', () => {
    expect(removeStop([5, 6, 5], 2)).toEqual([5, 6])
  })

  it('ends at its end rather than wrapping round', () => {
    expect(step([1, 2, 3], 2, 1)).toBe(null)
    expect(step([1, 2, 3], 0, -1)).toBe(null)
    expect(step([1, 2, 3], 1, 1)).toBe(2)
  })

  it('reads from the address whole or not at all', () => {
    expect(readRoute('/route/54,962,120')).toEqual([54, 962, 120])
    expect(readRoute('/route/7,7/')).toEqual([7, 7])
    expect(readRoute('/route/54,x,120')).toBe(null)
    expect(readRoute('/route/54,,120')).toBe(null)
    expect(readRoute('/route/')).toBe(null)
    expect(readRoute('/star/54')).toBe(null)
  })

  it('owns the path while it is open, and keeps the camera in the fragment', () => {
    expect(writeLocation({ artistId: 962, view: { x: 1, y: 2, scale: 3 }, route: [54, 962] })).toBe('/route/54,962#1,2,3')
    expect(writeLocation({ artistId: 962, view: null, route: [] })).toBe('/star/962')
  })

  it('refuses a route longer than the bound', () => {
    const long = Array.from({ length: MAX_ROUTE + 1 }, (_, i) => i + 1).join(',')
    expect(readRoute(`/route/${long}`)).toBe(null)
  })
})

describe('tiles of format 2', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function tile(version: number, stars: [number, number, number, number, number][]): ArrayBuffer {
    const buffer = new ArrayBuffer(16 + stars.length * 20)
    const view = new DataView(buffer)
    'LYST'.split('').forEach((c, i) => view.setUint8(i, c.charCodeAt(0)))
    view.setUint16(4, version, true)
    view.setUint32(8, stars.length, true)
    stars.forEach(([id, x, y, brightness, year], i) => {
      const at = 16 + i * 20
      view.setInt32(at, id, true)
      view.setFloat32(at + 4, x, true)
      view.setFloat32(at + 8, y, true)
      view.setFloat32(at + 12, brightness, true)
      view.setInt16(at + 16, year, true)
    })
    return buffer
  }

  it('reads the year of every star', async () => {
    const body = tile(2, [
      [7, 1.5, -2, 0.5, 1969],
      [8, 3, 4, 0.25, 0],
    ])
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(body))))
    const read = await fetchTile(sky, 0, 0, 0)
    expect(read?.stars.map(s => [s.artistId, s.year])).toEqual([
      [7, 1969],
      [8, 0],
    ])
    expect(Array.from(read?.packed ?? [])).toEqual([1.5, -2, 0.5, 1969, 3, 4, 0.25, 0])
  })

  it('refuses a tile of the old format rather than drawing it wrong', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(tile(1, [[7, 1, 2, 0.5, 0]])))))
    await expect(fetchTile(sky, 0, 0, 0)).resolves.toBe(null)
  })

  it('asks for this cut of the sky and no other', () => {
    expect(stamped({ ...sky, stamp: 1727000000 }, '/tiles', '2/1/3.bin')).toBe('/tiles/2/1/3.bin?v=1727000000')
    expect(stamped(sky, '/tiles', '2/1/3.bin')).toBe('/tiles/2/1/3.bin')
  })
})

describe('why two stars are near', () => {
  const t = ((key: string, values?: Record<string, string>) => (values ? `${key}:${Object.values(values).join('|')}` : key)) as never

  it('says influence first, then what they share', () => {
    expect(reasons({ genres: ['Rock', 'Blues'], co_listening: 0.4, influence: 'shaped_by' }, t)).toEqual([
      'why.shapedBy',
      'why.genres:Rock, Blues',
    ])
  })

  it('falls back to co-listening only when nothing else is known', () => {
    expect(reasons({ genres: [], co_listening: 0.4, influence: null }, t)).toEqual(['why.coListening'])
    expect(reasons({ genres: ['Jazz'], co_listening: 0.4, influence: null }, t)).toEqual(['why.genres:Jazz'])
  })

  it('has nothing to say about a pair with no edge and nothing shared', () => {
    expect(reasons({ genres: [], co_listening: null, influence: null }, t)).toEqual([])
  })
})
