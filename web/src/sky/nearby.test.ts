import { describe, expect, it } from 'vitest'

import { visibleBounds } from './Sky'
import { boundsKey } from './NearbyStars'

describe('the rectangle the canvas is showing', () => {
  it('puts the camera in the middle', () => {
    const bounds = visibleBounds({ x: 10, y: 20, scale: 1 }, 100, 40)
    expect(bounds).toEqual({ minX: -40, minY: 0, maxX: 60, maxY: 40 })
    // The camera is the centre of the rectangle, not a corner: a projection
    // that treated it as the origin would list the stars of the patch up and
    // to the right of the view rather than the one on screen.
    expect((bounds.minX + bounds.maxX) / 2).toBe(10)
    expect((bounds.minY + bounds.maxY) / 2).toBe(20)
  })

  it('shrinks as the view zooms in', () => {
    const wide = visibleBounds({ x: 0, y: 0, scale: 1 }, 200, 200)
    const close = visibleBounds({ x: 0, y: 0, scale: 10 }, 200, 200)
    expect(close.maxX - close.minX).toBeCloseTo((wide.maxX - wide.minX) / 10)
  })

  it('is as wide as the canvas is, not as tall', () => {
    // A square rectangle over a wide canvas would ask for the stars of a
    // square patch and list names that are off the left and right of the
    // screen -- which reads as the list being wrong rather than the projection.
    const bounds = visibleBounds({ x: 0, y: 0, scale: 2 }, 800, 200)
    expect(bounds.maxX - bounds.minX).toBe(400)
    expect(bounds.maxY - bounds.minY).toBe(100)
  })
})

describe('when a moved view is worth another request', () => {
  const view = { minX: 0, minY: 0, maxX: 1000, maxY: 1000 }

  it('gives the same key for a move too small to change a name', () => {
    // The camera moves on every frame of a drag; a key that changed with it
    // would spend a request per frame. A thousandth of the span changes no
    // name in a list of twelve.
    expect(boundsKey({ ...view, minX: 0.1, maxX: 1000.1 })).toBe(boundsKey(view))
  })

  it('gives a different key once the view has really moved', () => {
    expect(boundsKey({ ...view, minX: 50, maxX: 1050 })).not.toBe(boundsKey(view))
  })

  it('scales its step with the zoom', () => {
    // The same absolute movement is a different fraction of the view at every
    // zoom. A fixed grid would be too coarse zoomed in -- where a pan of one
    // unit crosses the whole screen -- and too fine zoomed out.
    const close = { minX: 0, minY: 0, maxX: 1, maxY: 1 }
    expect(boundsKey({ ...close, minX: 0.1, maxX: 1.1 })).not.toBe(boundsKey(close))
    // ...and that same 0.1 is nothing at all across a thousand units.
    expect(boundsKey({ ...view, minX: 0.1, maxX: 1000.1 })).toBe(boundsKey(view))
  })

  it('does not divide by zero on a canvas that has not been measured', () => {
    // The first frame can arrive with a zero-sized canvas; a step of `span /
    // 1000` would be zero, and every coordinate would round to NaN.
    const key = boundsKey({ minX: 5, minY: 5, maxX: 5, maxY: 5 })
    expect(key).not.toContain('NaN')
  })
})
