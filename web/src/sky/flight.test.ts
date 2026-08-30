import { describe, expect, it } from 'vitest'

import { advance } from './flight'

const target = { x: 100, y: -50, scale: 8 }

describe('advance', () => {
  it('arrives, and says so', () => {
    const camera = { x: 0, y: 0, scale: 1 }
    let frames = 0
    while (!advance(camera, target, false)) {
      frames += 1
      // A flight that never converges would hang here rather than fail
      // somewhere later, which is the point of the assertion below.
      expect(frames).toBeLessThan(600)
    }
    expect(camera).toEqual(target)
    // Roughly a second at 60 fps: long enough to follow with the eye, short
    // enough not to feel like waiting.
    expect(frames).toBeGreaterThan(20)
    expect(frames).toBeLessThan(120)
  })

  it('takes reduced motion straight there', () => {
    const camera = { x: 0, y: 0, scale: 1 }
    expect(advance(camera, target, true)).toBe(true)
    expect(camera).toEqual(target)
  })

  it('moves scale geometrically, not linearly', () => {
    // Zooming from 1 to 100 linearly would cover 12 units in the first frame
    // and crawl for the rest; a ratio covers a constant fraction of the
    // remaining zoom, which is what reads as a steady approach.
    const camera = { x: 0, y: 0, scale: 1 }
    advance(camera, { x: 0, y: 0, scale: 100 }, false)
    expect(camera.scale).toBeCloseTo(100 ** 0.12, 5)
    expect(camera.scale).toBeLessThan(2)
  })

  it('zooms out as readily as it zooms in', () => {
    // The same ratio in the other direction: a flight from close to far must
    // converge too, and linear easing on scale would overshoot past zero.
    const camera = { x: 0, y: 0, scale: 100 }
    let frames = 0
    while (!advance(camera, { x: 0, y: 0, scale: 1 }, false)) {
      frames += 1
      expect(frames).toBeLessThan(600)
    }
    expect(camera.scale).toBe(1)
  })

  it('is already there when it starts there', () => {
    const camera = { ...target }
    expect(advance(camera, target, false)).toBe(true)
    expect(camera).toEqual(target)
  })
})
