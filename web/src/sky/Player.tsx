import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import type { Nebula, Station } from '@/api'
import { Button } from '@/components/ui/button'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import { uploadIndex, type Listening } from './listening'
import { STATE, createPlayer, loadYouTube, type YouTubePlayer } from './youtube'

interface Props {
  listening: Listening | null
  /** The radio of the place the view is looking at, if it has one. */
  here: Nebula | null
  /** A line to say under the controls: why nothing played, or what failed. */
  note: string | null
  /** True while the signal of the day is being asked for. */
  tuning: boolean
  onRadio: (nebula: Nebula) => void
  onSignal: () => void
  /** Steps a radio back or on. */
  onStep: (by: 1 | -1) => void
  /** The song a radio was playing has ended. */
  onEnded: () => void
  /** What was meant to play cannot be played here. */
  onBroken: () => void
  /** Something is actually playing. */
  onPlaying: () => void
  /** Flies to the star that is playing and opens it. */
  onGo: (station: Station) => void
  onStop: () => void
  className?: string
}

/**
 * The one player: idle, it offers the radio of where the view is and the
 * signal of the day; playing, it is the video, what is playing, and the way to
 * that star on the sky.
 */
export function Player(props: Props) {
  const { t } = useTranslation()
  const { listening, here, note, tuning, onRadio, onSignal, onStop, className } = props

  const panel = cn(panelVariants(), 'glass flex w-[min(22rem,calc(100vw-3rem))] flex-col gap-2 p-3', className)

  if (!listening) {
    return (
      // Idle, it is an offer, and on a phone's width offers are what the
      // instruments and the compass already make way for: the narrow layout
      // is a stage of its own. Playing, it shows everywhere -- a card's play
      // button starts it, and music with no way to stop it would be worse.
      <section className={cn(panel, 'hidden md:flex')} aria-labelledby="listen-heading">
        <SectionLabel id="listen-heading" className="text-dim">
          {t('listen.heading')}
        </SectionLabel>
        <div className="flex flex-wrap gap-1.5">
          <Button
            size="sm"
            variant="soft"
            disabled={!here}
            disabledReason={t('listen.nothingHere')}
            onClick={() => {
              if (here) onRadio(here)
            }}
          >
            {here ? t('listen.radioOf', { name: here.name }) : t('listen.radio')}
          </Button>
          <Button size="sm" disabled={tuning} onClick={onSignal}>
            {tuning ? t('listen.tuning') : t('listen.signal')}
          </Button>
        </div>
        {note && (
          <p role="status" className="m-0 text-2xs text-dim">
            {note}
          </p>
        )}
      </section>
    )
  }

  return (
    <section className={panel} aria-labelledby="listen-heading">
      <div className="flex items-center gap-2">
        <SectionLabel id="listen-heading" className="min-w-0 flex-1 truncate text-dim">
          {heading(listening, t)}
        </SectionLabel>
        {listening.kind === 'radio' && (
          <span className="shrink-0 font-mono text-2xs text-dim">
            {t('listen.position', { at: listening.at + 1, of: listening.stations.length })}
          </span>
        )}
        <Button variant="icon" size="icon-sm" onClick={onStop} aria-label={t('listen.stop')}>
          ×
        </Button>
      </div>
      <Screen {...props} listening={listening} />
      <Footer {...props} listening={listening} />
      {note && (
        <p role="status" className="m-0 text-2xs text-dim">
          {note}
        </p>
      )}
    </section>
  )
}

type Translate = ReturnType<typeof useTranslation>['t']

function heading(listening: Listening, t: Translate): string {
  if (listening.kind === 'radio') return t('listen.radioOf', { name: listening.nebula.name })
  if (listening.kind === 'signal') return t('listen.signalHeading')
  return t('listen.channel')
}

/** What is playing, and the way to it on the sky. */
function Footer({ listening, onStep, onGo }: Props & { listening: Listening }) {
  const { t } = useTranslation()
  const station = listening.kind === 'channel' ? listening.station : listening.stations[listening.at]
  if (!station) return null

  // The signal keeps its name until the listener has reached it: hearing it
  // first and finding out who it is by going there is the whole mechanic.
  if (listening.kind === 'signal' && !listening.found) {
    return (
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-dim">{t('listen.somewhere')}</span>
        <Button size="sm" variant="primary" onClick={() => onGo(station)}>
          {t('listen.goToSignal')}
        </Button>
      </div>
    )
  }

  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        className="min-w-0 flex-1 cursor-pointer truncate text-left text-sm text-accent underline-offset-2 hover:underline"
        onClick={() => onGo(station)}
      >
        {station.name}
      </button>
      {listening.kind === 'radio' && (
        <>
          <Button variant="icon" size="icon-sm" disabled={listening.at === 0} onClick={() => onStep(-1)} aria-label={t('listen.previous')}>
            ⏮
          </Button>
          <Button variant="icon" size="icon-sm" onClick={() => onStep(1)} aria-label={t('listen.next')}>
            ⏭
          </Button>
        </>
      )}
    </div>
  )
}

