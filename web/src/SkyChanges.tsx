import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { SectionLabel } from '@/components/ui/panel'
import { Spinner } from '@/components/ui/spinner'
import { fetchChanges, risingState, splitReleases, versionDate, type Edition, type Release, type Rising, type SkyChanges as Changes } from '@/changes'
import { useLanguage } from '@/lib/language'
import type { PlacedStar } from '@/scrobbling'

/**
 * What changed in the sky: which edition of the canon it is and what that
 * edition changed, the stars rising this week, and the releases out around
 * now.
 *
 * A dialog opened from the status line, like the rest of what is visited now
 * and then rather than watched. Every name in it is a star this sky can fly
 * to, and pressing one goes there.
 */

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** What the status line already read, so the dialog opens on it at once. */
  changes: Changes | null
  onChanges: (changes: Changes) => void
  onOpen: (star: PlacedStar) => void
}

export function SkyChanges({ open, onOpenChange, changes, onChanges, onOpen }: Props) {
  const { t } = useTranslation()
  const [failed, setFailed] = useState(false)

  // Asked again on every opening: the feeds change it twice an hour, and a
  // dialog left open from yesterday would show yesterday's weather.
  useEffect(() => {
    if (!open) return
    const abort = new AbortController()
    fetchChanges(abort.signal).then(
      next => {
        setFailed(false)
        onChanges(next)
      },
      () => {
        if (!abort.signal.aborted) setFailed(true)
      }
    )
    return () => abort.abort()
  }, [open, onChanges])

  const go = (star: PlacedStar) => {
    onOpenChange(false)
    onOpen(star)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup size="md">
        <DialogHeader>
          <DialogTitle>{t('changes.heading')}</DialogTitle>
          <DialogDescription>{t('changes.description')}</DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-5">
          {changes ? (
            <>
              <EditionSection edition={changes.edition} onOpen={go} />
              <RisingSection rising={changes.rising} onOpen={go} />
              <ReleasesSection releases={changes.releases.releases} onOpen={go} />
            </>
          ) : failed ? (
            <p role="alert" className="m-0 text-sm text-bad">
              {t('changes.failed')}
            </p>
          ) : (
            <Spinner label={t('changes.loading')} />
          )}
        </DialogBody>
      </DialogPopup>
    </Dialog>
  )
}

