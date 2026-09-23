import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { panelVariants, SectionLabel } from '@/components/ui/panel'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { count } from '@/metrics'
import { ERAS, ERA_COLOURS, UNDATED, css, timeRange } from './decades'
import type { Instruments as Settings } from './renderer'

interface Props {
  value: Settings
  onChange: (next: Settings) => void
  className?: string
}

/**
 * The observer's instruments: the era lens and the time machine.
 *
 * Both change how the whole field is drawn and nothing else — no request, no
 * new tiles — so they are switches that act at once rather than a form with a
 * button. The time machine opens on the present, which draws the sky as it
 * always was; moving the slider back is what makes stars go out.
 */
export function Instruments({ value, onChange, className }: Props) {
  const { t } = useTranslation()
  const range = timeRange()
  const travelling = value.until !== null

  return (
    <section className={cn(panelVariants(), 'glass flex w-64 flex-col gap-2 p-3', className)} aria-labelledby="instruments-heading">
      <SectionLabel id="instruments-heading" className="text-dim">
        {t('instruments.heading')}
      </SectionLabel>

      <Switch
        checked={value.lens}
        onCheckedChange={lens => {
          if (lens) count('lens_used')
          onChange({ ...value, lens })
        }}
      >
        {t('instruments.lens')}
      </Switch>
      {value.lens && <Legend />}

      <Switch
        checked={travelling}
        onCheckedChange={on => {
          // Opens on the present: the sky as it already is, so turning the
          // machine on changes nothing until the slider is moved.
          onChange({ ...value, until: on ? range.max : null })
        }}
      >
        {t('instruments.timeMachine')}
      </Switch>
      {travelling && (
        <div className="flex flex-col gap-1">
          <div className="flex items-baseline justify-between text-xs">
            <span className="text-dim">{t('instruments.skyIn')}</span>
            {/* Announced as it changes, so a reader moving the thumb by
                keyboard hears the year the sky is showing. */}
            <span className="font-mono text-sm text-text" aria-live="polite">
              {value.until}
            </span>
          </div>
          <Slider
            value={value.until ?? range.max}
            min={range.min}
            max={range.max}
            step={1}
            aria-label={t('instruments.year')}
            onValueChange={year => {
              if (typeof year !== 'number') return
              onChange({ ...value, until: year })
            }}
            onValueCommitted={year => {
              if (typeof year === 'number' && year < range.max) count('time_travelled')
            }}
          />
          <p className="m-0 text-2xs text-dim">{t('instruments.timeHint')}</p>
        </div>
      )}
    </section>
  )
}

/**
 * What each colour under the lens means.
 *
 * The swatches are drawn from the same list the shader is given, so a legend
 * that disagreed with the sky would need someone to edit two lists — there is
 * only one. Each era is named in words beside its colour: the colour carries
 * identity on the sky, and the words carry it here, for anyone who cannot tell
 * two of the colours apart.
 */
function Legend() {
  const { t } = useTranslation()
  const names = ERAS.map((start, index) => {
    if (index === 0) return t('instruments.before', { year: ERAS[1] })
    if (index === ERAS.length - 1) return t('instruments.since', { year: start })
    return t('instruments.decade', { year: start })
  })
  return (
    <ul className="m-0 grid list-none grid-cols-2 gap-x-3 gap-y-0.5 p-0 text-2xs text-dim" aria-label={t('instruments.legend')}>
      {ERA_COLOURS.map((colour, index) => (
        <li key={ERAS[index]} className="flex items-center gap-1.5">
          <span aria-hidden="true" className="size-2.5 shrink-0 rounded-full" style={{ background: css(colour) }} />
          {names[index]}
        </li>
      ))}
      <li className="flex items-center gap-1.5">
        <span aria-hidden="true" className="size-2.5 shrink-0 rounded-full" style={{ background: css(UNDATED) }} />
        {t('instruments.undated')}
      </li>
    </ul>
  )
}