/**
 * How long a channel may stay silent before it is given up on.
 *
 * A channel whose uploads cannot be listed -- closed, emptied, or kept from
 * embedding -- does not answer at all: no state, no error, a black box for as
 * long as anyone waits. Measured on 25 channels, the live ones answered within
 * 2.4 s of the player being made; six seconds leaves room for a slow network
 * and is short enough that a radio skipping one does not feel stopped.
 */
const PATIENCE_MS = 6_000

/**
 * The video: a YouTube player of its own for each star.
 *
 * A new player per station rather than one player handed station after
 * station, and that is measured, not taste: once a player has loaded one
 * channel's uploads, asking it for another channel's is silently ignored --
 * it re-cues the list it already has, and a second request gets no answer at
 * all. The radio built on one player played its first star and then skipped
 * every one after it. A fresh player costs about a second between songs.
 *
 * A radio plays one upload per star: the channel's uploads are cued, one of
 * the latest few is chosen, and when it ends the radio moves on. A channel or
 * the signal plays the uploads through, as the card's player always did.
 */
function Screen({ listening, onEnded, onBroken, onPlaying }: Props & { listening: Listening }) {
  const { t } = useTranslation()
  const station = listening.kind === 'channel' ? listening.station : listening.stations[listening.at]
  const single = listening.kind === 'radio'
  const seed = listening.kind === 'radio' ? listening.seed : 0

  const host = useRef<HTMLDivElement>(null)
  const [failed, setFailed] = useState(false)

  // The player's callbacks are registered when it is made and read these, so
  // a new callback from the parent does not cost a new player.
  const handlers = useRef({ onEnded, onBroken, onPlaying })
  useEffect(() => {
    handlers.current = { onEnded, onBroken, onPlaying }
  }, [onEnded, onBroken, onPlaying])

  useEffect(() => {
    if (!station) return
    const mount = document.createElement('div')
    host.current?.append(mount)
    let cancelled = false
    let made: YouTubePlayer | null = null
    // Cueing: the uploads are being listed so one can be chosen. Playing: a
    // video has been asked for, and its end is the radio's cue to move on.
    let phase: 'cueing' | 'playing' = single ? 'cueing' : 'playing'
    let watchdog: number | undefined

    loadYouTube().then(
      api => {
        if (cancelled) return
        // Any word from the player lifts this; silence past it is a channel
        // that will not play here -- a radio moves on, a channel says so.
        watchdog = window.setTimeout(() => handlers.current.onBroken(), PATIENCE_MS)
        made = createPlayer(api, mount, {
          onReady: event => {
            if (single) event.target.cuePlaylist({ list: station.uploads, listType: 'playlist' })
            else event.target.loadPlaylist({ list: station.uploads, listType: 'playlist', index: 0 })
          },
          onStateChange: event => {
            window.clearTimeout(watchdog)
            if (event.data === STATE.cued && phase === 'cueing') {
              const uploads = event.target.getPlaylist() ?? []
              const video = uploads[uploadIndex(seed, station.id, uploads.length)]
              if (!video) {
                handlers.current.onBroken()
                return
              }
              phase = 'playing'
              event.target.loadVideoById(video)
            } else if (event.data === STATE.playing) {
              handlers.current.onPlaying()
            } else if (event.data === STATE.ended && phase === 'playing' && single) {
              handlers.current.onEnded()
            }
          },
          // 2, 5, 100, 101, 150: a bad id, a player fault, a removed video, or
          // one its owner will not have embedded. All mean the same thing to
          // a listener -- this one will not play here.
          onError: () => {
            window.clearTimeout(watchdog)
            handlers.current.onBroken()
          },
        })
      },
      () => {
        if (!cancelled) setFailed(true)
      }
    )
    return () => {
      cancelled = true
      window.clearTimeout(watchdog)
      made?.destroy()
      mount.remove()
    }
  }, [station, single, seed])

  return (
    // 200px tall at the least: YouTube's embedded player must be given a
    // viewport of at least 200 by 200, and a 16:9 box at this width is less.
    <div ref={host} className="relative h-50 w-full overflow-hidden rounded-md bg-black ring-1 ring-line [&_iframe]:size-full">
      {failed && <p className="absolute inset-0 m-0 grid place-items-center p-4 text-center text-xs text-dim">{t('listen.noPlayer')}</p>}
    </div>
  )
}
