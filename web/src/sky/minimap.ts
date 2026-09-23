/**
 * The arithmetic of the minimap: the whole sky drawn small, and the keyboard's
 * way of moving the big one.
 *
 * Kept apart from the component because a minimap is two projections that must
 * be exact inverses — a click on the small sky has to land the camera where the
 * click was — and that is a property to test, not to eyeball.
 */

import type { Bounds, View } from './Sky'
import type { Sky } from './tiles'

/** A point in the minimap, in its own pixels from the top-left. */
export interface MapPoint {
  x: number
  y: number
}

/** Where a world point lands on a square minimap `size` pixels across. */
export function toMap(sky: Sky, size: number, x: number, y: number): MapPoint {
  const span = sky.max_x - sky.min_x
  return {
    x: ((x - sky.min_x) / span) * size,
    // The sky's y grows upward; a canvas's grows downward.
    y: ((sky.max_y - y) / span) * size,
  }
}

/** The world point under a minimap pixel: the inverse of `toMap`. */
export function fromMap(sky: Sky, size: number, point: MapPoint): { x: number; y: number } {
  const span = sky.max_x - sky.min_x
  return {
    x: sky.min_x + (point.x / size) * span,
    y: sky.max_y - (point.y / size) * span,
  }
}

/** The visible rectangle as a rectangle on the minimap. */
export function viewRect(sky: Sky, size: number, visible: Bounds): { x: number; y: number; width: number; height: number } {
  const topLeft = toMap(sky, size, visible.minX, visible.maxY)
  const bottomRight = toMap(sky, size, visible.maxX, visible.minY)
  return { x: topLeft.x, y: topLeft.y, width: bottomRight.x - topLeft.x, height: bottomRight.y - topLeft.y }
}

/**
 * The middle half of the view, which is what "where am I" asks about: the
 * edges of a wide view belong to the neighbouring regions as much as to this
 * one.
 */
export function middle(visible: Bounds): Bounds {
  const w = (visible.maxX - visible.minX) / 4
  const h = (visible.maxY - visible.minY) / 4
  return { minX: visible.minX + w, minY: visible.minY + h, maxX: visible.maxX - w, maxY: visible.maxY - h }
}

/** How far one arrow press moves the view: a quarter of what is on screen. */
export const STEP = 0.25
/** How much one press of + or - zooms. */
export const ZOOM = 1.5

/**
 * Where a key press on the minimap sends the camera, or `null` for a key it
 * does not use.
 *
 * Arrows pan by a share of the view rather than a fixed distance, so a press
 * means the same thing at every zoom; `+` and `-` zoom about the middle; Home
 * shows the whole sky. `wholeScale` is the scale at which the sky fills the
 * screen, which is what Home returns to.
 */
export function keyMove(key: string, view: View, visible: Bounds, sky: Sky, wholeScale: number): View | null {
  const dx = (visible.maxX - visible.minX) * STEP
  const dy = (visible.maxY - visible.minY) * STEP
  switch (key) {
    case 'ArrowLeft':
      return { ...view, x: view.x - dx }
    case 'ArrowRight':
      return { ...view, x: view.x + dx }
    case 'ArrowUp':
      return { ...view, y: view.y + dy }
    case 'ArrowDown':
      return { ...view, y: view.y - dy }
    case '+':
    case '=':
      return { ...view, scale: view.scale * ZOOM }
    case '-':
    case '_':
      return { ...view, scale: view.scale / ZOOM }
    case 'Home':
      return { x: (sky.min_x + sky.max_x) / 2, y: (sky.min_y + sky.max_y) / 2, scale: wholeScale }
    default:
      return null
  }
}
