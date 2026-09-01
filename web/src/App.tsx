import { useCallback, useEffect, useRef, useState } from 'react'

import { Sky, type SkyState, type View } from '@/sky/Sky'
import { StarCard } from '@/sky/StarCard'
import { Search } from '@/sky/Search'
import { readLocation, writeLocation } from '@/sky/location'
import { HaloPicker, HALO_COLOURS, HALO_DEFAULT } from '@/sky/HaloPicker'
import { HALO_SHAPES, type HaloShape } from '@/sky/renderer'
import { fetchArtist } from '@/api'
import { AccountPanel } from '@/AccountPanel'
import { fetchMe, saveProfile, worthSaving, type Me } from '@/account'
import type { Star } from '@/sky/renderer'

/**
 * The sky fills the window; everything else floats over it.
 *
 * The map is the product, not a panel inside a page — so the shell is
 * deliberately thin: a mark, a search box, a card when a star is picked, and
 * a line of numbers for whoever wants to know what they are looking at.
 *
 * The address bar is part of that state: `/star/54` says which card is open
 * and `#x,y,scale` says where the camera is, so any view can be sent to
 * someone else. See `location.ts` for why the two live in different halves of
 * the URL.
 *
 * An account adds memory to all of that and takes nothing away: a visitor who
 * never signs in gets the same sky, with their marker remembered per browser
 * as it always was.
 */
export function App() {
  const [state, setState] = useState<SkyState | null>(null)
  const [picked, setPicked] = useState<Star | null>(null)
  const [target, setTarget] = useState<View | null>(null)
  const [me, setMe] = useState<Me | null>(null)

  // Read once, before anything renders: the opening view must reach the sky
  // on its first frame, not after a flight from somewhere else. A lazy
  // useState rather than a ref, because this value is read while rendering.
  const [opened] = useState(readLocation)

  // Stable callbacks: the render loop lives outside React, and a new function
  // every render would tear the canvas down and build it again.
  const onState = useCallback((next: SkyState) => setState(next), [])

  const onPick = useCallback((star: Star | null) => {
    setPicked(star)
    // A star picked on the map is already on screen; flying to it would yank
    // the view out from under the click.
  }, [])

  // A star chosen by name or arrived at by link has to be found first.
  const goTo = useCallback((star: Star) => {
    setPicked(star)
    setTarget({ x: star.x, y: star.y, scale: 8 })
  }, [])

  // A link to a star carries an id, not a position: the card knows where it
  // is, so the position is fetched and the camera flies there.
  useEffect(() => {
    const { artistId, view } = opened
    if (artistId === null) return
    const abort = new AbortController()
    fetchArtist(artistId, abort.signal)
      .then(artist => {
        const at = artist.position
        setPicked({ artistId, x: at?.x ?? 0, y: at?.y ?? 0, brightness: at?.brightness ?? 1 })
        // A fragment in the link wins: it says where the sender was looking,
        // which may be a wide view holding this star among others.
        if (!view && at) setTarget({ x: at.x, y: at.y, scale: 8 })
      })
      .catch(() => {
        // A link to a star that is not there opens the sky rather than an
        // error: the sky is still worth looking at.
      })
    return () => abort.abort()
  }, [opened])

  // How the marked star is drawn. Without an account this is remembered per
  // browser, as it always was; with one it follows the person to their next
  // machine. Reads are guarded because storage can be refused outright.
  const [shape, setShape] = useState<HaloShape>(() => remembered('lyrid.halo.shape', HALO_SHAPES[0], HALO_SHAPES))
  const [colour, setColour] = useState<[number, number, number]>(() => {
    const names = HALO_COLOURS.map(option => option.name)
    const name = remembered('lyrid.halo.colour', HALO_DEFAULT.name, names)
    return (HALO_COLOURS.find(option => option.name === name) ?? HALO_DEFAULT).rgb
  })

  // What the account already holds, so a save is only sent when something has
  // actually changed. A ref rather than state: nothing renders from it.
  const savedCamera = useRef<{ x: number; y: number; scale: number } | null>(null)

  // Whatever the profile says, applied to the sky. Called on sign-in and once
  // at startup for a session that is already open.
  const adopt = useCallback((profile: Me | null) => {
    setMe(profile)
    savedCamera.current = profile?.camera ?? null
    if (!profile) return

    if (profile.halo_shape && (HALO_SHAPES as readonly string[]).includes(profile.halo_shape)) {
      setShape(profile.halo_shape as HaloShape)
    }
    const named = HALO_COLOURS.find(option => option.name === profile.halo_colour)
    if (named) setColour(named.rgb)

    // The link wins over the saved camera. A fragment in the address was put
    // there by whoever sent it and says where to look; the saved camera is
    // only where this person happened to stop last time, and overriding a
    // shared view with it would make every link open somewhere else.
    if (profile.camera && !opened.view && opened.artistId === null) {
      setTarget(profile.camera)
    }
  }, [opened])

  // Who is signed in. Asked of the server rather than read from storage: the
  // session cookie is HttpOnly, so this page cannot see it, and a local flag
  // would only be a guess about a cookie that may have expired.
  useEffect(() => {
    const abort = new AbortController()
    fetchMe(abort.signal).then(adopt, () => {
      // A visitor is the normal case and not an error; a server that cannot
      // answer leaves the sky anonymous, which still works.
    })
    return () => abort.abort()
  }, [adopt])

  const chooseShape = useCallback(
    (next: HaloShape) => {
      setShape(next)
      remember('lyrid.halo.shape', next)
      if (me) void saveProfile({ halo_shape: next }).catch(() => undefined)
    },
    [me]
  )

  const chooseColour = useCallback(
    (next: [number, number, number]) => {
      setColour(next)
      const named = HALO_COLOURS.find(c => c.rgb[0] === next[0] && c.rgb[1] === next[1] && c.rgb[2] === next[2])
      if (!named) return
      remember('lyrid.halo.colour', named.name)
      if (me) void saveProfile({ halo_colour: named.name }).catch(() => undefined)
    },
    [me]
  )

  // Handed up by the sky once its loop is running; stable, so the sky's effect
  // does not restart on every render.
  const captureRef = useRef<(() => Promise<Blob | null>) | null>(null)
  const onCapture = useCallback((capture: () => Promise<Blob | null>) => {
    captureRef.current = capture
  }, [])

  // The address follows the view, but never adds to history: the camera moves
  // on every pan and zoom, and a Back button that steps through hundreds of
  // camera positions is worse than no Back button at all.
  useEffect(() => {
    if (!state) return
    const next = writeLocation({ artistId: picked?.artistId ?? null, view: state.view })
    if (next !== window.location.pathname + window.location.hash) {
      window.history.replaceState(null, '', next)
    }
  }, [state, picked])

  // Where the sky was left, saved for next time. The camera changes on every
  // frame, so this asks whether the view has really moved before spending a
  // request -- see `worthSaving`. The answer is compared against what the
  // account already holds rather than against the previous frame, or a slow
  // drift would never cross the threshold and never be saved at all.
  useEffect(() => {
    if (!me || !state) return
    const now = state.view
    if (!worthSaving(savedCamera.current, now)) return
    savedCamera.current = now
    void saveProfile({ camera: now }).catch(() => {
      // Nothing to say: the view is still on screen, and the next real move
      // tries again.
    })
  }, [me, state])

  return (
    <main className="app">
      <Sky
        onState={onState}
        onPick={onPick}
        target={target}
        initial={opened.view}
        marked={picked && { x: picked.x, y: picked.y, shape, colour }}
        onCapture={onCapture}
      />

      <header className="app__header">
        <img className="app__mark" src="/mark.svg" alt="" />
        <div>
          <h1 className="app__title">lyrid</h1>
          <p className="app__tagline">a music universe</p>
        </div>
      </header>

      <Search onPick={goTo} />

      {/* Keyed by the star: picking another one mounts a fresh card rather
          than leaving the previous artist on screen while the new one loads. */}
      {picked && <StarCard key={picked.artistId} artistId={picked.artistId} onClose={() => setPicked(null)} />}

      {state && (
        // One stack in the bottom-left, so nothing can land on top of
        // anything else as the pieces grow -- the defect v0.9.1 fixed, and
        // the reason the account panel joins the stack rather than claiming
        // a corner of its own. The top-right is the search box and the card.
        <div className="app__corner">
          <AccountPanel me={me} onSignedIn={adopt} onSignedOut={() => adopt(null)} />
          <HaloPicker shape={shape} colour={colour} onShape={chooseShape} onColour={chooseColour} />
          <Share capture={captureRef} />
          <p className="app__status">
            {state.stars.toLocaleString('en')} stars · level {state.level} · v{__APP_VERSION__}
          </p>
        </div>
      )}
    </main>
  )
}

