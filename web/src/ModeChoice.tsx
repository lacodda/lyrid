import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { chooseMode, type Me, type Mode } from '@/account'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { count } from '@/metrics'
import { useSkyField } from '@/sky/field'
import type { Overview } from '@/sky/Sky'

interface Props {
  /** The sky, once it has loaded, for the two small pictures of it. */
  overview: Overview | null
  onChosen: (me: Me) => void
  /** Not now: the sky stays open, and the question comes back on the next visit. */
  onLater: () => void
}

/** The picture on each door, a strip through the middle of the sky. */
const WIDTH = 368
const HEIGHT = 150
/**
 * The side the small sky is drawn at, wider than the picture: the sky is a disc,
 * and its bright part fills the strip only when the strip cuts through it.
 */
const SIDE = 720

/**
 * The one question lyrid asks of everybody, once (Vision, principle 5).
 *
 * The first thing after registering, and the first thing on the next visit of a
 * profile from before there was anything to choose between: one screen and one
 * route for both, so there is one way to arrive at a mode.
 *
 * It says plainly that it is permanent, and it can be put off, because a choice
 * that cannot be taken back should not also be one that cannot be left. A person
 * who puts it off has the whole sky in the meantime; an account is memory added,
 * never a gate.
 */
export function ModeChoice({ overview, onChosen, onLater }: Props) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)
  // Where the keyboard lands when the dialog opens. Not on either door: pressing
  // Enter by accident must not decide something that cannot be taken back.
  const later = useRef<HTMLButtonElement>(null)

  const choose = (mode: Mode) => {
    setBusy(true)
    setFailed(false)
    chooseMode(mode).then(
      me => {
        count('mode_chosen')
        onChosen(me)
      },
      () => {
        setBusy(false)
        setFailed(true)
      }
    )
  }

  return (
    <Dialog
      open
      onOpenChange={open => {
        if (!open) onLater()
      }}
    >
      <DialogPopup size="xl" initialFocus={later}>
        <DialogHeader>
          <DialogTitle>{t('fog.chooseTitle')}</DialogTitle>
          <DialogDescription>{t('fog.chooseBody')}</DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-3">
          <div className="grid gap-3 md:grid-cols-2">
            <Door
              overview={overview}
              fogged
              title={t('fog.exploreTitle')}
              body={t('fog.exploreBody')}
              action={t('fog.exploreAction')}
              primary
              busy={busy}
              onChoose={() => choose('explore')}
            />
            <Door
              overview={overview}
              fogged={false}
              title={t('fog.createTitle')}
              body={t('fog.createBody')}
              action={t('fog.createAction')}
              primary={false}
              busy={busy}
              onChoose={() => choose('create')}
            />
          </div>
          {failed && (
            <p role="alert" className="m-0 text-xs text-bad">
              {t('fog.chooseFailed')}
            </p>
          )}
          <button ref={later} type="button" className="cursor-pointer self-center text-xs text-dim underline-offset-2 hover:underline" onClick={onLater}>
            {t('fog.later')}
          </button>
        </DialogBody>
      </DialogPopup>
    </Dialog>
  )
}

interface DoorProps {
  overview: Overview | null
  fogged: boolean
  title: string
  body: string
  action: string
  primary: boolean
  busy: boolean
  onChoose: () => void
}

function Door({ overview, fogged, title, body, action, primary, busy, onChoose }: DoorProps) {
  return (
    <section className="flex flex-col overflow-hidden rounded-md border border-line">
      {/* The same sky behind both doors: the choice is what happens in it. */}
      {overview ? <SmallSky overview={overview} fogged={fogged} /> : <div className="bg-void" style={{ height: HEIGHT }} />}
      <div className="flex flex-1 flex-col gap-2 p-3">
        <h3 className="m-0 font-mono text-sm font-semibold text-text">{title}</h3>
        <p className="m-0 flex-1 text-xs leading-relaxed text-dim">{body}</p>
        <Button variant={primary ? 'primary' : undefined} size="sm" className="self-start" disabled={busy} onClick={onChoose}>
          {action}
        </Button>
      </div>
    </section>
  )
}

/** Where the small sky in the fog has been: two soft patches, the way the reference draws it. */
const SECTORS = [
  { x: 0.44, y: 0.62, r: 0.13 },
  { x: 0.6, y: 0.3, r: 0.09 },
]

function SmallSky({ overview, fogged }: { overview: Overview; fogged: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const field = useSkyField(overview, SIDE)

  useEffect(() => {
    const canvas = ref.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    const ratio = window.devicePixelRatio || 1
    canvas.width = Math.round(WIDTH * ratio)
    canvas.height = Math.round(HEIGHT * ratio)
    // Centred both ways: what shows is the middle of the sky, where it is densest.
    const left = (canvas.width - field.width) / 2
    const top = (canvas.height - field.height) / 2

    context.fillStyle = '#07080d'
    context.fillRect(0, 0, canvas.width, canvas.height)
    context.globalAlpha = fogged ? 0.12 : 1
    context.drawImage(field, left, top)
    context.globalAlpha = 1
    if (!fogged) return

    // The sky again, seen through a soft mask of where has been opened.
    const lit = document.createElement('canvas')
    lit.width = canvas.width
    lit.height = canvas.height
    const paint = lit.getContext('2d')
    if (!paint) return
    for (const sector of SECTORS) {
      const x = sector.x * canvas.width
      const y = sector.y * canvas.height
      const radius = sector.r * canvas.width
      const glow = paint.createRadialGradient(x, y, 0, x, y, radius)
      glow.addColorStop(0, 'rgba(0, 0, 0, 1)')
      glow.addColorStop(0.7, 'rgba(0, 0, 0, 0.9)')
      glow.addColorStop(1, 'rgba(0, 0, 0, 0)')
      paint.fillStyle = glow
      paint.fillRect(0, 0, lit.width, lit.height)
    }
    paint.globalCompositeOperation = 'source-in'
    paint.drawImage(field, left, top)
    context.drawImage(lit, 0, 0)
  }, [field, fogged])

  return <canvas ref={ref} aria-hidden="true" className="block w-full" style={{ height: HEIGHT }} />
}
