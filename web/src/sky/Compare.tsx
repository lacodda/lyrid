import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { fetchComparison, type Comparison, type SpectrumLine } from '@/api'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogActions,
  DialogBody,
  DialogClose,
  DialogDescription,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { SectionLabel } from '@/components/ui/panel'
import { Spinner } from '@/components/ui/spinner'
import { reasons } from './why'

/** One side of a comparison: enough to name it. */
export interface Side {
  id: number
  name: string
}

interface Props {
  a: Side
  b: Side
  onClose: () => void
  /** Opens a star named in the comparison — a shared neighbour. */
  onOpen: (id: number) => void
}

/**
 * The spectrograph: two stars side by side.
 *
 * What joins them — the same reasons the card gives a neighbour — then their
 * sounds as spectra, genre by genre, each as a share of that star's own
 * discography. Shares and not counts, so a prolific act and a sparse one are
 * compared by what their work is rather than by how much of it there is.
 * Then the stars both are listened alongside: the common ground on the map.
 */
export function Compare({ a, b, onClose, onOpen }: Props) {
  const { t } = useTranslation()
  const [comparison, setComparison] = useState<Comparison | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const abort = new AbortController()
    fetchComparison(a.id, b.id, abort.signal).then(setComparison, (cause: unknown) => {
      if (abort.signal.aborted) return
      setError(cause instanceof Error ? cause.message : t('compare.failed'))
    })
    return () => abort.abort()
  }, [a.id, b.id, t])

  const said = comparison ? reasons(comparison.why, t) : []

  return (
    <Dialog
      open
      onOpenChange={open => {
        if (!open) onClose()
      }}
    >
      {/* The header and the close button stay put; only the body scrolls,
          so a long spectrum never carries the way out off the screen. */}
      <DialogPopup size="lg">
        <DialogHeader>
          <DialogTitle>{t('compare.title', { a: a.name, b: b.name })}</DialogTitle>
          <DialogDescription>{t('compare.lede')}</DialogDescription>
        </DialogHeader>

        <DialogBody className="flex flex-col gap-3">
          {!comparison && !error && (
            <p className="flex items-center gap-2 text-xs text-dim">
              <Spinner size="sm" label={t('card.reading')} />
              {t('card.reading')}
            </p>
          )}
          {error && (
            <p role="alert" className="m-0 text-xs text-bad">
              {error}
            </p>
          )}

          {comparison && (
            <>
              <SectionLabel>{t('compare.joins')}</SectionLabel>
              <p className="m-0 text-sm text-text">{said.length > 0 ? said.join(' · ') : t('compare.nothing')}</p>

              <SectionLabel>{t('compare.spectrum')}</SectionLabel>
              <Key a={a} b={b} />
              <Spectrum lines={comparison.spectrum.filter(line => !line.is_style)} a={a} b={b} />
              {comparison.spectrum.some(line => line.is_style) && (
                <>
                  <p className="m-0 mt-1 text-2xs text-dim">{t('compare.styles')}</p>
                  <Spectrum lines={comparison.spectrum.filter(line => line.is_style)} a={a} b={b} />
                </>
              )}

              {comparison.shared_neighbours.length > 0 && (
                <>
                  <SectionLabel>{t('compare.shared')}</SectionLabel>
                  <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-xs">
                    {comparison.shared_neighbours.map(star => (
                      <li key={star.id}>
                        <button
                          type="button"
                          className="cursor-pointer text-accent underline-offset-2 hover:underline"
                          onClick={() => onOpen(star.id)}
                        >
                          {star.name}
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </>
          )}
        </DialogBody>

        <DialogActions>
          <DialogClose render={<Button size="sm">{t('compare.close')}</Button>} />
        </DialogActions>
      </DialogPopup>
    </Dialog>
  )
}

/** Two series, so a key: each colour named once, beside its mark. */
function Key({ a, b }: { a: Side; b: Side }) {
  return (
    <p className="m-0 flex flex-wrap gap-x-4 gap-y-1 text-xs text-dim">
      <span className="flex items-center gap-1.5">
        <span aria-hidden="true" className={`size-2.5 rounded-full ${A_FILL}`} />
        {a.name}
      </span>
      <span className="flex items-center gap-1.5">
        <span aria-hidden="true" className={`size-2.5 rounded-full ${B_FILL}`} />
        {b.name}
      </span>
    </p>
  )
}

// The accent for the first star and warm gold for the second: two hues far
// apart on the wheel, so the pair survives every common colour deficiency.
// The key names them and every bar carries its figure in words, so identity
// never rests on colour alone.
const A_FILL = 'bg-accent'
const B_FILL = 'bg-[#e0a040]'

/**
 * The bands, one row each: the name, then a thin bar per star with its share
 * written beside it.
 */
function Spectrum({ lines, a, b }: { lines: SpectrumLine[]; a: Side; b: Side }) {
  const { t } = useTranslation()
  const percent = (share: number) => `${String(Math.round(share * 100))}%`
  return (
    <ul className="m-0 flex list-none flex-col gap-2 p-0">
      {lines.map(line => (
        <li key={`${line.name}-${String(line.is_style)}`} className="grid grid-cols-[7rem_1fr] items-center gap-x-3 text-xs">
          <span className="truncate text-text">{line.name}</span>
          <span className="flex flex-col gap-0.5">
            {[
              { side: a, share: line.a, fill: A_FILL },
              { side: b, share: line.b, fill: B_FILL },
            ].map(({ side, share, fill }) => (
              <span key={side.id} className="flex items-center gap-2">
                {/* The bar is drawn for the eye and said in words for a
                    reader: a bare percentage would not say whose it is. */}
                <span className="sr-only">{t('compare.share', { name: side.name, genre: line.name, share: percent(share) })}</span>
                <span className="h-1.5 flex-1 rounded-full bg-soft" aria-hidden="true">
                  <span className={`block h-full rounded-full ${fill}`} style={{ width: percent(share) }} />
                </span>
                <span className="w-9 shrink-0 text-right font-mono text-2xs text-dim" aria-hidden="true">
                  {percent(share)}
                </span>
              </span>
            ))}
          </span>
        </li>
      ))}
    </ul>
  )
}