/**
 * Taking the view with you: the link to it, or a picture of it.
 *
 * The address bar already holds the link, but nobody reads an address bar —
 * so the same string is offered as one press. The poster is the frame as
 * drawn, at the resolution it is drawn: what is on screen is what is saved.
 */
function Share({ capture }: { capture: { current: (() => Promise<Blob | null>) | null } }) {
  const [copied, setCopied] = useState(false)

  const copyLink = () => {
    const url = window.location.href
    void navigator.clipboard.writeText(url).then(
      () => {
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      },
      () => {
        // Clipboard access can be refused outright; the address bar still
        // holds the link, so there is nothing to apologise for.
      }
    )
  }

  const savePoster = () => {
    const take = capture.current
    if (!take) return
    void take().then(blob => {
      if (!blob) return
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = 'lyrid-sky.png'
      link.click()
      // Revoked once the browser has taken the data; leaving it would hold
      // the whole image in memory for the life of the page.
      URL.revokeObjectURL(url)
    })
  }

  return (
    <div className="app__share">
      <button onClick={copyLink}>{copied ? 'link copied' : 'copy link'}</button>
      <button onClick={savePoster}>save poster</button>
    </div>
  )
}

/**
 * A remembered choice, or the fallback.
 *
 * Storage can be refused outright — a private window, or a browser told to
 * block site data — and it throws rather than returning null when it is. The
 * value is checked against what the code knows about, so a stale or
 * hand-edited entry cannot put the renderer into a shape it has no branch for.
 */
function remembered<T extends string>(key: string, fallback: T, allowed: readonly T[]): T {
  try {
    const stored = window.localStorage.getItem(key)
    return allowed.includes(stored as T) ? (stored as T) : fallback
  } catch {
    return fallback
  }
}

function remember(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Nothing to do and nothing to say: the choice still holds for this visit.
  }
}
