import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import type { Hit } from '@/api'
import { Button } from '@/components/ui/button'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import { count } from '@/metrics'
import { step } from './route'

interface Props {
  /** The stops, resolved to names and places, in route order. */
  stops: Hit[]
  /** Which stop is being looked at, or null before the walk starts. */
  at: number | null
  onGo: (index: number) => void
  onRemove: (index: number) => void
  onClear: () => void
  className?: string
}

/**
 * A route of stars: the list, the step forward and back, and the link.
 *
 * The link is the route — `/route/54,962,120` — so sending it is sending the
 * whole walk, in order. That address is also the foundation of the routes as
 * playlists the plan holds for later, which is why it is shown and copied
 * rather than hidden behind a share button that could change shape.
 */
export function RoutePanel({ stops, at, onGo, onRemove, onClear, className }: Props) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const ids = stops.map(stop => stop.id)
  const previous = at === null ? null : step(ids, at, -1)
  const next = at === null ? (stops.length > 0 ? 0 : null) : step(ids, at, 1)

  const copy = () => {
    void navigator.clipboard.writeText(window.location.href).then(
      () => {
        count('view_shared')
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      },
      () => {
        // Refused clipboard: the address bar holds the same link.
      }
    )
  }

  return (
    <section className={cn(panelVariants(), 'glass flex w-64 flex-col gap-1.5 p-2', className)} aria-labelledby="route-heading">
      <div className="flex items-center justify-between gap-2">
        <SectionLabel id="route-heading" className="text-dim">
          {t('route.heading', { count: stops.length })}
        </SectionLabel>
        <Button variant="icon" size="icon-sm" onClick={onClear} aria-label={t('route.clear')}>
          ×
        </Button>
      </div>

      {/* Its own scroll with a ceiling, so a long route neither pushes the
          compass off the screen nor is squeezed to nothing by the card: the
          card is the piece of the column that yields. */}
      <ol className="m-0 flex max-h-40 list-none flex-col overflow-y-auto p-0">
        {stops.map((stop, index) => (
          <li key={`${String(stop.id)}-${String(index)}`} className="flex items-center gap-1">
            <button
              type="button"
              aria-current={index === at ? 'step' : undefined}
              className={cn(
                'flex min-w-0 flex-1 cursor-pointer items-baseline gap-1.5 rounded-sm px-1.5 py-1 text-left text-xs hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none',
                index === at ? 'text-accent' : 'text-text'
              )}
              onClick={() => onGo(index)}
            >
              <span className="w-4 shrink-0 text-right font-mono text-2xs text-dim">{index + 1}</span>
              <span className="truncate">{stop.name}</span>
            </button>
            <Button variant="icon" size="icon-sm" onClick={() => onRemove(index)} aria-label={t('route.remove', { name: stop.name })}>
              −
            </Button>
          </li>
        ))}
      </ol>

      <div className="flex flex-wrap gap-1.5">
        <Button size="sm" disabled={previous === null} onClick={() => previous !== null && onGo(previous)}>
          {t('route.previous')}
        </Button>
        <Button size="sm" disabled={next === null} onClick={() => next !== null && onGo(next)}>
          {at === null ? t('route.start') : t('route.next')}
        </Button>
        <Button size="sm" onClick={copy}>
          {copied ? t('share.linkCopied') : t('share.copyLink')}
        </Button>
      </div>
    </section>
  )
}
