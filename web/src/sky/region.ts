import { useEffect, useState } from 'react'

import { fetchRegion, type Region } from '@/api'
import { middle } from './minimap'
import { boundsKey } from './NearbyStars'
import type { Bounds } from './Sky'

/**
 * Where the view is, in the canon's words: the commonest main style and genre
 * of the stars in the middle of it, and the radio of that place.
 *
 * One request for two readers — the compass says the name, the player offers
 * the radio — so it lives above both rather than being asked twice. Debounced
 * and keyed on a rounded view, the way the nearby list is: the answer is a vote
 * among stars in the database, and a pan is a hundred moves in a row.
 */
export function useRegion(visible: Bounds | null): Region | null {
  const [region, setRegion] = useState<Region | null>(null)
  const key = visible ? boundsKey(middle(visible)) : null

  useEffect(() => {
    if (key === null) return
    const abort = new AbortController()
    const [minX = 0, minY = 0, maxX = 0, maxY = 0] = key.split(',').map(Number)
    const timer = window.setTimeout(() => {
      fetchRegion({ minX, minY, maxX, maxY }, abort.signal).then(setRegion, () => undefined)
    }, 400)
    return () => {
      window.clearTimeout(timer)
      abort.abort()
    }
  }, [key])

  return region
}
