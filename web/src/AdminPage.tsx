import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { KeyValue, KeyValueRow } from '@/components/ui/key-value'
import { Spinner } from '@/components/ui/spinner'
import { covered, feedState, fetchOverview, missing, Refused, shortSource, took, type Health, type Overview } from '@/admin'
import { useLanguage } from '@/lib/language'

/**
 * The stand's own state, for whoever runs it: the canon it holds, how its
 * feeds are doing, and how its accounts and mechanics are used.
 *
 * A page of its own, like the charter, rather than a panel over the sky: it
 * is read on purpose, now and then, and by one or two people. Counts only --
 * the server sends nothing that names a person, and so nothing here can.
 */
export function AdminPage({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const [overview, setOverview] = useState<Overview | null>(null)
  const [refused, setRefused] = useState<401 | 403 | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const abort = new AbortController()
    fetchOverview(abort.signal).then(setOverview, (failure: unknown) => {
      if (abort.signal.aborted) return
      if (failure instanceof Refused) setRefused(failure.status)
      else setError(failure instanceof Error ? failure.message : t('admin.failed'))
    })
    return () => abort.abort()
  }, [t])

  return (
    <main className="h-full overflow-y-auto bg-void">
      <article className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-10">
        <header className="flex items-baseline justify-between gap-4">
          <div>
            <h1 className="m-0 text-xl font-semibold text-text">{t('admin.title')}</h1>
            {overview && <p className="m-0 font-mono text-xs text-dim">{t('admin.version', { version: overview.version })}</p>}
          </div>
          <Button size="sm" onClick={onClose}>
            {t('admin.back')}
          </Button>
        </header>

        {refused === 401 && <p className="m-0 text-sm text-dim">{t('admin.signIn')}</p>}
        {refused === 403 && (
          <p className="m-0 text-sm text-dim">
            {t('admin.notAdmin')} <code className="font-mono text-xs text-text">lyrid admin grant</code>
          </p>
        )}
        {error && (
          <p role="alert" className="m-0 text-sm text-bad">
            {error}
          </p>
        )}
        {!overview && refused === null && error === null && <Spinner label={t('admin.loading')} />}
        {overview && <Sections overview={overview} />}
      </article>
    </main>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="m-0 border-b border-line pb-1 text-base font-semibold text-text">{title}</h2>
      {children}
    </section>
  )
}

