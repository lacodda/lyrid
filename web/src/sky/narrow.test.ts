import { describe, expect, it } from 'vitest'

import { SHEET_FLOOR, SHEET_SHARE, aboveSheet, sheetHeight } from './narrow'
import { pinch, type Point } from './touch'

describe('a sheet on a narrow screen', () => {
  it('takes its share of the window when little sits above it', () => {
    expect(sheetHeight(844, 110)).toBe(Math.round(844 * SHEET_SHARE))
  })

  it('stops short of what is pinned to the top', () => {
    // The player under the search box ends at 370: the sheet may not reach it.
    const height = sheetHeight(844, 370)
    expect(height).toBeLessThan(844 * SHEET_SHARE)
    expect(844 - height).toBeGreaterThan(370)
  })

  it('keeps a height that can be read on a window too short for both', () => {
    expect(sheetHeight(480, 370)).toBe(SHEET_FLOOR)
  })

  it('is never taller than the window', () => {
    expect(sheetHeight(180, 0)).toBeLessThanOrEqual(180)
  })
})

describe('aiming a star above the sheet', () => {
  // A window 400 by 800 CSS pixels showing 400 by 800 layout units at scale 1.
  const now = { visible: { minX: -200, minY: -400, maxX: 200, maxY: 400 }, view: { x: 0, y: 0, scale: 1 } }

  /** Where a star lands on screen, in CSS pixels from the top. */
  const landsAt = (starY: number, cameraY: number, scale: number) => 400 - (starY - cameraY) * scale

  it('lands the star in the middle of the free strip', () => {
    const view = aboveSheet({ x: 10, y: 50 }, 1, now, 800, 100)
    const sheetTop = 800 - sheetHeight(800, 100)
    expect(view.x).toBe(10)
    expect(landsAt(50, view.y, 1)).toBeCloseTo((100 + sheetTop) / 2)
  })

  it('lands it there at another scale too', () => {
    // A flight zooms in: a pixel is an eighth of what it is now.
    const view = aboveSheet({ x: 0, y: 50 }, 8, now, 800, 100)
    const sheetTop = 800 - sheetHeight(800, 100)
    expect(view.scale).toBe(8)
    expect(landsAt(50, view.y, 8)).toBeCloseTo((100 + sheetTop) / 2)
  })

  it('leaves the star in the middle when there is no strip to aim for', () => {
    // The top pieces and the sheet meet: nowhere is better than the middle.
    expect(aboveSheet({ x: 3, y: 4 }, 2, now, 800, 480)).toEqual({ x: 3, y: 4, scale: 2 })
  })
})

describe('a pinch', () => {
  const camera = { x: 100, y: 50, scale: 2 }
  const pair = (a: Point, b: Point) => [a, b] as const
  const under = (cam: typeof camera, at: Point) => ({ x: cam.x + at.x / cam.scale, y: cam.y + at.y / cam.scale })

  it('zooms by how far the fingers spread', () => {
    const next = pinch(camera, pair({ x: -50, y: 0 }, { x: 50, y: 0 }), pair({ x: -100, y: 0 }, { x: 100, y: 0 }), 0.1, 400)
    expect(next.scale).toBe(4)
  })

  it('keeps the sky under the middle of the fingers under it', () => {
    const from = pair({ x: 20, y: 40 }, { x: 120, y: 40 })
    const to = pair({ x: -10, y: 90 }, { x: 190, y: 110 })
    const next = pinch(camera, from, to, 0.1, 400)
    const held = under(camera, { x: 70, y: 40 })
    const still = under(next, { x: 90, y: 100 })
    expect(still.x).toBeCloseTo(held.x)
    expect(still.y).toBeCloseTo(held.y)
  })

  it('pans when both fingers move together', () => {
    const next = pinch(camera, pair({ x: 0, y: 0 }, { x: 100, y: 0 }), pair({ x: 30, y: 0 }, { x: 130, y: 0 }), 0.1, 400)
    expect(next.scale).toBe(2)
    expect(next.x).toBe(85)
  })

  it('stops at the bounds the wheel stops at', () => {
    const out = pinch(camera, pair({ x: -100, y: 0 }, { x: 100, y: 0 }), pair({ x: -1, y: 0 }, { x: 1, y: 0 }), 0.5, 400)
    expect(out.scale).toBe(0.5)
    const far = pinch(camera, pair({ x: -1, y: 0 }, { x: 1, y: 0 }), pair({ x: -900, y: 0 }, { x: 900, y: 0 }), 0.5, 400)
    expect(far.scale).toBe(400)
  })

  it('ignores two fingers with no distance between them', () => {
    expect(pinch(camera, pair({ x: 5, y: 5 }, { x: 5, y: 5 }), pair({ x: 0, y: 0 }, { x: 9, y: 9 }), 0.1, 400)).toBe(camera)
  })
})
