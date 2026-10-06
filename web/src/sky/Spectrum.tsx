import { Trans, useTranslation } from 'react-i18next'

import type { Spectrum as SpectrumData, SpectrumBand } from '@/api'
import { SectionLabel } from '@/components/ui/panel'
import { Track, TrackScale } from '@/components/ui/track'

/**
 * What a star sounds like: tempo, energy, mood and the rest, each drawn as a
 * place on a line between its two ends.
 *
 * The place is the star's rank among every star AcousticBrainz measured, not
 * the number a model gave. The models are poorly calibrated — most music ever
 * measured is "relaxed" by their reckoning — so a raw figure would put nearly
 * every star at the same end of the same line; a rank says where this one
 * stands among the others, which is what a reader looks at a spectrum for.
 * The hairline in the middle of each line is the middle of all of them.
 */
export function Spectrum({ spectrum }: { spectrum: SpectrumData }) {
  const { t } = useTranslation()
  return (
    <>
      <SectionLabel>{t('spectrum.heading')}</SectionLabel>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        {spectrum.bands.map(band => (
          <Line key={band.name} band={band} bpm={band.name === 'tempo' ? spectrum.bpm : null} />
        ))}
      </ul>
      <p className="m-0 text-2xs text-faint">
        <Trans
          i18nKey="spectrum.measured"
          count={spectrum.recordings}
          components={[
            <a key="source" className="text-dim underline underline-offset-2" href="https://acousticbrainz.org/" target="_blank" rel="noreferrer" />,
          ]}
        />
      </p>
    </>
  )
}

function Line({ band, bpm }: { band: SpectrumBand; bpm: number | null }) {
  const { t } = useTranslation()
  const name = t(`spectrum.${band.name}.name`)
  const percent = percentOf(band.rank)
  return (
    <li className="grid grid-cols-[5.5rem_1fr] items-start gap-x-3 text-xs">
      <span className="flex flex-col">
        <span className="text-text">{name}</span>
        {/* Beats per minute mean something by themselves, so tempo keeps its
            number beside its place. */}
        {bpm !== null && <span className="text-2xs text-faint">{t('spectrum.bpm', { bpm: Math.round(bpm) })}</span>}
      </span>
      <span className="flex flex-col gap-0.5 pt-1">
        <Track
          size="sm"
          from={0}
          to={100}
          minWidth={0}
          segments={[{ key: 'middle', start: 49.75, end: 50.25, tone: 'idle', label: t('spectrum.middle') }]}
          marker={band.rank * 100}
          label={t('spectrum.place', { band: name, percent })}
        />
        <TrackScale>
          <span>{t(`spectrum.${band.name}.low`)}</span>
          <span>{t(`spectrum.${band.name}.high`)}</span>
        </TrackScale>
      </span>
    </li>
  )
}

/** A rank as the whole percent of stars it stands above, for a reader who cannot see the line. */
export function percentOf(rank: number): number {
  return Math.round(Math.min(Math.max(rank, 0), 1) * 100)
}
