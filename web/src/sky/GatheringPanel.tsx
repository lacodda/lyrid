import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { Button } from '@/components/ui/button'
import { panelVariants } from '@/components/ui/panel'
import { useLanguage } from '@/lib/language'

interface Props {
  /** The station whose stars are marked. */
  name: string
  /** How many of them. */
  size: number
  /** Opens the station's dossier again. */
  onBack: () => void
  onClear: () => void
  className?: string
}

/**
 * Which station's stars the sky is marking, and the ways out: back to its
 * dossier, or the marks gone.
 *
 * The rings on the sky say nothing of whose they are; without this line a
 * reader who panned away would be left with gold circles and no key.
 */
export function GatheringPanel({ name, size, onBack, onClear, className }: Props) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  return (
    <section className={cn(panelVariants(), 'glass flex w-64 flex-col gap-1.5 p-2', className)} aria-label={t('gathering.label', { name })}>
      <div className="flex items-start justify-between gap-2">
        <p className="m-0 flex min-w-0 items-center gap-1.5 text-xs text-text">
          <span aria-hidden="true" className="size-2.5 shrink-0 rounded-full border-2 border-[#e0a040]" />
          <span className="truncate">{t('gathering.heading', { name, count: size, number: size.toLocaleString(resolved) })}</span>
        </p>
        <Button variant="icon" size="icon-sm" onClick={onClear} aria-label={t('gathering.clear')}>
          ×
        </Button>
      </div>
      <Button size="sm" className="self-start" onClick={onBack}>
        {t('gathering.back')}
      </Button>
    </section>
  )
}
