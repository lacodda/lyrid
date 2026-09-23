import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { advance } from './flight'
import { layerFor, placeLabels, type Label } from './labels'
import { SkyRenderer, type Camera, type Halo, type Instruments, type Star } from './renderer'
import { fetchLevel, fetchSky, levelFor, neighbouringLevels, tileAction, type Sky as SkyMeta, type Tile } from './tiles'

/** The whole sky at a glance: its extent and its brightest stars. */
export interface Overview {
  sky: SkyMeta
  stars: Star[]
}

/** What the sky is doing, for the caller to show around it. */
export interface SkyState {
  stars: number
  level: number
  scale: number
  /** Where the camera is now, so the caller can put it in the address. */
  view: View
  /**
   * What the canvas is showing, in layout coordinates.
   *
   * Handed out rather than left to the caller to work out. The projection from
   * a camera and a scale to a rectangle needs the canvas size in device
   * pixels, which only this component has; a consumer computing it from `view`
   * alone would be guessing at the viewport, and would be wrong on every
   * display whose device pixel ratio is not one.
   */
  visible: Bounds
}

/** A rectangle of the layout. */
export interface Bounds {
  minX: number
  minY: number
  maxX: number
  maxY: number
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
  /** The era lens and the time machine. */
  instruments?: Instruments
  /** A route's stops, in order, drawn joined. */
  route?: readonly { x: number; y: number }[]
  /** Genre and style names to write on the sky. */
  labels?: readonly Label[]
  /**
   * Handed the sky's extent and its level-0 stars once they have loaded, for
   * whatever draws the sky small — the minimap. Handed over rather than
   * fetched twice: the level is already in memory here.
   */
  onOverview?: (overview: Overview) => void
}

/**
 * The sky itself: a canvas, a camera, and the star field.
 *
 * React owns the element and the events; the render loop is outside React
 * entirely. A component that re-rendered per frame would spend more time in
 * reconciliation than in drawing.
 */
export function Sky({ onState, onPick, target, initial, marked, onCapture, instruments, route, labels, onOverview }: Props) {
  const { t } = useTranslation()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  // Names are text, and text on a WebGL canvas means a glyph atlas; a 2D
  // canvas laid over it draws them with the browser's own fonts instead, in
  // the same frame, for a few dozen names.
  const namesRef = useRef<HTMLCanvasElement>(null)

  // `t` in a ref, not in the effect's dependencies. The effect below builds the
  // WebGL context and starts the render loop; restarting it because the
  // interface language changed would tear the canvas down and rebuild it, which
  // is a black flash and a lost camera for a word nobody is reading at that
  // moment. What the effect needs `t` for is the two failure messages, and
  // those are written when they happen, in whatever language is showing then.
  //
  // Kept current in an effect rather than assigned while rendering: a ref
  // written during render is read by the render that wrote it, which is the one
  // ordering React does not promise.
  const translate = useRef(t)
  useEffect(() => {
    translate.current = t
  }, [t])
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

  // The same arrangement for everything else the loop reads per frame.
  const instrumentsRef = useRef<Instruments>({ lens: false, until: null })
  useEffect(() => {
    instrumentsRef.current = instruments ?? { lens: false, until: null }
  }, [instruments])
  const labelsRef = useRef<readonly Label[]>([])
  useEffect(() => {
    labelsRef.current = labels ?? []
  }, [labels])
  // The route is uploaded when it changes, not per frame; the loop picks up a
  // pending one on its next frame, once the renderer exists.
  const pendingRoute = useRef<readonly { x: number; y: number }[] | null>(null)
  useEffect(() => {
    pendingRoute.current = route ?? []
  }, [route])
  const onOverviewRef = useRef(onOverview)
  useEffect(() => {
    onOverviewRef.current = onOverview
  }, [onOverview])

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
      const message = cause instanceof Error ? cause.message : translate.current('sky.rendererFailed')
      queueMicrotask(() => {
        setError(message)
        setLoading(false)
      })
      return
    }

    // The device pixel ratio is clamped: fill rate is the risk in a
    // glow-heavy scene, and a 3x buffer triples it for no visible gain on
    // points this small (ADR 0003, confirmed in ADR 0009).
    const names = namesRef.current
    const context = names?.getContext('2d') ?? null
    const resize = () => {
      const ratio = Math.min(window.devicePixelRatio || 1, 1.5)
      canvas.width = Math.floor(canvas.clientWidth * ratio)
      canvas.height = Math.floor(canvas.clientHeight * ratio)
      renderer.resize(canvas.width, canvas.height)
      // The names at the full device ratio, not the clamped one: the clamp is
      // about glow fill rate, and blurry text costs legibility for nothing.
      if (names) {
        const textRatio = window.devicePixelRatio || 1
        names.width = Math.floor(names.clientWidth * textRatio)
        names.height = Math.floor(names.clientHeight * textRatio)
      }
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

        const level0 = await fetchLevel(sky, 0, '/tiles', abort.signal)
        levels.current.set(0, level0)
        setLoading(false)
        onOverviewRef.current?.({ sky, stars: level0.stars })

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
              void fetchLevel(sky, wanted, '/tiles', abort.signal).then(
                loaded => levels.current.set(wanted, loaded),
                () => {
                  // Without this the placeholder stays forever and the level
                  // is never asked for again: one dropped connection would
                  // cost that zoom for the rest of the visit.
                  levels.current.delete(wanted)
                }
              )
            } else if (action.do === 'upload' && tile) {
              renderer.upload(tile.packed)
              shownLevel.current = wanted
              // The level is on screen and the loop has nothing to wait for:
              // a good moment to fetch the two levels a zoom could go to
              // next. Done here rather than every frame so it happens once
              // per level change, and the placeholder keeps it to one
              // request each.
              for (const near of neighbouringLevels(sky, wanted)) {
                if (levels.current.has(near)) continue
                levels.current.set(near, { packed: new Float32Array(0), stars: [] })
                void fetchLevel(sky, near, '/tiles', abort.signal).then(
                  loaded => levels.current.set(near, loaded),
                  () => {
                    // A prefetch that fails is not a failure: the level is
                    // fetched again, for real, if the person zooms to it. The
                    // placeholder is dropped so that request can happen.
                    levels.current.delete(near)
                  }
                )
              }
            }

            if (pendingRoute.current) {
              renderer.setRoute(pendingRoute.current)
              pendingRoute.current = null
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
              markedRef.current,
              instrumentsRef.current
            )
            if (names && context) {
              drawNames(context, names, sky, camera.current, canvas.width, labelsRef.current)
            }
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
              visible: visibleBounds(camera.current, canvas.width, canvas.height),
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
        setError(cause instanceof Error ? cause.message : translate.current('sky.failed'))
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
    <div className="absolute inset-0">
      <canvas
        ref={canvasRef}
        className="absolute inset-0 block size-full cursor-grab touch-none active:cursor-grabbing"
        // The canvas has no children and cannot be reached by keyboard, so it
        // says what it is and points at the list that can be. The list is the
        // keyboard path; this is the sentence that says so out loud rather
        // than leaving a screen reader with an unlabelled rectangle.
        role="img"
        aria-label={t('sky.label')}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onWheel={onWheel}
      />
      {/* Hidden from assistive technology: the names are the same words the
          compass says out loud, and a reader does not need them twice. */}
      <canvas ref={namesRef} aria-hidden="true" className="pointer-events-none absolute inset-0 block size-full" />
      {loading && !error && (
        <p className="pointer-events-none absolute inset-0 m-0 grid place-content-center text-dim">{t('sky.loading')}</p>
      )}
      {error && (
        <p role="alert" className="pointer-events-none absolute inset-0 mx-auto grid max-w-lg place-content-center px-8 text-center text-warn">
          {error}
        </p>
      )}
    </div>
  )
}

