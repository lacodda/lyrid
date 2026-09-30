import { useEffect, useMemo, useState } from 'react'

import { toMap } from './minimap'
import type { Overview } from './Sky'
import type { Star } from './renderer'
import { fetchLevel } from './tiles'

/** The pyramid level the small sky is drawn from. */
const FIELD_LEVEL = 2

/**
 * The whole sky drawn small, as a bitmap: the backdrop of every minimap.
 *
 * Drawn from a deeper level than the sky opens on. Level 0 is the bright core
 * only -- measured on the slice, its stars sit within 160 units of the middle
 * while the faint ones spread past 1,200 -- so a minimap of level 0 would show
 * the sky's shape as a dot. Level 2 is a few tens of thousands of points,
 * drawn once, and the sky has usually fetched it already.
 *
 * Shared by the compass and the dossiers, so the sky a roster is placed on is
 * the same small sky the compass shows.
 */
export function useSkyField(overview: Overview, size: number): HTMLCanvasElement {
  const { sky } = overview
  const [stars, setStars] = useState<Star[]>(overview.stars)
  useEffect(() => {
    const abort = new AbortController()
    fetchLevel(sky, Math.min(FIELD_LEVEL, sky.max_level), '/tiles', abort.signal).then(
      tile => setStars(tile.stars),
      () => undefined
    )
    return () => abort.abort()
  }, [sky])

  return useMemo(() => {
    const ratio = window.devicePixelRatio || 1
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = Math.round(size * ratio)
    const context = canvas.getContext('2d')
    if (!context) return canvas
    context.scale(ratio, ratio)
    for (const star of stars) {
      const at = toMap(sky, size, star.x, star.y)
      // Faint, because tens of thousands of points share a few thousand
      // pixels: at full strength the sky's shape saturates into a flat disc.
      context.globalAlpha = 0.05 + 0.6 * star.brightness
      context.fillStyle = star.brightness > 0.5 ? '#f2ead9' : '#6f9ee0'
      context.fillRect(at.x, at.y, 1, 1)
    }
    return canvas
  }, [sky, stars, size])
}
