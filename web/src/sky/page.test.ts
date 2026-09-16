import { describe, expect, it } from 'vitest'

import { readPage } from './location'

describe('telling a page apart from the map', () => {
  it('reads the charter', () => {
    expect(readPage('/charter', '')).toEqual({ kind: 'charter' })
  })

  it('reads a token out of a link from a letter', () => {
    expect(readPage('/confirm', '?token=abc123')).toEqual({ kind: 'confirm', token: 'abc123' })
    expect(readPage('/reset', '?token=abc123')).toEqual({ kind: 'reset', token: 'abc123' })
  })

  it('tells a missing token apart from a wrong one', () => {
    // A link cut short between the letter and the browser is not an expired
    // link, and saying so would send someone looking for a new letter that
    // will be cut the same way.
    expect(readPage('/confirm', '')).toEqual({ kind: 'confirm', token: '' })
    expect(readPage('/reset', '?token=')).toEqual({ kind: 'reset', token: '' })
  })

  it('survives a mail client adding its own parameters', () => {
    // Some of them append tracking or rewrite the order; the token is looked
    // up by name rather than by position for exactly that reason.
    expect(readPage('/reset', '?utm_source=mail&token=abc&x=1')).toEqual({ kind: 'reset', token: 'abc' })
  })

  it('takes a trailing slash, because a mail client may add one', () => {
    expect(readPage('/charter/', '')).toEqual({ kind: 'charter' })
    expect(readPage('/confirm/', '?token=abc')).toEqual({ kind: 'confirm', token: 'abc' })
  })

  it('reads an embedded star', () => {
    expect(readPage('/embed/star/54', '')).toEqual({ kind: 'embed', artistId: 54 })
  })

  it('will not embed something that is not a star', () => {
    // The id reader is shared with the map, so the two cannot come to
    // different conclusions about what a valid id is -- and an embed of
    // nothing renders the sky, which is the last thing a rectangle in
    // someone else's page should do.
    expect(readPage('/embed', '')).toBeNull()
    expect(readPage('/embed/star/0', '')).toBeNull()
    expect(readPage('/embed/star/abc', '')).toBeNull()
    expect(readPage('/embed/charter', '')).toBeNull()
  })

  it('leaves the map alone', () => {
    // Everything that is not one of the three is the sky, including a star
    // and the root -- a page that claimed those would replace the product.
    expect(readPage('/', '')).toBeNull()
    expect(readPage('/star/54', '')).toBeNull()
    expect(readPage('/charterer', '')).toBeNull()
    expect(readPage('/confirmation', '?token=abc')).toBeNull()
  })
})
