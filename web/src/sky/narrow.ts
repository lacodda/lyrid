import { useSyncExternalStore } from 'react'

import type { Bounds, View } from './Sky'

/**
 * The narrow layout: what the sky's chrome does at a phone's width.
 *
 * On a wide window the chrome is two columns floating over the sky. A phone
 * has room for neither, so the card and the tools become sheets that rise
 * from the bottom edge, and the player sits under the search box where a
 * sheet cannot cover it (ADR 0031). This module holds the arithmetic of that
 * arrangement; the components only ask it.
 */

/**
 * The width below which the layout is the narrow one.
 *
 * The same boundary as Tailwind's `md`, written as the complement of it: the
 * classes that move the pieces and the code that decides which pieces there
 * are must change at the same pixel, or a window in between gets half of each.
 */
export const NARROW = '(width < 48rem)'

function subscribe(changed: () => void): () => void {
  const query = window.matchMedia(NARROW)
  query.addEventListener('change', changed)
  return () => query.removeEventListener('change', changed)
}

/** Whether the window is a phone's width now; follows a resize or a turn. */
export function useNarrow(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(NARROW).matches,
    () => false
  )
}

/** The share of the window's height a sheet may take when nothing is above it. */
export const SHEET_SHARE = 0.6

/** The least a sheet is given, in CSS pixels, however much sits above it. */
export const SHEET_FLOOR = 224

/** The gap kept between a sheet and what sits above it, in CSS pixels. */
const GAP = 8

/**
 * How tall a sheet may be, in CSS pixels.
 *
 * `ceiling` is where the pieces pinned to the top of the window end -- the
 * search box, and under it the player. A sheet stops short of them rather than
 * rising over them: the player is a video, and YouTube's player has to stay in
 * view while it plays. The floor wins only on a window too short for both,
 * where a sheet nobody can read would be worse than one that overlaps.
 */
export function sheetHeight(height: number, ceiling: number): number {
  const room = height - ceiling - GAP
  return Math.round(Math.max(Math.min(SHEET_FLOOR, height), Math.min(height * SHEET_SHARE, room)))
}

/** The least strip of sky worth aiming a star into, in CSS pixels. */
const WORTH_AIMING = 48

/**
 * The camera that shows a star in the sky a sheet leaves free.
 *
 * A flight ends with its star in the middle of the window, and on a phone the
 * middle of the window is under the card that the same press opened. So the
 * camera is aimed lower by as much as the free strip's middle is above the
 * window's: the star lands between the top pieces and the sheet. With no strip
 * to speak of -- the player and the card filling the screen between them --
 * the star goes to the middle as it always did, since nowhere is better.
 *
 * `visible` and `view` are the sky as it is now; they give the size of a CSS
 * pixel in layout units, which the target's own scale then corrects.
 */
export function aboveSheet(
  star: { x: number; y: number },
  scale: number,
  now: { visible: Bounds; view: View },
  height: number,
  ceiling: number
): View {
  const sheetTop = height - sheetHeight(height, ceiling)
  if (height <= 0 || sheetTop - ceiling < WORTH_AIMING) return { x: star.x, y: star.y, scale }
  const perPixel = ((now.visible.maxY - now.visible.minY) / height) * (now.view.scale / scale)
  const middle = (ceiling + sheetTop) / 2
  // Layout `y` grows upward: a star above the middle of the window is one the
  // camera sits below.
  return { x: star.x, y: star.y - (height / 2 - middle) * perPixel, scale }
}
