import { useCallback, useEffect, useRef, useState } from 'react'

import { SkyRenderer, type Camera, type Star } from './renderer'
import { fetchLevel, fetchSky, levelFor, type Sky as SkyMeta, type Tile } from './tiles'

/** What the sky is doing, for the caller to show around it. */
export interface SkyState {
  stars: number
  level: number
  scale: number
}

interface Props {
  onState?: (state: SkyState) => void
  onPick?: (star: Star | null) => void
}

/**
 * The sky itself: a canvas, a camera, and the star field.
 *
 * React owns the element and the events; the render loop is outside React
 * entirely. A component that re-rendered per frame would spend more time in
 * reconciliation than in drawing.
 */
export function Sky({ onState, onPick }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  // Mutable per-frame state, deliberately outside React.
  const camera = useRef<Camera>({ x: 0, y: 0, scale: 1 })
  const meta = useRef<SkyMeta | null>(null)
  const levels = useRef<Map<number, Tile>>(new Map())
  const shownLevel = useRef(-1)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const abort = new AbortController()

    let renderer: SkyRenderer
    try {
      renderer = new SkyRenderer(canvas)
    } catch (cause) {
      // Reported after the effect rather than during it: a browser without
      // WebGL2 is a state to show, not a render to cascade.
      const message = cause instanceof Error ? cause.message : 'the renderer could not start'
      queueMicrotask(() => {
        setError(message)
        setLoading(false)
      })
      return
    }

    // The device pixel ratio is clamped: fill rate is the risk in a
    // glow-heavy scene, and a 3x buffer triples it for no visible gain on
    // points this small (ADR 0003, confirmed in ADR 0009).
    const resize = () => {
      const ratio = Math.min(window.devicePixelRatio || 1, 1.5)
      canvas.width = Math.floor(canvas.clientWidth * ratio)
      canvas.height = Math.floor(canvas.clientHeight * ratio)
      renderer.resize(canvas.width, canvas.height)
    }
    resize()
    window.addEventListener('resize', resize)

    // Twinkle is motion; someone who asked for less of it gets none.
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)')

    let frame = 0
    let running = true

    const start = async () => {
      try {
        const sky = await fetchSky('/tiles', abort.signal)
        meta.current = sky
        camera.current = {
          x: (sky.min_x + sky.max_x) / 2,
          y: (sky.min_y + sky.max_y) / 2,
          scale: canvas.width / (sky.max_x - sky.min_x),
        }

        const level0 = await fetchLevel(0, '/tiles', abort.signal)
        levels.current.set(0, level0)
        setLoading(false)

        const loop = (now: number) => {
          if (!running) return
          const sky = meta.current
          if (sky) {
            const wanted = levelFor(sky, canvas.width / camera.current.scale)
            const tile = levels.current.get(wanted)
            if (tile && wanted !== shownLevel.current) {
              renderer.upload(tile.packed)
              shownLevel.current = wanted
            } else if (!tile) {
              // Fetched once; until it lands the previous level keeps drawing,
              // so zooming never shows an empty sky.
              levels.current.set(wanted, { packed: new Float32Array(0), stars: [] })
              void fetchLevel(wanted, '/tiles', abort.signal).then(loaded => levels.current.set(wanted, loaded))
            }

            renderer.draw(camera.current, [canvas.width, canvas.height], now * 0.001, reduceMotion.matches ? 0 : 1)
            onState?.({ stars: renderer.starCount, level: shownLevel.current, scale: camera.current.scale })
          }
          frame = requestAnimationFrame(loop)
        }
        frame = requestAnimationFrame(loop)
      } catch (cause) {
        if (abort.signal.aborted) return
        setError(cause instanceof Error ? cause.message : 'the sky could not be loaded')
        setLoading(false)
      }
    }
    void start()

    return () => {
      running = false
      abort.abort()
      cancelAnimationFrame(frame)
      window.removeEventListener('resize', resize)
    }
  }, [onState])

  // ------------------------------------------------------------- controls
  const dragging = useRef(false)
  const last = useRef({ x: 0, y: 0 })
  const moved = useRef(0)

  const onPointerDown = useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    dragging.current = true
    moved.current = 0
    last.current = { x: event.clientX, y: event.clientY }
    event.currentTarget.setPointerCapture(event.pointerId)
  }, [])

  const onPointerMove = useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!dragging.current) return
    const canvas = event.currentTarget
    const ratio = canvas.width / canvas.clientWidth
    const dx = event.clientX - last.current.x
    const dy = event.clientY - last.current.y
    moved.current += Math.abs(dx) + Math.abs(dy)
    camera.current.x -= (dx * ratio) / camera.current.scale
    camera.current.y += (dy * ratio) / camera.current.scale
    last.current = { x: event.clientX, y: event.clientY }
  }, [])

  const onPointerUp = useCallback(
    (event: React.PointerEvent<HTMLCanvasElement>) => {
      dragging.current = false
      event.currentTarget.releasePointerCapture(event.pointerId)
      // A drag is not a click. Without this every pan would also select
      // whatever star happened to be under the finger when it stopped.
      if (moved.current > 4 || !onPick) return

      const canvas = event.currentTarget
      const ratio = canvas.width / canvas.clientWidth
      const rect = canvas.getBoundingClientRect()
      const px = (event.clientX - rect.left) * ratio - canvas.width / 2
      const py = canvas.height / 2 - (event.clientY - rect.top) * ratio
      const world = {
        x: camera.current.x + px / camera.current.scale,
        y: camera.current.y + py / camera.current.scale,
      }

      const tile = levels.current.get(shownLevel.current)
      onPick(tile ? nearest(tile.stars, world, 24 / camera.current.scale) : null)
    },
    [onPick],
  )

  const onWheel = useCallback((event: React.WheelEvent<HTMLCanvasElement>) => {
    const sky = meta.current
    if (!sky) return
    const canvas = event.currentTarget
    const span = sky.max_x - sky.min_x
    const next = camera.current.scale * Math.exp(-event.deltaY * 0.001)
    // Bounded so the sky cannot be lost off-screen or zoomed past its detail.
    camera.current.scale = Math.max(canvas.width / span / 4, Math.min(next, 400))
  }, [])

  return (
    <div className="sky">
      <canvas
        ref={canvasRef}
        className="sky__canvas"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onWheel={onWheel}
      />
      {loading && !error && <p className="sky__notice">reading the sky…</p>}
      {error && <p className="sky__notice sky__notice--error">{error}</p>}
    </div>
  )
}

/** The closest star within `radius` world units, or null. */
function nearest(stars: Star[], at: { x: number; y: number }, radius: number): Star | null {
  let best: Star | null = null
  let bestDistance = radius * radius
  for (const star of stars) {
    const dx = star.x - at.x
    const dy = star.y - at.y
    const distance = dx * dx + dy * dy
    if (distance < bestDistance) {
      bestDistance = distance
      best = star
    }
  }
  return best
}
