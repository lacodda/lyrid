import { describe, expect, it } from 'vitest'

import { tileAction, type Tile } from './tiles'

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
