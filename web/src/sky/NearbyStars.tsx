import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { fetchNearby, type NearbyStar } from '@/api'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import { retrying } from '@/retry'
import { listInFog, type Fog } from './fog'
import type { Bounds } from './Sky'
import type { Place } from './renderer'

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
 *
 * In the fog the list answers from what the person already holds and the server is
 * not asked: the names in view are the stars they have opened, and the next steps
 * from those are echoes, listed without a name until they are reached.
 */

interface Props {
  /** What the canvas is showing. Changes on every frame the camera moves. */
  visible: Bounds
  /** Opening a star from the list does what clicking it on the canvas does. */
  onPick: (star: Place) => void
  /** This person's fog, in the mode that has one. */
  fog?: Fog | null
  className?: string
}

/** A line of the list, wherever it came from. */
interface Line {
  id: number
  x: number
  y: number
  name: string
  comment: string | null
}

export function NearbyStars({ visible, onPick, fog, className }: Props) {
  const { t } = useTranslation()
  // `failed` when the server could not be read even after asking again: the
  // list says so rather than calling the sky empty.
  const [nearby, setNearby] = useState<NearbyStar[] | 'failed'>([])
  // The first view is never empty -- it is the saved camera, a link, or the
  // whole sky -- so an empty first answer came from a server in the middle of
  // starting or of having its canon replaced, and is asked again. Later views
  // can be empty for real: someone zoomed into the dark between nebulae.
  const looked = useRef(false)

  // The view is quantised before it becomes a dependency. `visible` is a fresh
  // object on every animation frame, so depending on it directly would fire a
  // request per frame; depending on a rounded string fires one per meaningful
  // move. The same problem the camera-save solves with `worthSaving`, solved
  // the same way -- by asking whether the view has really changed rather than
  // whether the object has.
  const key = boundsKey(visible)

  const inFog = useMemo<Line[] | null>(
    () =>
      fog
        ? listInFog(fog, parseKey(key)).map(star => ({
            id: star.id,
            x: star.x,
            y: star.y,
            name: star.name ?? (star.echoOf === null ? t('fog.echo') : t('fog.echoOf', { name: star.echoOf })),
            comment: null,
          }))
        : null,
    [fog, key, t]
  )
  const failed = !inFog && nearby === 'failed'
  const stars: Line[] = inFog ?? (nearby === 'failed' ? [] : nearby)

  const asking = !fog
  useEffect(() => {
    if (!asking) return
    const abort = new AbortController()
    // Debounced, because a pan is a hundred meaningful moves in a row and only
    // the last one is a view somebody is looking at.
    const timer = window.setTimeout(() => {
      const first = !looked.current
      retrying(
        () => fetchNearby(parseKey(key), abort.signal),
        abort.signal,
        stars => first && stars.length === 0
      ).then(
        stars => {
          looked.current = true
          setNearby(stars)
        },
        () => {
          // Asked again and still not read. The canvas still shows the sky;
          // the list says it could not be read, and the next move asks again.
          if (!abort.signal.aborted) setNearby('failed')
        }
      )
    }, 400)

    return () => {
      window.clearTimeout(timer)
      abort.abort()
    }
  }, [key, asking])

  return (
    // `overflow-hidden`: this is the piece of the corner stack that yields,
    // and a panel squeezed below its own content must cut it off at its edge
    // rather than spill it over the panel underneath.
    <section className={cn(panelVariants(), 'glass flex w-56 flex-col gap-1 overflow-hidden p-2', className)} aria-labelledby="nearby-heading">
      {/* `text-dim` over dowel's own `text-faint`. SectionLabel is faint by
          design, which measures 3.16:1 on the glass -- AA wants 4.5:1. The
          primitive is not wrong: it is checked against the opaque surfaces the
          rest of the line puts it on, and this surface is deliberately not
          one of those. Overridden here rather than in the primitive, and
          filed as a wish so the system can decide what faint means on glass. */}
      <SectionLabel id="nearby-heading" className="text-dim">
        {t('sky.nearby.heading')}
      </SectionLabel>
      {/* Said once, to whoever reaches the list and wonders what it is for.
          Small rather than hidden: see the note at the top of this file.
          `dim`, not `faint`: measured at 3.16:1 over the glass, where AA wants
          4.5:1. `faint` is for decoration, and this is the sentence that
          explains the one feature on the screen that exists for readability --
          it has no business being the hardest thing here to read. */}
      <p className="m-0 text-2xs text-dim">{t('sky.nearby.hint')}</p>

      {stars.length === 0 ? (
        <p className="m-0 text-2xs text-dim" role={failed ? 'status' : undefined}>
          {failed ? t('sky.nearby.failed') : fog ? t('fog.nearbyEmpty') : t('sky.nearby.empty')}
        </p>
      ) : (
        <ul className="m-0 flex max-h-64 min-h-0 list-none flex-col overflow-y-auto p-0">
          {stars.map(star => (
            <li key={star.id}>
              <button
                type="button"
                className="block w-full cursor-pointer rounded-sm px-1.5 py-1 text-left text-xs text-text hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none"
                onClick={() => onPick({ artistId: star.id, x: star.x, y: star.y })}
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
