import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Sky, type Overview, type SkyState, type View } from '@/sky/Sky'
import { NearbyStars } from '@/sky/NearbyStars'
import { Instruments } from '@/sky/Instruments'
import { Compass } from '@/sky/Compass'
import type { Side } from '@/sky/Compare'
import { RoutePanel } from '@/sky/RoutePanel'
import { addStop, removeStop } from '@/sky/route'
import { fetchLabels, type Label } from '@/sky/labels'
import { LanguagePicker } from '@/LanguagePicker'
import { Button } from '@/components/ui/button'
import { StarCard } from '@/sky/StarCard'
import { Search } from '@/sky/Search'
import { readLocation, readPage, writeLocation, type Page } from '@/sky/location'
import { HaloPicker, HALO_COLOURS, HALO_DEFAULT } from '@/sky/HaloPicker'
import { HALO_SHAPES, type HaloShape, type Instruments as InstrumentSettings } from '@/sky/renderer'
import { fetchArtist, fetchStars, type Hit } from '@/api'
import { AccountPanel } from '@/AccountPanel'
import { fetchMe, saveProfile, worthSaving, type Me } from '@/account'
import { Charter } from '@/Charter'
import { Embed } from '@/Embed'
import { ConfirmPage, ResetPage } from '@/LetterPage'
import { count } from '@/metrics'
import { useLanguage } from '@/lib/language'
import type { Place, Star } from '@/sky/renderer'

// The spectrograph is opened on purpose and rarely, so it is fetched when it
// is, rather than weighing on every visit's first load.
const Compare = lazy(() => import('@/sky/Compare').then(module => ({ default: module.Compare })))

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
 * as it always was. That is the whole of the public preview — there is no
 * anonymous mode to build, because nothing here was ever behind the account.
 *
 * Three addresses are not the map: `/charter`, and the two a link in a letter
 * lands on. They render instead of the sky rather than over it, so a person
 * arriving from their mail client is not made to wait for a WebGL canvas they
 * did not come for.
 */
