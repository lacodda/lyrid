import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { search, type Hit } from '@/api'
import { Dialog, DialogBody, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { SearchField } from '@/components/ui/search-field'
import { beginExpedition, type Expedition as Begun } from '@/expedition'
import { count } from '@/metrics'

interface Props {
  /** The expedition as the server holds it once it has begun. */
  onBegun: (expedition: Begun, star: Hit) => void
  /** Not now: the sky stays dark, and the question comes back on the next visit. */
  onLater: () => void
}

/**
 * The first step of the exploration mode: "name an artist you love" (S5).
 *
 * Asked right after the mode is chosen, and on the next visit of an explorer
 * who has not answered. The star named becomes the home port, it is lit at once
 * -- a person who loves an artist has heard them -- and the first contract asks
 * for three new stars of its neighbourhood, which its echoes point at. It can
 * be put off like the mode: an expedition nobody asked to start yet is not one
 * to force.
 */
export function HomePortChoice({ onBegun, onLater }: Props) {
  const { t } = useTranslation()
  const [term, setTerm] = useState('')
  const [stars, setStars] = useState<Hit[]>([])
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | null>(null)
  const field = useRef<HTMLInputElement>(null)
  const timer = useRef<number | undefined>(undefined)

  useEffect(() => {
    window.clearTimeout(timer.current)
    if (term.trim().length < 2) return
    // Debounced, as the search box is: a keystroke is not a query.
    const abort = new AbortController()
    timer.current = window.setTimeout(() => {
      search(term, abort.signal).then(
        found => setStars(found.stars.filter(star => star.x !== null && star.y !== null)),
        () => {
          if (!abort.signal.aborted) setStars([])
        }
      )
    }, 180)
    return () => {
      window.clearTimeout(timer.current)
      abort.abort()
    }
  }, [term])

  const choose = (star: Hit) => {
    setBusy(true)
    setFailure(null)
    beginExpedition(star.mbid).then(
      expedition => {
        count('expedition_begun')
        onBegun(expedition, star)
      },
      (error: unknown) => {
        setBusy(false)
        setFailure(error instanceof Error ? error.message : t('expedition.failed'))
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
      <DialogPopup size="md" initialFocus={field}>
        <DialogHeader>
          <DialogTitle>{t('expedition.askTitle')}</DialogTitle>
          <DialogDescription>{t('expedition.askBody')}</DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-2">
          <SearchField
            ref={field}
            value={term}
            onValueChange={next => {
              setTerm(next)
              if (next.trim().length < 2) setStars([])
            }}
            placeholder={t('expedition.placeholder')}
            aria-label={t('expedition.askTitle')}
            clearLabel={t('search.clear')}
            spellCheck={false}
            disabled={busy}
          />
          {stars.length > 0 && (
            <ul className="m-0 flex max-h-72 list-none flex-col overflow-y-auto p-0" aria-label={t('expedition.results')}>
              {stars.map(star => (
                <li key={star.id}>
                  <button
                    type="button"
                    disabled={busy}
                    className="block w-full cursor-pointer rounded-sm px-2 py-1.5 text-left text-sm text-text hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none disabled:cursor-default disabled:opacity-60"
                    onClick={() => choose(star)}
                  >
                    {star.name}
                    {star.comment && <span className="block text-xs text-dim">{star.comment}</span>}
                  </button>
                </li>
              ))}
            </ul>
          )}
          {term.trim().length >= 2 && stars.length === 0 && <p className="m-0 text-xs text-dim">{t('expedition.nothing')}</p>}
          {failure && (
            <p role="alert" className="m-0 text-xs text-bad">
              {failure}
            </p>
          )}
          <p className="m-0 text-xs leading-relaxed text-dim">{t('expedition.what')}</p>
          <button type="button" className="cursor-pointer self-center text-xs text-dim underline-offset-2 hover:underline" onClick={onLater}>
            {t('expedition.later')}
          </button>
        </DialogBody>
      </DialogPopup>
    </Dialog>
  )
}
