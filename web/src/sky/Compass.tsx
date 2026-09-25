import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { panelVariants } from '@/components/ui/panel'
import type { Region } from '@/api'
import { fromMap, keyMove, toMap, viewRect } from './minimap'
import type { Bounds, Overview, View } from './Sky'
import type { Star } from './renderer'
import { fetchLevel } from './tiles'

interface Props {
  overview: Overview
  view: View
  visible: Bounds
  /** What the middle of the view is, asked once above for every reader. */
  region: Region | null
  /** The star the player is sounding, marked on the small sky. */
  sounding: { x: number; y: number } | null
  onNavigate: (view: View) => void
  className?: string
}

/** The minimap's side, in CSS pixels. */
const SIZE = 168

/** The pyramid level the minimap draws. */
const MAP_LEVEL = 2

/**
 * Where am I: the whole sky drawn small, the view as a rectangle on it, and the
 * name of the region the middle of the view is in.
 *
 * The minimap is also the keyboard's way of moving the big sky — arrows pan,
 * `+` and `-` zoom, Home shows everything — because the canvas itself has no
 * content a keyboard can land on. A pointer drags or clicks on it to fly.
 */
export function Compass({ overview, view, visible, region, sounding, onNavigate, className }: Props) {
  const { t } = useTranslation()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const dragging = useRef(false)
  const { sky } = overview

  // Drawn from a deeper level than the sky opens on. Level 0 is the bright
  // core only -- measured on the slice, its stars sit within 160 units of the
  // middle while the faint ones spread past 1,200 -- so a minimap of level 0
  // would show the sky's shape as a dot. Level 2 is a few tens of thousands of
  // points, drawn once, and the sky has usually fetched it already.
  const [stars, setStars] = useState<Star[]>(overview.stars)
  useEffect(() => {
    const abort = new AbortController()
    fetchLevel(sky, Math.min(MAP_LEVEL, sky.max_level), '/tiles', abort.signal).then(
      tile => setStars(tile.stars),
      () => undefined
    )
    return () => abort.abort()
  }, [sky])

  // The stars are drawn once into a bitmap of their own; each frame after
  // that is one copy and one rectangle.
  const field = useMemo(() => {
    const ratio = window.devicePixelRatio || 1
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = Math.round(SIZE * ratio)
    const context = canvas.getContext('2d')
    if (!context) return canvas
    context.scale(ratio, ratio)
    for (const star of stars) {
      const at = toMap(sky, SIZE, star.x, star.y)
      // Faint, because tens of thousands of points share a few thousand
      // pixels: at full strength the sky's shape saturates into a flat disc.
      context.globalAlpha = 0.05 + 0.6 * star.brightness
      context.fillStyle = star.brightness > 0.5 ? '#f2ead9' : '#6f9ee0'
      context.fillRect(at.x, at.y, 1, 1)
    }
    return canvas
  }, [sky, stars])

  useEffect(() => {
    const canvas = canvasRef.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    const ratio = window.devicePixelRatio || 1
    canvas.width = canvas.height = Math.round(SIZE * ratio)
    context.setTransform(1, 0, 0, 1, 0, 0)
    context.clearRect(0, 0, canvas.width, canvas.height)
    context.drawImage(field, 0, 0)
    context.scale(ratio, ratio)

    // The view, never smaller than a mark that can be seen: zoomed far in, the
    // true rectangle is under a pixel and the minimap would show nothing.
    const rect = viewRect(sky, SIZE, visible)
    const minimum = 6
    const width = Math.max(rect.width, minimum)
    const height = Math.max(rect.height, minimum)
    context.strokeStyle = 'rgba(140, 190, 255, 0.95)'
    context.lineWidth = 1.5
    context.strokeRect(rect.x + (rect.width - width) / 2, rect.y + (rect.height - height) / 2, width, height)
  }, [field, sky, visible])

  const where = [region?.style, region?.genre].filter(Boolean).join(' · ')
  const wholeScale = (visibleWidth(visible) * view.scale) / (sky.max_x - sky.min_x)

  const flyTo = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    const point = fromMap(sky, SIZE, { x: event.clientX - rect.left, y: event.clientY - rect.top })
    onNavigate({ ...point, scale: view.scale })
  }

  return (
    <section className={cn(panelVariants(), 'glass flex w-fit flex-col gap-1.5 p-2', className)} aria-labelledby="compass-heading">
      <h2 id="compass-heading" className="sr-only">
        {t('compass.heading')}
      </h2>
      {/* The sound's place on the small sky: where the music is coming from,
          which for the signal of the day is the only clue to where it is. A
          ping only where motion is welcome; a still ring otherwise. */}
      <div className="relative">
        <canvas
          ref={canvasRef}
          style={{ width: SIZE, height: SIZE }}
          className="block cursor-crosshair rounded-inner bg-[#07080d] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          tabIndex={0}
          role="application"
          aria-label={t('compass.label')}
          aria-describedby="compass-hint"
          onPointerDown={event => {
            dragging.current = true
            event.currentTarget.setPointerCapture(event.pointerId)
            flyTo(event)
          }}
          onPointerMove={event => {
            if (dragging.current) flyTo(event)
          }}
          onPointerUp={event => {
            dragging.current = false
            event.currentTarget.releasePointerCapture(event.pointerId)
          }}
          onKeyDown={event => {
            const next = keyMove(event.key, view, visible, sky, wholeScale)
            if (!next) return
            event.preventDefault()
            onNavigate(next)
          }}
        />
        {sounding && <Sounding at={toMap(sky, SIZE, sounding.x, sounding.y)} />}
      </div>
      <p id="compass-hint" className="sr-only">
        {t('compass.hint')}
      </p>
      {/* Spoken when it changes: the region is the answer to "where am I",
          and a reader panning by keyboard should hear it change. */}
      <p className="m-0 max-w-[10.5rem] truncate text-2xs text-dim" aria-live="polite">
        {where ? t('compass.in', { place: where }) : t('compass.nowhere')}
      </p>
    </section>
  )
}

/**
 * Two tones, like a pin on a map: the minimap is blue at its edges and white
 * at its core, and the accent -- tried first -- vanished into the blue. White
 * in a dark rim reads on both.
 */
function Sounding({ at }: { at: { x: number; y: number } }) {
  return (
    <span
      aria-hidden="true"
      className="pointer-events-none absolute size-3 -translate-x-1/2 -translate-y-1/2"
      style={{ left: at.x, top: at.y }}
    >
      <span className="absolute inset-0 rounded-full bg-white/80 motion-safe:animate-ping" />
      <span className="absolute inset-0 rounded-full border-2 border-black bg-white" />
    </span>
  )
}

function visibleWidth(visible: Bounds): number {
  return visible.maxX - visible.minX
}
