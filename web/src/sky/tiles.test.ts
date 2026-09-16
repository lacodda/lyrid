import { describe, expect, it } from 'vitest'

import { neighbouringLevels, tileAction, type Sky, type Tile } from './tiles'

const placeholder: Tile = { packed: new Float32Array(0), stars: [] }
const loaded: Tile = { packed: new Float32Array([1, 2, 3, 4]), stars: [] }

describe('tileAction', () => {
  it('fetches a level it has never seen', () => {
    expect(tileAction(undefined, 4, 0)).toEqual({ do: 'fetch' })
  })

  it('never uploads the placeholder', () => {
    // This is the whole reason the rule is a function. Uploading the empty
    // array drew nothing *and* recorded level 4 as shown, so the real tiles
    // arriving a moment later found the level already current and could never
    // replace them: an empty sky that no amount of zooming repaired.
    expect(tileAction(placeholder, 4, 0)).toEqual({ do: 'wait' })
  })

  it('uploads the tiles once they land', () => {
    expect(tileAction(loaded, 4, 0)).toEqual({ do: 'upload' })
  })

  it('leaves the level alone once it is shown', () => {
    // Re-uploading the same buffer every frame would be pure waste.
    expect(tileAction(loaded, 4, 4)).toEqual({ do: 'wait' })
  })

  it('does not fetch the same level twice', () => {
    // The placeholder is what marks a level as in flight; asking again while
    // it is there would start a second request for every frame until the
    // first one lands.
    expect(tileAction(placeholder, 4, 4).do).not.toBe('fetch')
    expect(tileAction(placeholder, 4, 0).do).not.toBe('fetch')
  })
})

describe('which levels are worth having ready', () => {
  const sky = (max_level: number): Sky => ({ min_x: -1, min_y: -1, max_x: 1, max_y: 1, max_level, record_bytes: 16 })

  it('reaches for the level on either side of this one', () => {
    // Zoom is the one move whose next step is predictable: from 3 you go to 2
    // or to 4, never to 7.
    expect(neighbouringLevels(sky(6), 3)).toEqual([2, 4])
  })

  it('does not reach past the top of the pyramid', () => {
    // There is no level 7 to fetch; asking for one would be a request that
    // can only 404, on every level change at full zoom.
    expect(neighbouringLevels(sky(6), 6)).toEqual([5])
  })

  it('does not reach below level zero', () => {
    expect(neighbouringLevels(sky(6), 0)).toEqual([1])
  })

  it('has nothing to prefetch in a sky of one level', () => {
    expect(neighbouringLevels(sky(0), 0)).toEqual([])
  })

  it('stays at one step, not two', () => {
    // Two steps out is four more levels for a move needing two deliberate
    // gestures: bandwidth spent on a guess rather than on a likelihood.
    expect(neighbouringLevels(sky(9), 5)).not.toContain(3)
    expect(neighbouringLevels(sky(9), 5)).not.toContain(7)
  })
})
