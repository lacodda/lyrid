import { useCallback, useEffect, useRef, useState } from 'react'

import { advance } from './flight'
import { SkyRenderer, type Camera, type Halo, type Star } from './renderer'
import { fetchLevel, fetchSky, levelFor, tileAction, type Sky as SkyMeta, type Tile } from './tiles'

/** What the sky is doing, for the caller to show around it. */
export interface SkyState {
  stars: number
  level: number
  scale: number
  /** Where the camera is now, so the caller can put it in the address. */
  view: View
}

/** Where the camera should be: a place in the sky and how close. */
export interface View {
  x: number
  y: number
  scale: number
}

interface Props {
  onState?: (state: SkyState) => void
  onPick?: (star: Star | null) => void
  /**
   * Somewhere to fly to. Changing this starts a flight; the camera is left
   * alone while it stays the same, so panning by hand is never fought.
   */
  target?: View | null
  /** The view to open on, instead of the whole sky. */
  initial?: View | null
  /** The star to mark with a halo — the one whose card is open. */
  marked?: Halo | null
  /**
   * Handed a function that captures the current frame as a PNG.
   *
   * The capture has to happen inside the render loop: without
   * `preserveDrawingBuffer` the colour buffer is undefined the moment the
   * frame ends, so a `toBlob` from outside would save an empty image. Turning
   * that flag on permanently would cost every frame for a button pressed
   * rarely, so instead one frame is drawn and read on the spot.
   */
  onCapture?: (capture: () => Promise<Blob | null>) => void
}

/**
 * The sky itself: a canvas, a camera, and the star field.
 *
 * React owns the element and the events; the render loop is outside React
 * entirely. A component that re-rendered per frame would spend more time in
 * reconciliation than in drawing.
 */
export function Sky({ onState, onPick, target, initial, marked, onCapture }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  // Mutable per-frame state, deliberately outside React.
  const camera = useRef<Camera>({ x: 0, y: 0, scale: 1 })
  const meta = useRef<SkyMeta | null>(null)
  const levels = useRef<Map<number, Tile>>(new Map())
  const shownLevel = useRef(-1)
  // Where the camera is flying, and nothing when it is still. Outside React
  // for the same reason as the camera: this is read every frame.
  const flight = useRef<View | null>(null)
  // The opening view is consumed once, when the sky's extent is known.
  const initialView = useRef<View | null>(initial ?? null)

  // Read every frame, so it lives outside React like the camera does. A prop
  // read directly in the loop would be captured at the effect's first run and
  // never change; the ref is updated in an effect rather than during render,
  // which is where a render-phase write would be a lie about purity.
  const markedRef = useRef<Halo | null>(null)
  useEffect(() => {
    markedRef.current = marked ?? null
  }, [marked])

  // A pending capture, resolved by the render loop on the next frame it draws.
  const pendingCapture = useRef<((blob: Blob | null) => void) | null>(null)

  // A flight starts when the target changes, not on every render: React
  // re-renders for reasons that have nothing to do with the camera, and
  // restarting the flight each time would drag the view back mid-pan.
  useEffect(() => {
    if (target) flight.current = target
  }, [target])

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
        camera.current = initialView.current ?? {
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
            const action = tileAction(tile, wanted, shownLevel.current)
            if (action.do === 'fetch') {
              // Marked as being fetched so the request is made once; until it
              // lands the previous level keeps drawing, so zooming never shows
              // an empty sky.
              levels.current.set(wanted, { packed: new Float32Array(0), stars: [] })
              void fetchLevel(wanted, '/tiles', abort.signal).then(loaded => levels.current.set(wanted, loaded))
            } else if (action.do === 'upload' && tile) {
              renderer.upload(tile.packed)
              shownLevel.current = wanted
            }

            const wantsCapture = pendingCapture.current
            if (flight.current && advance(camera.current, flight.current, reduceMotion.matches)) {
              // Cleared on arrival: a target left in place would fight the
              // next pan by hand.
              flight.current = null
            }
            renderer.draw(
              camera.current,
              [canvas.width, canvas.height],
              now * 0.001,
              reduceMotion.matches ? 0 : 1,
              markedRef.current
            )
            // Read while the frame is still in the colour buffer: after this
            // callback returns, the browser is free to discard it.
            if (wantsCapture) {
              pendingCapture.current = null
              canvas.toBlob(blob => wantsCapture(blob), 'image/png')
            }

            onState?.({
              stars: renderer.starCount,
              level: shownLevel.current,
              scale: camera.current.scale,
              view: { ...camera.current },
            })
          }
          frame = requestAnimationFrame(loop)
        }
        frame = requestAnimationFrame(loop)

        onCapture?.(
          () =>
            new Promise<Blob | null>(resolve => {
              pendingCapture.current = resolve
            })
        )
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
  }, [onState, onCapture])

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