function useDay() {
  const { resolved } = useLanguage()
  return (at: string | Date) =>
    new Intl.DateTimeFormat(resolved, { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' }).format(typeof at === 'string' ? new Date(at) : at)
}

/** A star's name as a way to fly to it. */
function StarButton({ star, onOpen, children }: { star: PlacedStar; onOpen: (star: PlacedStar) => void; children?: ReactNode }) {
  return (
    <Button variant="link" className="inline text-left whitespace-normal [overflow-wrap:anywhere]" onClick={() => onOpen(star)}>
      {children ?? star.name}
    </Button>
  )
}

function EditionSection({ edition, onOpen }: { edition: Edition | null; onOpen: (star: PlacedStar) => void }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const day = useDay()

  if (!edition) {
    return (
      <section className="flex flex-col gap-1.5" aria-labelledby="changes-edition">
        <SectionLabel id="changes-edition">{t('changes.edition.heading')}</SectionLabel>
        <p className="m-0 text-sm text-dim">{t('changes.edition.unsealed')}</p>
      </section>
    )
  }

  const changed = edition.changed
  return (
    <section className="flex flex-col gap-2" aria-labelledby="changes-edition">
      <SectionLabel id="changes-edition">{t('changes.edition.heading')}</SectionLabel>
      <p className="m-0 text-sm text-text">
        {t('changes.edition.is', {
          number: edition.number,
          day: day(edition.sealed_at),
          stars: edition.stars.toLocaleString(resolved),
        })}
      </p>
      {!edition.current && <p className="m-0 text-xs text-warn">{t('changes.edition.notCurrent')}</p>}
      <ul className="m-0 flex list-none flex-col gap-0.5 p-0 text-xs text-dim">
        {edition.sources.map(source => {
          const date = versionDate(source.version)
          return (
            <li key={source.source}>
              {t('changes.edition.source', {
                source: t(`changes.sources.${source.source}`, { defaultValue: source.source }),
                version: date ? day(date) : source.version,
                imported: day(source.imported_at),
              })}
            </li>
          )
        })}
      </ul>

      {changed && edition.previous ? (
        <>
          <p className="m-0 text-sm text-text">
            {t('changes.edition.since', {
              number: edition.previous.number,
              new: changed.new.toLocaleString(resolved),
              merged: changed.merged.toLocaleString(resolved),
              gone: changed.gone.toLocaleString(resolved),
            })}
          </p>
          {changed.new_stars.length > 0 && (
            <div className="flex flex-col gap-0.5">
              <p className="m-0 text-xs text-dim">{t('changes.edition.newStars')}</p>
              <p className="m-0 flex flex-wrap gap-x-3 gap-y-0.5 text-sm">
                {changed.new_stars.map(star => (
                  <StarButton key={star.id} star={star} onOpen={onOpen} />
                ))}
              </p>
            </div>
          )}
          {changed.merges.length > 0 && (
            <div className="flex flex-col gap-0.5">
              <p className="m-0 text-xs text-dim">{t('changes.edition.merges')}</p>
              <ul className="m-0 flex list-none flex-col gap-0.5 p-0 text-sm">
                {changed.merges.map(merge => (
                  <li key={`${merge.from}-${String(merge.into.id)}`}>
                    <span className="text-dim">{merge.from}</span> → <StarButton star={merge.into} onOpen={onOpen} />
                  </li>
                ))}
              </ul>
            </div>
          )}
          {changed.gone_names.length > 0 && (
            <p className="m-0 text-xs text-dim">{t('changes.edition.gone', { names: changed.gone_names.join(', ') })}</p>
          )}
        </>
      ) : (
        <p className="m-0 text-xs text-dim">{t('changes.edition.first')}</p>
      )}
    </section>
  )
}

function RisingSection({ rising, onOpen }: { rising: Rising; onOpen: (star: PlacedStar) => void }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const day = useDay()
  const state = risingState(rising)
  const number = (value: number) => value.toLocaleString(resolved, { maximumFractionDigits: value < 10 ? 1 : 0 })

  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="changes-rising">
      <SectionLabel id="changes-rising">{t('changes.rising.heading')}</SectionLabel>
      {state === 'counting' && <p className="m-0 text-sm text-dim">{t('changes.rising.counting', { count: rising.days_counted })}</p>}
      {state === 'quiet' && <p className="m-0 text-sm text-dim">{t('changes.rising.quiet')}</p>}
      {state === 'rising' && rising.through && (
        <>
          <p className="m-0 text-xs text-dim">{t('changes.rising.how', { through: day(rising.through) })}</p>
          <ol className="m-0 flex list-none flex-col gap-0.5 p-0 text-sm">
            {rising.stars.map(star => (
              <li key={star.id} className="flex items-baseline justify-between gap-3">
                <StarButton star={star} onOpen={onOpen} />
                <span className="shrink-0 font-mono text-xs text-dim">
                  {t('changes.rising.numbers', { recent: number(star.recent), earlier: number(star.earlier) })}
                </span>
              </li>
            ))}
          </ol>
        </>
      )}
    </section>
  )
}

function ReleasesSection({ releases, onOpen }: { releases: Release[]; onOpen: (star: PlacedStar) => void }) {
  const { t } = useTranslation()
  const [now] = useState(() => new Date())
  const { out, coming } = splitReleases(releases, now)

  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="changes-releases">
      <SectionLabel id="changes-releases">{t('changes.releases.heading')}</SectionLabel>
      {releases.length === 0 ? (
        <p className="m-0 text-sm text-dim">{t('changes.releases.none')}</p>
      ) : (
        <>
          <ReleaseList label={t('changes.releases.out')} releases={out} onOpen={onOpen} />
          <ReleaseList label={t('changes.releases.coming')} releases={coming} onOpen={onOpen} />
        </>
      )}
    </section>
  )
}

function ReleaseList({ label, releases, onOpen }: { label: string; releases: Release[]; onOpen: (star: PlacedStar) => void }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  if (releases.length === 0) return null
  const short = new Intl.DateTimeFormat(resolved, { day: 'numeric', month: 'short', timeZone: 'UTC' })
  return (
    <div className="flex flex-col gap-0.5">
      <p className="m-0 text-xs text-dim">{label}</p>
      <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm">
        {releases.map(release => (
          <li key={`${release.title}-${release.out_on}-${String(release.star.id)}`} className="flex items-baseline justify-between gap-3">
            <span className="min-w-0">
              <span className="text-text">{release.title}</span>{' '}
              <span className="text-dim">
                {t('changes.releases.by')} <StarButton star={release.star} onOpen={onOpen}>{release.credit}</StarButton>
              </span>
            </span>
            <span className="shrink-0 font-mono text-xs text-dim">
              {release.kind ? `${t(`changes.releases.kinds.${release.kind}`, { defaultValue: release.kind })} · ` : ''}
              {short.format(new Date(`${release.out_on}T00:00:00Z`))}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}