function useFormat() {
  const { resolved } = useLanguage()
  const number = (value: number) => value.toLocaleString(resolved)
  const moment = (at: string | null) =>
    at === null ? '—' : new Intl.DateTimeFormat(resolved, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' }).format(new Date(at)) + ' UTC'
  const day = (at: string) => new Intl.DateTimeFormat(resolved, { day: 'numeric', month: 'short', timeZone: 'UTC' }).format(new Date(`${at.slice(0, 10)}T00:00:00Z`))
  return { number, moment, day }
}

function Sections({ overview }: { overview: Overview }) {
  const { t } = useTranslation()
  const { number, moment, day } = useFormat()
  const { canon, feeds, accounts } = overview
  const latest = canon.editions[0]

  return (
    <>
      <Section title={t('admin.canon.heading')}>
        <KeyValue>
          <KeyValueRow label={t('admin.canon.artists')}>{number(canon.artists)}</KeyValueRow>
          <KeyValueRow label={t('admin.canon.stars')}>{number(canon.stars)}</KeyValueRow>
          <KeyValueRow label={t('admin.canon.layout')}>
            {canon.layout ? `${canon.layout} · ${moment(canon.laid_out_at)}` : t('admin.canon.noLayout')}
          </KeyValueRow>
          <KeyValueRow label={t('admin.canon.edition')}>
            {latest
              ? t('admin.canon.editionIs', { number: latest.number, sealed: moment(latest.sealed_at), artists: number(latest.artists), stars: number(latest.stars) })
              : t('admin.canon.unsealed')}
          </KeyValueRow>
          {canon.slice && latest && (
            <KeyValueRow label={t('admin.canon.slice')}>
              {t('admin.canon.sliceIs', { held: number(canon.artists), of: number(latest.artists) })}
            </KeyValueRow>
          )}
        </KeyValue>

        {canon.editions.length > 0 && (
          <Table
            caption={t('admin.canon.editions')}
            head={[t('admin.columns.edition'), t('admin.columns.sealed'), t('admin.columns.layout'), t('admin.columns.stars'), t('admin.columns.changes')]}
            rows={canon.editions.map((edition, at) => [
              String(edition.number),
              moment(edition.sealed_at),
              edition.layout,
              number(edition.stars),
              // The oldest edition changed nothing: there was nothing before
              // it, and zeros would read as a rebuild that kept every star.
              at === canon.editions.length - 1
                ? t('admin.canon.first')
                : t('admin.canon.changes', { new: number(edition.new), merged: number(edition.merged), gone: number(edition.gone) }),
            ])}
          />
        )}
        <Table
          caption={t('admin.canon.imports')}
          head={[t('admin.columns.source'), t('admin.columns.version'), t('admin.columns.finished'), t('admin.columns.rows')]}
          rows={canon.imports.map(item => [item.source, item.version, moment(item.finished_at), item.rows === null ? '—' : number(item.rows)])}
        />
      </Section>

      <Section title={t('admin.listening.heading')}>
        <FeedHealth health={feeds.listening} />
        <Coverage days={feeds.listening.days} />
        <KeyValue>
          <KeyValueRow label={t('admin.listening.counted')}>{number(feeds.listening.counted)}</KeyValueRow>
          <KeyValueRow label={t('admin.listening.waiting')}>{number(feeds.listening.waiting)}</KeyValueRow>
          <KeyValueRow label={t('admin.listening.rising')}>
            {feeds.listening.rising_through
              ? t('admin.listening.risingIs', { count: feeds.listening.risers, through: day(feeds.listening.rising_through) })
              : t('admin.listening.notYet')}
          </KeyValueRow>
        </KeyValue>
        <Runs health={feeds.listening} />
      </Section>

      <Section title={t('admin.releases.heading')}>
        <FeedHealth health={feeds.releases} />
        <KeyValue>
          <KeyValueRow label={t('admin.releases.known')}>{number(feeds.releases.known)}</KeyValueRow>
          <KeyValueRow label={t('admin.releases.thisWeek')}>{number(feeds.releases.out_this_week)}</KeyValueRow>
        </KeyValue>
        <Runs health={feeds.releases} />
      </Section>

      <Section title={t('admin.accounts.heading')}>
        <KeyValue>
          <KeyValueRow label={t('admin.accounts.total')}>
            {t('admin.accounts.totalIs', { total: number(accounts.total), confirmed: number(accounts.confirmed) })}
          </KeyValueRow>
          <KeyValueRow label={t('admin.accounts.joined')}>
            {t('admin.accounts.joinedIs', { week: number(accounts.joined_this_week), month: number(accounts.joined_this_month) })}
          </KeyValueRow>
          <KeyValueRow label={t('admin.accounts.sessions')}>{number(accounts.sessions)}</KeyValueRow>
          <KeyValueRow label={t('admin.accounts.admins')}>{number(accounts.admins)}</KeyValueRow>
          <KeyValueRow label={t('admin.accounts.listenbrainz')}>
            {t('admin.accounts.listenbrainzIs', {
              linked: number(accounts.listenbrainz.linked),
              history: number(accounts.listenbrainz.reading_history),
              listens: number(accounts.listenbrainz.listens),
              read: moment(accounts.listenbrainz.last_read),
            })}
          </KeyValueRow>
          {accounts.listenbrainz.failing.length > 0 && (
            <KeyValueRow label={t('admin.accounts.failing')}>
              {accounts.listenbrainz.failing.map(failing => `${failing.failure}: ${number(failing.links)}`).join(', ')}
            </KeyValueRow>
          )}
          <KeyValueRow label={t('admin.accounts.lastfm')}>{number(accounts.lastfm)}</KeyValueRow>
          <KeyValueRow label={t('admin.accounts.letters')}>
            {t('admin.accounts.lettersIs', { subscribed: number(accounts.letters.subscribed), week: accounts.letters.last_week ?? '—' })}
          </KeyValueRow>
        </KeyValue>
      </Section>

      <Section title={t('admin.mechanics.heading')}>
        <p className="m-0 text-xs text-dim">{t('admin.mechanics.how')}</p>
        {overview.mechanics.length === 0 ? (
          <p className="m-0 text-sm text-dim">{t('admin.mechanics.none')}</p>
        ) : (
          <Table
            caption={t('admin.mechanics.caption')}
            head={[t('admin.columns.mechanic'), t('admin.columns.uses')]}
            rows={overview.mechanics.map(mechanic => [mechanic.mechanic, number(mechanic.total)])}
          />
        )}
      </Section>
    </>
  )
}

function FeedHealth({ health }: { health: Health }) {
  const { t } = useTranslation()
  const { moment, number } = useFormat()
  const state = feedState(health)
  const variant = state === 'ok' ? 'good' : state === 'failing' ? 'bad' : state === 'running' ? 'info' : 'soft'
  return (
    <div className="flex flex-col gap-1.5">
      <div>
        <Badge variant={variant}>{t(`admin.state.${state}`)}</Badge>
      </div>
      <KeyValue>
        <KeyValueRow label={t('admin.feed.lastDone')}>{moment(health.last_done)}</KeyValueRow>
        <KeyValueRow label={t('admin.feed.lastFailed')}>{moment(health.last_failed)}</KeyValueRow>
        <KeyValueRow label={t('admin.feed.failedThisWeek')}>{number(health.failed_this_week)}</KeyValueRow>
      </KeyValue>
    </div>
  )
}

/**
 * The last weeks of listening as a strip: a bar a day, as tall as the share of
 * the day the dumps covered. A day ListenBrainz published no dump for is a gap
 * in the strip, which is the point of drawing it.
 */
function Coverage({ days }: { days: Overview['feeds']['listening']['days'] }) {
  const { t } = useTranslation()
  const { number, day } = useFormat()
  if (days.length === 0) return <p className="m-0 text-sm text-dim">{t('admin.listening.noDays')}</p>
  const oldestFirst = [...days].reverse()
  const skipped = oldestFirst.filter(missing).length
  return (
    <figure className="m-0 flex flex-col gap-1">
      <div
        className="flex h-12 items-end gap-0.5"
        role="img"
        aria-label={t('admin.listening.strip', { count: oldestFirst.length, missing: skipped })}
      >
        {oldestFirst.map(entry => (
          <div
            key={entry.day}
            className={!entry.closed ? 'w-2 rounded-xs bg-line-2' : missing(entry) ? 'w-2 rounded-xs bg-warn' : 'w-2 rounded-xs bg-accent'}
            style={{ height: `${String(Math.max(4, Math.round(covered(entry) * 100)))}%` }}
            title={t('admin.listening.dayIs', {
              day: day(entry.day),
              covered: Math.round(covered(entry) * 100),
              listeners: entry.listeners === null ? '—' : number(entry.listeners),
            })}
          />
        ))}
      </div>
      <figcaption className="text-xs text-dim">
        {t('admin.listening.stripCaption', { from: day(oldestFirst[0]?.day ?? ''), to: day(oldestFirst.at(-1)?.day ?? ''), missing: skipped })}
      </figcaption>
    </figure>
  )
}

function Runs({ health }: { health: Health }) {
  const { t } = useTranslation()
  const { number, moment } = useFormat()
  if (health.runs.length === 0) return null
  return (
    <Table
      caption={t('admin.feed.runs')}
      head={[t('admin.columns.source'), t('admin.columns.started'), t('admin.columns.took'), t('admin.columns.read'), t('admin.columns.kept'), t('admin.columns.error')]}
      rows={health.runs.map(run => {
        const seconds = took(run)
        return [
          shortSource(run.source),
          moment(run.started_at),
          seconds === null ? t('admin.feed.running') : t('admin.feed.seconds', { count: seconds }),
          run.rows_read === null ? '—' : number(run.rows_read),
          run.rows_kept === null ? '—' : number(run.rows_kept),
          run.error ?? '',
        ]
      })}
    />
  )
}

function Table({ caption, head, rows }: { caption: string; head: string[]; rows: string[][] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-left text-xs">
        <caption className="pb-1 text-left text-xs text-dim">{caption}</caption>
        <thead>
          <tr className="border-b border-line text-dim">
            {head.map(cell => (
              <th key={cell} scope="col" className="py-1 pr-3 font-medium">
                {cell}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => (
            // Rows have no identity of their own beyond their place: a run
            // or an import listed twice is two rows.
            <tr key={`${String(index)}-${row[0] ?? ''}`} className="border-b border-line/60 align-top text-text">
              {row.map((cell, at) => (
                <td key={`${String(at)}-${cell}`} className="py-1 pr-3 break-words">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