/**
 * The rectangle of the layout the canvas currently shows.
 *
 * The same projection the shader uses, in reverse: the camera is the middle of
 * the canvas, and a device pixel is `1 / scale` of a layout unit. `y` grows
 * upward here and downward in the canvas, which is why the vertical pair is
 * built the same way as the horizontal one rather than flipped.
 */
export function visibleBounds(camera: Camera, width: number, height: number): Bounds {
  const halfWidth = width / 2 / camera.scale
  const halfHeight = height / 2 / camera.scale
  return {
    minX: camera.x - halfWidth,
    minY: camera.y - halfHeight,
    maxX: camera.x + halfWidth,
    maxY: camera.y + halfHeight,
  }
}

/**
 * Writes the names that suit the view onto the overlay.
 *
 * The overlay is in its own device pixels, which may differ from the star
 * canvas's clamped ones, so the camera's scale is converted before placing.
 */
function drawNames(
  context: CanvasRenderingContext2D,
  names: HTMLCanvasElement,
  sky: SkyMeta,
  camera: Camera,
  starWidth: number,
  labels: readonly Label[]
): void {
  context.clearRect(0, 0, names.width, names.height)
  if (labels.length === 0 || starWidth === 0) return
  const layer = layerFor(sky, starWidth / camera.scale)
  if (!layer) return

  const ratio = names.width / starWidth
  const size = Math.round((layer === 'genre' ? 13 : 12) * (names.width / names.clientWidth))
  // Genres in small capitals and spaced out, as a map letters a region;
  // styles in italic, as it letters a feature inside one.
  context.font = layer === 'genre' ? `600 ${String(size)}px system-ui, sans-serif` : `italic 500 ${String(size)}px system-ui, sans-serif`
  context.letterSpacing = layer === 'genre' ? `${String(size * 0.18)}px` : '0px'
  context.textAlign = 'center'
  context.textBaseline = 'middle'

  const text = (label: Label) => (layer === 'genre' ? label.name.toUpperCase() : label.name)
  const placed = placeLabels(
    labels,
    layer,
    { x: camera.x, y: camera.y, scale: camera.scale * ratio },
    { width: names.width, height: names.height },
    name => context.measureText(layer === 'genre' ? name.toUpperCase() : name).width,
    size * 1.4
  )
  for (const { label, x, y } of placed) {
    // A dark outline first, so a name crossing a bright cluster stays
    // readable; then the fill in the interface's dim ink.
    context.lineWidth = size * 0.3
    context.strokeStyle = 'rgba(7, 8, 13, 0.85)'
    context.strokeText(text(label), x, y)
    context.fillStyle = layer === 'genre' ? 'rgba(214, 222, 240, 0.72)' : 'rgba(190, 204, 232, 0.8)'
    context.fillText(text(label), x, y)
  }
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
