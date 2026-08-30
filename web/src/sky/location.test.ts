import { describe, expect, it } from 'vitest'

import { readArtistId, readView, writeLocation } from './location'

describe('readArtistId', () => {
  it('reads a star route', () => {
    expect(readArtistId('/star/54')).toBe(54)
    expect(readArtistId('/star/54/')).toBe(54)
  })

  it('is null for anything that is not one', () => {
    expect(readArtistId('/')).toBeNull()
    expect(readArtistId('/star')).toBeNull()
    expect(readArtistId('/star/abc')).toBeNull()
    // Ids are database keys, and zero is not one.
    expect(readArtistId('/star/0')).toBeNull()
    expect(readArtistId('/star/-1')).toBeNull()
    // Beyond what a number can hold exactly, an id would silently become a
    // different id.
    expect(readArtistId('/star/90071992547409911')).toBeNull()
  })
})

describe('readView', () => {
  it('reads a camera out of the fragment', () => {
    expect(readView('#-59.17,-69.55,12')).toEqual({ x: -59.17, y: -69.55, scale: 12 })
    expect(readView('-59.17,-69.55,12')).toEqual({ x: -59.17, y: -69.55, scale: 12 })
  })

  it('refuses a fragment it cannot trust', () => {
    // A truncated or hand-edited link must open the whole sky, not a camera
    // at NaN — which draws nothing and reads as a broken product.
    expect(readView('')).toBeNull()
    expect(readView('#1,2')).toBeNull()
    expect(readView('#1,2,3,4')).toBeNull()
    expect(readView('#a,b,c')).toBeNull()
    expect(readView('#1,,3')).toBeNull()
    // A scale of zero or less is not a camera; it would divide by zero in the
    // renderer's world-to-screen transform.
    expect(readView('#1,2,0')).toBeNull()
    expect(readView('#1,2,-5')).toBeNull()
  })
})

describe('writeLocation', () => {
  it('puts the star in the path and the camera in the fragment', () => {
    const url = writeLocation({ artistId: 54, view: { x: -59.17, y: -69.55, scale: 12 } })
    expect(url).toBe('/star/54#-59.17,-69.55,12')
  })

  it('drops what it does not have', () => {
    expect(writeLocation({ artistId: null, view: { x: 1, y: 2, scale: 3 } })).toBe('/#1,2,3')
    expect(writeLocation({ artistId: 54, view: null })).toBe('/star/54')
    expect(writeLocation({ artistId: null, view: null })).toBe('/')
  })

  it('rounds to what the eye can tell apart', () => {
    // A raw float doubles the length of a link people are meant to read.
    const url = writeLocation({ artistId: null, view: { x: 1.23456789, y: -9.87654321, scale: 12.3456789 } })
    expect(url).toBe('/#1.23,-9.88,12.3')
  })

  it('round-trips through the reader', () => {
    const view = { x: -59.17, y: -69.55, scale: 12.3 }
    const url = writeLocation({ artistId: 54, view })
    const [path, hash] = url.split('#')
    expect(readArtistId(path as string)).toBe(54)
    expect(readView(hash as string)).toEqual(view)
  })
})
