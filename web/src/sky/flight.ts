/**
 * Moving the camera towards somewhere, one frame at a time.
 *
 * Separated from the component because this is arithmetic, not rendering: it
 * is where a wrong easing or a missing stop condition hides, and neither is
 * visible by looking at the sky for a moment.
 */

import type { Camera } from './renderer'
import type { View } from './Sky'

/** How much of the remaining distance is covered each frame. */
const EASE = 0.12

/**
 * Advances `camera` towards `to`, returning true once it has arrived.
 *
 * Position eases linearly, but **scale moves geometrically** — zoom is felt as
 * a ratio, not a distance, so interpolating it linearly crawls at the start and
 * lurches at the end. Multiplying by a constant fraction of the remaining ratio
 * each frame reads as a steady approach.
 *
 * With `instant`, the camera is taken there rather than flown: someone who
 * asked for reduced motion should not be moved across the sky.
 */
export function advance(camera: Camera, to: View, instant: boolean): boolean {
  if (instant) {
    arrive(camera, to)
    return true
  }

  camera.x += (to.x - camera.x) * EASE
  camera.y += (to.y - camera.y) * EASE
  camera.scale *= (to.scale / camera.scale) ** EASE

  // "Arrived" measured in what the eye can see: less than half a pixel of
  // travel on screen, and a zoom within a tenth of a percent. Without this the
  // camera would approach forever and never release the flight.
  const pixels = Math.hypot(to.x - camera.x, to.y - camera.y) * camera.scale
  const zoomRatio = Math.abs(Math.log(to.scale / camera.scale))
  if (pixels < 0.5 && zoomRatio < 0.001) {
    arrive(camera, to)
    return true
  }
  return false
}

function arrive(camera: Camera, to: View): void {
  camera.x = to.x
  camera.y = to.y
  camera.scale = to.scale
}
