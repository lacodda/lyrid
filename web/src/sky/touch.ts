import type { Camera } from './renderer'

/** A point on the canvas, in device pixels from its middle, `y` upward. */
export interface Point {
  x: number
  y: number
}

/**
 * The camera after two fingers moved.
 *
 * The sky under the middle of the two fingers stays under it: the place they
 * held is the place they still hold, nearer or further as they spread or
 * close, and dragged along if both move together. That one rule is pan and
 * zoom at once, which is why there is no separate two-finger pan.
 *
 * `least` and `most` bound the scale as the wheel's zoom is bounded, so a
 * pinch cannot lose the sky off-screen or go past its detail either.
 */
export function pinch(camera: Camera, from: readonly [Point, Point], to: readonly [Point, Point], least: number, most: number): Camera {
  const before = spread(from)
  // Two fingers on one pixel have no distance to compare against.
  if (before === 0) return camera
  const held = middle(from)
  const world = { x: camera.x + held.x / camera.scale, y: camera.y + held.y / camera.scale }
  const scale = Math.max(least, Math.min((camera.scale * spread(to)) / before, most))
  const now = middle(to)
  return { x: world.x - now.x / scale, y: world.y - now.y / scale, scale }
}

function spread([a, b]: readonly [Point, Point]): number {
  return Math.hypot(a.x - b.x, a.y - b.y)
}

function middle([a, b]: readonly [Point, Point]): Point {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
}
