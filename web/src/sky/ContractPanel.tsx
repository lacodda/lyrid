import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { Button } from '@/components/ui/button'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import type { Contract } from '@/expedition'
import type { Echo } from './fog'
import type { Place } from './renderer'

interface Props {
  contract: Contract
  /**
   * The echoes of the contract's star in this person's sky: the stars that
   * fulfil it, closest first. Named nowhere here -- an echo keeps its name until
   * it is reached, as everywhere else in the fog.
   */
  echoes: Echo[]
  /** Flies to a star and opens its card. */
  onGo: (star: Place) => void
  /** Puts a fulfilled contract away. */
  onClose: () => void
  className?: string
}

/**
 * The contract in hand, in the corner stack.
 *
 * One at a time, the oldest still to do: in this version that is the first
 * contract of the expedition, chart the home sector. It says what is asked, how
 * far it has come in pips rather than a percentage -- three is a number a person
 * counts, not a ratio -- and offers the way forward: home, and the next echo.
 * Fulfilled, it says so once and can be put away.
 */
export function ContractPanel({ contract, echoes, onGo, onClose, className }: Props) {
  const { t } = useTranslation()
  const { star, goal, progress } = contract
  const done = contract.done_at !== null
  const name = star.name ?? t('expedition.homeUnknown')
  const home = star.id !== null && star.x !== null && star.y !== null ? { artistId: star.id, x: star.x, y: star.y } : null
  const next = echoes[0]

  return (
    <section className={cn(panelVariants(), 'glass flex w-56 shrink-0 flex-col gap-1.5 p-2', className)} aria-labelledby="contract-heading">
      <SectionLabel id="contract-heading" className="text-dim">
        {done ? t('expedition.doneLabel') : t('expedition.contractLabel')}
      </SectionLabel>
      <p className="m-0 font-mono text-xs font-semibold text-text">{done ? t('expedition.doneTitle') : t('expedition.homeTitle')}</p>
      <p className="m-0 text-xs leading-relaxed text-text">
        {done ? t('expedition.doneBody', { count: goal, name }) : t('expedition.homeBody', { count: goal, name })}
      </p>
      {goal > 0 && (
        <div className="flex items-center gap-1.5" role="img" aria-label={t('expedition.progress', { progress, goal })}>
          {Array.from({ length: goal }, (_, index) => (
            <span
              key={index}
              aria-hidden="true"
              className={cn('size-2.5 rounded-full border border-accent', index < progress ? 'bg-accent' : 'bg-transparent')}
            />
          ))}
          <span className="font-mono text-2xs text-dim">{t('expedition.progress', { progress, goal })}</span>
        </div>
      )}
      {!done && <p className="m-0 text-2xs leading-relaxed text-dim">{t('expedition.how')}</p>}
      <div className="flex flex-wrap gap-1.5">
        {!done && next && (
          <Button size="sm" variant="primary" onClick={() => onGo({ artistId: next.artistId, x: next.x, y: next.y })}>
            {t('expedition.toEcho')}
          </Button>
        )}
        {home && (
          <Button size="sm" onClick={() => onGo(home)}>
            {t('expedition.toHome')}
          </Button>
        )}
        {done && (
          <Button size="sm" onClick={onClose}>
            {t('expedition.close')}
          </Button>
        )}
      </div>
    </section>
  )
}
