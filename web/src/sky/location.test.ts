import { describe, expect, it } from 'vitest'

import { readArtistId, readDossier, readView, writeLocation } from './location'

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

describe('readDossier', () => {
  it('reads a label by its Discogs id and a scene by its Wikidata item', () => {
    expect(readDossier('/label/1')).toEqual({ kind: 'label', id: 1 })
    expect(readDossier('/label/1/')).toEqual({ kind: 'label', id: 1 })
    expect(readDossier('/scene/Q18125')).toEqual({ kind: 'scene', qid: 18125 })
    expect(readDossier('/scene/q18125')).toEqual({ kind: 'scene', qid: 18125 })
    // A bare number can only mean the same item.
    expect(readDossier('/scene/18125')).toEqual({ kind: 'scene', qid: 18125 })
  })

  it('is null for anything that is not one', () => {
    expect(readDossier('/')).toBeNull()
    expect(readDossier('/star/54')).toBeNull()
    expect(readDossier('/label/motown')).toBeNull()
    expect(readDossier('/label/0')).toBeNull()
    expect(readDossier('/scene/Q')).toBeNull()
    expect(readDossier('/scene/P434')).toBeNull()
    expect(readDossier('/label/90071992547409911')).toBeNull()
  })
})

describe('writeLocation with a dossier', () => {
  it('gives the path to the dossier and keeps the camera', () => {
    const view = { x: 1, y: 2, scale: 3 }
    expect(writeLocation({ artistId: 54, view, route: [1, 2], dossier: { kind: 'label', id: 7 } })).toBe('/label/7#1,2,3')
    expect(writeLocation({ artistId: 54, view: null, dossier: { kind: 'scene', qid: 18125 } })).toBe('/scene/Q18125')
  })

  it('round-trips through the reader', () => {
    for (const dossier of [{ kind: 'label', id: 7 } as const, { kind: 'scene', qid: 18125 } as const]) {
      expect(readDossier(writeLocation({ artistId: null, view: null, dossier }))).toEqual(dossier)
    }
  })

  it('hands the path back when it closes', () => {
    expect(writeLocation({ artistId: 54, view: null, dossier: null })).toBe('/star/54')
  })
})
