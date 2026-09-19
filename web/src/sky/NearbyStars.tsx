import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { fetchNearby, type NearbyStar } from '@/api'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import type { Bounds } from './Sky'
import type { Star } from './renderer'

/**
 * The sky, as a list.
 *
 * A WebGL canvas is one element with no children. To a screen reader it is a
 * blank rectangle, and to a keyboard it is nothing at all: every star in the
 * canon is reachable only by aiming a pointer at a few pixels of light. That is
 * not a detail of the rendering — it means the product has no keyboard path to
 * its own content.
 *
 * So the names in view are also rendered as text, beside the canvas rather than
 * hidden from sight. Two reasons it is visible rather than a screen-reader-only
 * block: a list nobody can see is a list nobody notices has gone stale, and
 * "what am I actually looking at" is a question a sighted reader of a star
 * field asks too — the canvas draws points of light, and points of light have
 * no names on them at any zoom.
 *
 * The list is not the canvas's equal, and does not pretend to be: it carries
 * the twelve most prominent names in view, and moving the view is how you get
 * different ones. A complete alternative to a two-million-star map is another
 * map, not a list.
 */

interface Props {
  /** What the canvas is showing. Changes on every frame the camera moves. */
  visible: Bounds
  /** Opening a star from the list does what clicking it on the canvas does. */
  onPick: (star: Star) => void
  className?: string
}

export function NearbyStars({ visible, onPick, className }: Props) {
  const { t } = useTranslation()
  const [stars, setStars] = useState<NearbyStar[]>([])

  // The view is quantised before it becomes a dependency. `visible` is a fresh
  // object on every animation frame, so depending on it directly would fire a
  // request per frame; depending on a rounded string fires one per meaningful
  // move. The same problem the camera-save solves with `worthSaving`, solved
  // the same way -- by asking whether the view has really changed rather than
  // whether the object has.
  const key = boundsKey(visible)

  useEffect(() => {
    const abort = new AbortController()
    // Debounced, because a pan is a hundred meaningful moves in a row and only
    // the last one is a view somebody is looking at.
    const timer = window.setTimeout(() => {
      fetchNearby(parseKey(key), abort.signal)
        .then(setStars)
        .catch(() => {
          // The canvas still shows the sky, so there is nothing to apologise
          // for; the next move asks again.
        })
    }, 400)

    return () => {
      window.clearTimeout(timer)
      abort.abort()
    }
  }, [key])

  return (
    <section className={cn(panelVariants(), 'glass flex w-56 flex-col gap-1 p-2', className)} aria-labelledby="nearby-heading">
      <SectionLabel id="nearby-heading">{t('sky.nearby.heading')}</SectionLabel>
      {/* Said once, to whoever reaches the list and wonders what it is for.
          Small rather than hidden: see the note at the top of this file. */}
      <p className="m-0 text-2xs text-faint">{t('sky.nearby.hint')}</p>

      {stars.length === 0 ? (
        <p className="m-0 text-2xs text-dim">{t('sky.nearby.empty')}</p>
      ) : (
        <ul className="m-0 flex max-h-64 min-h-0 list-none flex-col overflow-y-auto p-0">
          {stars.map(star => (
            <li key={star.id}>
              <button
                type="button"
                className="block w-full cursor-pointer rounded-sm px-1.5 py-1 text-left text-xs text-text hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none"
                onClick={() => onPick({ artistId: star.id, x: star.x, y: star.y, brightness: 1 })}
              >
                {star.name}
                {star.comment && <span className="block text-2xs text-dim">{star.comment}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

/**
 * The view as a string, rounded to the grid a request is worth making on.
 *
 * Four significant figures: a pan of less than a thousandth of the visible
 * width changes no name in the list, and asking again for it would spend a
 * request to receive the same twelve rows.
 */
export function boundsKey(bounds: Bounds): string {
  const span = Math.max(bounds.maxX - bounds.minX, bounds.maxY - bounds.minY)
  // A step derived from the span rather than fixed: the same absolute movement
  // is a different fraction of the view at every zoom, and a fixed grid would
  // be either too coarse when zoomed in or too fine when zoomed out.
  const step = span / 1000 || 1
  const snap = (value: number) => Math.round(value / step) * step
  return [snap(bounds.minX), snap(bounds.minY), snap(bounds.maxX), snap(bounds.maxY)].map(String).join(',')
}

function parseKey(key: string): Bounds {
  const [minX = 0, minY = 0, maxX = 0, maxY = 0] = key.split(',').map(Number)
  return { minX, minY, maxX, maxY }
}