export function App() {
  const { t } = useTranslation()
  // The language the numbers are formatted in, which is the resolved locale and
  // not the stored choice: `system` is a choice, not a language a formatter
  // understands.
  const { resolved } = useLanguage()
  const [state, setState] = useState<SkyState | null>(null)
  const [picked, setPicked] = useState<Place | null>(null)
  const [target, setTarget] = useState<View | null>(null)
  const [me, setMe] = useState<Me | null>(null)

  // Read once, before anything renders: the opening view must reach the sky
  // on its first frame, not after a flight from somewhere else. A lazy
  // useState rather than a ref, because this value is read while rendering.
  const [opened] = useState(readLocation)

  // Which standalone page is showing, if any. Seeded from the address so a
  // link in a letter lands on the right one, and closable back to the sky.
  const [page, setPage] = useState<Page>(() => readPage(window.location.pathname, window.location.search))

  // Leaving a page puts the address back on the map, so the next thing that
  // writes the location does not fight with a stale `/confirm?token=...`.
  const leavePage = useCallback(() => {
    setPage(null)
    window.history.replaceState(null, '', '/')
  }, [])

  const openCharter = useCallback(() => {
    count('charter_read')
    setPage({ kind: 'charter' })
    window.history.replaceState(null, '', '/charter')
  }, [])

  // Stable callbacks: the render loop lives outside React, and a new function
  // every render would tear the canvas down and build it again.
  const onState = useCallback((next: SkyState) => setState(next), [])

  const onPick = useCallback((star: Star | null) => {
    setPicked(star)
    if (star) count('card_opened')
    // A star picked on the map is already on screen; flying to it would yank
    // the view out from under the click.
  }, [])

  // A star chosen by name or arrived at by link has to be found first.
  const goTo = useCallback((star: Place) => {
    setPicked(star)
    count('card_opened')
    setTarget({ x: star.x, y: star.y, scale: 8 })
  }, [])

  // A star named somewhere without its place — a neighbour on a card, a
  // shared neighbour in a comparison — is looked up and then flown to. A star
  // the layout left out has nowhere to fly, so nothing happens rather than a
  // flight to the origin.
  const openById = useCallback(
    (id: number) => {
      void fetchStars([id]).then(
        ([hit]) => {
          if (hit?.x != null && hit.y != null) goTo({ artistId: hit.id, x: hit.x, y: hit.y })
        },
        () => undefined
      )
    },
    [goTo]
  )

  // ---------------------------------------------------------- instruments
  const [instruments, setInstruments] = useState<InstrumentSettings>({ lens: false, until: null })

  // The whole sky small, for the minimap, and the names written on it. Both
  // arrive once the sky has loaded, so the names are asked for in the same
  // cut the tiles came from.
  const [overview, setOverview] = useState<Overview | null>(null)
  const [labels, setLabels] = useState<Label[]>([])
  const onOverview = useCallback((next: Overview) => {
    setOverview(next)
    void fetchLabels(next.sky).then(setLabels, () => undefined)
  }, [])

  // ------------------------------------------------------------ comparison
  // A star held up for comparison waits here until a second one is opened.
  const [pinned, setPinned] = useState<Side | null>(null)
  const [comparing, setComparing] = useState<[Side, Side] | null>(null)

  // ----------------------------------------------------------------- route
  const [route, setRoute] = useState<number[]>(() => opened.route ?? [])
  const [stops, setStops] = useState<Hit[]>([])
  const [routeAt, setRouteAt] = useState<number | null>(null)

  // Arriving by a route link opens its first stop, once — the link is a walk,
  // and a walk starts somewhere. Done where the stops land rather than in an
  // effect watching them, so it happens exactly when they are known.
  const routeOpened = useRef(false)

  // The names and places of the stops, asked for in one request whenever the
  // route changes. Ids the canon does not know drop out of the list — one
  // stale stop in a link someone sent should not erase the rest of the walk.
  useEffect(() => {
    if (route.length === 0) return
    const abort = new AbortController()
    fetchStars(route, abort.signal).then(hits => {
      setStops(hits)
      if (routeOpened.current || !opened.route) return
      routeOpened.current = true
      count('route_opened')
      const first = hits[0]
      if (opened.view || first?.x == null || first.y == null) return
      setRouteAt(0)
      goTo({ artistId: first.id, x: first.x, y: first.y })
    }, () => undefined)
    return () => abort.abort()
  }, [route, opened, goTo])

  // An empty route shows no stops, whatever the last answer held: derived
  // here rather than cleared in the effect, so there is one moment it changes.
  const shownStops = useMemo(() => (route.length === 0 ? [] : stops), [route, stops])

  // Placed stops only: a line can only be drawn through stars that have a
  // place on this sky.
  const routeLine = useMemo(
    () => shownStops.filter(stop => stop.x != null && stop.y != null).map(stop => ({ x: stop.x ?? 0, y: stop.y ?? 0 })),
    [shownStops]
  )

  const goToStop = useCallback(
    (index: number) => {
      const stop = shownStops[index]
      if (!stop || stop.x == null || stop.y == null) return
      setRouteAt(index)
      goTo({ artistId: stop.id, x: stop.x, y: stop.y })
    },
    [shownStops, goTo]
  )

  // A link to a star carries an id, not a position: the card knows where it
  // is, so the position is fetched and the camera flies there.
  useEffect(() => {
    const { artistId, view } = opened
    if (artistId === null) return
    const abort = new AbortController()
    fetchArtist(artistId, abort.signal)
      .then(artist => {
        const at = artist.position
        setPicked({ artistId, x: at?.x ?? 0, y: at?.y ?? 0 })
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

  // One count for the visit, not one per render: a mechanic opened twice in a
  // session is two uses, but a component re-rendering is not.
  useEffect(() => {
    count('sky_opened')
  }, [])

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
    // A standalone page owns the address while it is up; letting the map
    // write over it would replace `/charter` with `/` on the first frame the
    // sky renders behind it.
    if (page) return
    const next = writeLocation({ artistId: picked?.artistId ?? null, view: state.view, route })
    if (next !== window.location.pathname + window.location.hash) {
      window.history.replaceState(null, '', next)
    }
  }, [state, picked, page, route])

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

  // Checked first, and before anything else renders: an embed is a rectangle
  // in someone else's page and must not boot a sky behind it.
  if (page?.kind === 'embed') {
    return <Embed artistId={page.artistId} />
  }
  if (page?.kind === 'charter') {
    return <Charter me={me} onSignedOut={() => adopt(null)} onClose={leavePage} />
  }
  if (page?.kind === 'confirm') {
    return <ConfirmPage token={page.token} onClose={leavePage} />
  }
  if (page?.kind === 'reset') {
    return <ResetPage token={page.token} onClose={leavePage} />
  }

  return (
    <main className="relative h-full overflow-hidden">
      <Sky
        onState={onState}
        onPick={onPick}
        target={target}
        initial={opened.view}
        marked={picked && { x: picked.x, y: picked.y, shape, colour }}
        onCapture={onCapture}
        instruments={instruments}
        route={routeLine}
        labels={labels}
        onOverview={onOverview}
      />

      <header className="pointer-events-none absolute left-6 top-5 flex items-center gap-3">
        <img className="size-9" src="/mark.svg" alt="" />
        <div>
          <h1 className="m-0 text-lg font-semibold tracking-wide text-text">lyrid</h1>
          <p className="m-0 text-xs text-dim">{t('app.tagline')}</p>
        </div>
      </header>

      <Search onPick={goTo} />

      {/* The right-hand column under the search box: the card, the route and
          the compass. One column with a ceiling, for the reason the left stack
          has one -- pieces that grow must push each other rather than land on
          each other -- and the card is the piece that yields, scrolling inside
          itself. */}
      <div className="pointer-events-none absolute bottom-4 right-6 top-20 flex flex-col items-end justify-end gap-2 [&>*]:pointer-events-auto">
        {/* Keyed by the star: picking another one mounts a fresh card rather
            than leaving the previous artist on screen while the new one loads. */}
        {picked && (
          <StarCard
            key={picked.artistId}
            className="mb-auto min-h-0"
            artistId={picked.artistId}
            onClose={() => setPicked(null)}
            onOpen={openById}
            onAddToRoute={side => {
              if (route.length === 0) count('route_opened')
              setRoute(current => addStop(current, side.id))
            }}
            pinned={pinned}
            onPin={setPinned}
            onCompare={side => {
              if (!pinned) return
              count('stars_compared')
              setComparing([pinned, side])
            }}
          />
        )}
        {shownStops.length > 0 && (
          <RoutePanel
            className="shrink-0"
            stops={shownStops}
            at={routeAt}
            onGo={goToStop}
            onRemove={index => {
              setRoute(current => removeStop(current, index))
              setRouteAt(null)
            }}
            onClear={() => {
              setRoute([])
              setRouteAt(null)
            }}
          />
        )}
        {overview && state && (
          <Compass className="hidden shrink-0 md:flex" overview={overview} view={state.view} visible={state.visible} onNavigate={setTarget} />
        )}
      </div>

      {comparing && (
        <Suspense fallback={null}>
          <Compare
            a={comparing[0]}
            b={comparing[1]}
            onClose={() => setComparing(null)}
            onOpen={id => {
              setComparing(null)
              openById(id)
            }}
          />
        </Suspense>
      )}

      {state && (
        // Not on a phone's width: there the card already covers the stack,
        // and two more panels would bury the sky. A narrow layout of its own
        // is a stage of its own.
        <div className="pointer-events-none absolute left-6 top-20 hidden md:block [&>*]:pointer-events-auto">
          <Instruments value={instruments} onChange={setInstruments} />
        </div>
      )}

      {state && (
        // One stack in the bottom-left, so nothing can land on top of
        // anything else as the pieces grow -- the defect v0.9.1 fixed, and
        // the reason the account panel joins the stack rather than claiming
        // a corner of its own. The top-right is the search box and the card.
        //
        // Bounded to the window, which v0.12 had to add: the stack grew a
        // tall piece and immediately reproduced that same defect in the other
        // direction -- at 1280x720 with the sign-in form open it ran 32 px off
        // the TOP of the screen and took the nearby list's heading with it.
        // Pinning both edges turns "as tall as it likes" into "as tall as
        // there is room for", and `justify-end` keeps it growing upward from
        // the corner it belongs to. Pointer events are handed back per child
        // so the full-height box does not swallow drags meant for the sky.
        <div className="pointer-events-none absolute bottom-4 left-6 top-4 flex md:top-72 flex-col items-start justify-end gap-2 [&>*]:pointer-events-auto">
          {/* The keyboard's way into the sky, first in the stack because it is
              the one piece here that is not optional: without it the canvas has
              no reachable content at all. It is also the only piece that can
              usefully be shorter, so it is the one that shrinks -- `min-h-0`
              because a flex child will not shrink below its content otherwise. */}
          <NearbyStars className="min-h-0" visible={state.visible} onPick={goTo} />
          <AccountPanel me={me} onSignedIn={adopt} onSignedOut={() => adopt(null)} onCharter={openCharter} />
          <HaloPicker shape={shape} colour={colour} onShape={chooseShape} onColour={chooseColour} />
          <Share capture={captureRef} artistId={picked?.artistId ?? null} />
          <div className="flex items-center gap-2">
            <p className="pointer-events-none m-0 font-mono text-xs text-dim">
              {t('app.status', {
                stars: state.stars.toLocaleString(resolved),
                level: state.level,
                version: __APP_VERSION__,
              })}
            </p>
            <LanguagePicker />
          </div>
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
function Share({ capture, artistId }: { capture: { current: (() => Promise<Blob | null>) | null }; artistId: number | null }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState<'link' | 'embed' | null>(null)

  const flash = (what: 'link' | 'embed') => {
    setCopied(what)
    window.setTimeout(() => setCopied(null), 1500)
  }

  const copyLink = () => {
    const url = window.location.href
    void navigator.clipboard.writeText(url).then(
      () => {
        count('view_shared')
        flash('link')
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
      count('view_shared')
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

  // The embed is offered only with a star open. A widget of the whole sky
  // would be a WebGL canvas in someone else's page, which is what the embed
  // deliberately is not.
  const copyEmbed = () => {
    if (artistId === null) return
    const src = `${window.location.origin}/embed/star/${String(artistId)}`
    const code = `<iframe src="${src}" width="320" height="180" style="border:0" loading="lazy" title="lyrid"></iframe>`
    void navigator.clipboard.writeText(code).then(
      () => {
        count('view_shared')
        flash('embed')
      },
      () => {
        // Clipboard access can be refused outright; nothing to apologise for.
      }
    )
  }

  return (
    <div className="flex gap-2">
      <Button size="sm" className="glass" onClick={copyLink}>
        {copied === 'link' ? t('share.linkCopied') : t('share.copyLink')}
      </Button>
      <Button size="sm" className="glass" onClick={savePoster}>
        {t('share.savePoster')}
      </Button>
      {artistId !== null && (
        <Button size="sm" className="glass" onClick={copyEmbed}>
          {copied === 'embed' ? t('share.embedCopied') : t('share.copyEmbed')}
        </Button>
      )}
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
