import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { saveProfile, type Me } from '@/account'
import { Button } from '@/components/ui/button'
import { Dialog, DialogBody, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { SectionLabel } from '@/components/ui/panel'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { useLanguage } from '@/lib/language'
import { count } from '@/metrics'
import {
  askForLetter,
  bringLastfm,
  fetchHeardStars,
  fetchYouth,
  forgetLastfm,
  stopLetter,
  youthFrom,
  type Brought,
  type Listening,
  type PlacedStar,
  type Youth,
} from '@/scrobbling'

/**
 * Your life on the sky: the home your listening gathers around, the stars of
 * your youth, a Last.fm history brought in a file, and the weekly letter.
 *
 * A dialog rather than more lines in the corner: the corner stack is bounded
 * by the window and already yields to the list of nearby stars, and all four
 * of these are visited now and then rather than watched. The corner keeps one
 * line of it -- the home -- and a way in.
 */

/** How many of a youth's stars are named in the dialog; the rest are on the sky. */
const NAMED = 8

/** Where people get their scrobbles out of Last.fm as CSV. */
const EXPORT_TOOL = 'https://lastfm.ghan.nl/export/'

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  me: Me
  listening: Listening
  onMe: (me: Me) => void
  onListening: (listening: Listening) => void
  /** Flies to the home star and opens it. */
  onHome: (star: PlacedStar) => void
  /** Rings a youth's stars on the sky and frames them. */
  onShowYouth: (youth: Youth) => void
}

export function YourSky({ open, onOpenChange, me, listening, onMe, onListening, onHome, onShowYouth }: Props) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup size="md">
        <DialogHeader>
          <DialogTitle>{t('yourSky.heading')}</DialogTitle>
          <DialogDescription>{t('yourSky.description')}</DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-5">
          <Home home={listening.home} onHome={onHome} />
          <YouthSection me={me} listening={listening} onMe={onMe} onShowYouth={onShowYouth} />
          <LastfmSection listening={listening} onListening={onListening} />
          <LetterSection me={me} onMe={onMe} />
        </DialogBody>
      </DialogPopup>
    </Dialog>
  )
}

function Home({ home, onHome }: { home: PlacedStar | null; onHome: (star: PlacedStar) => void }) {
  const { t } = useTranslation()
  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="your-sky-home">
      <SectionLabel id="your-sky-home">{t('yourSky.home.heading')}</SectionLabel>
      {home ? (
        <>
          <p className="m-0 text-sm text-text">{t('yourSky.home.is', { name: home.name })}</p>
          <p className="m-0 text-xs text-dim">{t('yourSky.home.how')}</p>
          <Button
            size="sm"
            className="self-start"
            onClick={() => {
              count('home_flown')
              onHome(home)
            }}
          >
            {t('yourSky.home.fly')}
          </Button>
        </>
      ) : (
        <p className="m-0 text-xs text-dim">{t('yourSky.home.none')}</p>
      )}
    </section>
  )
}

/** The year as a whole number between the bounds, or `null` for anything else. */
export function readYear(text: string, now: number): number | null {
  if (!/^\d{4}$/.test(text.trim())) return null
  const year = Number(text.trim())
  return year >= 1900 && year <= now ? year : null
}

function YouthSection({
  me,
  listening,
  onMe,
  onShowYouth,
}: {
  me: Me
  listening: Listening
  onMe: (me: Me) => void
  onShowYouth: (youth: Youth) => void
}) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const thisYear = new Date().getFullYear()
  const [typed, setTyped] = useState(me.born === null ? '' : String(me.born))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Kept with the year it answers, so a year changed while the answer was out
  // never shows another youth's stars under it.
  const [youth, setYouth] = useState<{ born: number; youth: Youth } | null>(null)
  const [heard, setHeard] = useState<Set<number>>(() => new Set())

  useEffect(() => {
    if (me.born === null || youthFrom(me.born) > thisYear) return
    const born = me.born
    const abort = new AbortController()
    fetchYouth(youthFrom(born), abort.signal).then(
      next => setYouth({ born, youth: next }),
      () => undefined
    )
    return () => abort.abort()
  }, [me.born, thisYear])

  // Which of them the listening has heard: the same rings "show on the sky"
  // draws, asked for only when there is listening to ask about.
  useEffect(() => {
    if (listening.listens === 0) return
    const abort = new AbortController()
    fetchHeardStars(abort.signal).then(
      stars => setHeard(new Set(stars.map(([id]) => id))),
      () => undefined
    )
    return () => abort.abort()
  }, [listening.listens])

  const save = (born: number | null) => {
    setSaving(true)
    setError(null)
    saveProfile({ born }).then(
      next => {
        setSaving(false)
        onMe(next)
      },
      (failure: unknown) => {
        setSaving(false)
        setError(failure instanceof Error ? failure.message : t('account.failed'))
      }
    )
  }

  const year = readYear(typed, thisYear)
  const shown = youth && youth.born === me.born ? youth.youth : null
  const heardCount = shown ? shown.stars.filter(star => heard.has(star.id)).length : 0

  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="your-sky-youth">
      <SectionLabel id="your-sky-youth">{t('yourSky.youth.heading')}</SectionLabel>
      <p className="m-0 text-xs text-dim">{t('yourSky.youth.how')}</p>
      <form
        className="flex items-end gap-1.5"
        onSubmit={event => {
          event.preventDefault()
          if (year !== null) save(year)
        }}
      >
        <Field label={t('yourSky.youth.born')} className="w-32" error={typed !== '' && year === null ? t('yourSky.youth.notAYear') : undefined}>
          <Input inputMode="numeric" value={typed} onChange={event => setTyped(event.target.value)} placeholder="1983" maxLength={4} />
        </Field>
        <Button type="submit" size="sm" disabled={saving || year === null || year === me.born}>
          {saving ? t('account.working') : t('yourSky.youth.save')}
        </Button>
        {me.born !== null && (
          <Button
            variant="link"
            className="mb-2 text-xs"
            disabled={saving}
            onClick={() => {
              setTyped('')
              save(null)
            }}
          >
            {t('yourSky.youth.forget')}
          </Button>
        )}
      </form>
      {error && (
        <p role="alert" className="m-0 text-xs text-bad">
          {error}
        </p>
      )}
      {me.born !== null && youthFrom(me.born) > thisYear && <p className="m-0 text-xs text-dim">{t('yourSky.youth.ahead')}</p>}
      {me.born !== null && youthFrom(me.born) <= thisYear && !shown && <Spinner label={t('account.working')} />}
      {shown && shown.stars.length === 0 && <p className="m-0 text-xs text-dim">{t('yourSky.youth.empty', { from: shown.from, to: shown.to })}</p>}
      {shown && shown.stars.length > 0 && (
        <>
          <p className="m-0 text-sm text-text">
            <span className="text-dim">{t('yourSky.youth.years', { from: shown.from, to: shown.to })}</span>{' '}
            {shown.stars.slice(0, NAMED).map((star, index) => (
              <span key={star.id}>
                {index > 0 && ', '}
                <span className={heard.has(star.id) ? 'text-light' : undefined}>{star.name}</span>
              </span>
            ))}
            {shown.stars.length > NAMED && <span className="text-dim"> {t('yourSky.youth.more', { number: (shown.stars.length - NAMED).toLocaleString(resolved) })}</span>}
          </p>
          {listening.listens > 0 && (
            <p className="m-0 text-xs text-dim">{t('yourSky.youth.heard', { count: heardCount, number: heardCount.toLocaleString(resolved), of: shown.stars.length.toLocaleString(resolved) })}</p>
          )}
          <Button
            size="sm"
            className="self-start"
            onClick={() => {
              count('youth_shown')
              onShowYouth(shown)
            }}
          >
            {t('yourSky.youth.show')}
          </Button>
        </>
      )}
    </section>
  )
}

function LastfmSection({ listening, onListening }: { listening: Listening; onListening: (listening: Listening) => void }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const input = useRef<HTMLInputElement>(null)
  const [busy, setBusy] = useState<'bring' | 'forget' | null>(null)
  const [brought, setBrought] = useState<Brought | null>(null)
  const [error, setError] = useState<string | null>(null)
  const number = (value: number) => value.toLocaleString(resolved)

  const bring = (file: File) => {
    setBusy('bring')
    setError(null)
    setBrought(null)
    bringLastfm(file).then(
      result => {
        count('lastfm_brought')
        setBusy(null)
        setBrought(result)
        onListening(result.listening)
      },
      (failure: unknown) => {
        setBusy(null)
        setError(failure instanceof Error ? failure.message : t('account.failed'))
      }
    )
  }

  const forget = () => {
    setBusy('forget')
    setError(null)
    setBrought(null)
    forgetLastfm().then(
      next => {
        setBusy(null)
        onListening(next)
      },
      (failure: unknown) => {
        setBusy(null)
        setError(failure instanceof Error ? failure.message : t('account.failed'))
      }
    )
  }

  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="your-sky-lastfm">
      <SectionLabel id="your-sky-lastfm">{t('yourSky.lastfm.heading')}</SectionLabel>
      <p className="m-0 text-xs text-dim">
        {t('yourSky.lastfm.how')}{' '}
        <a className="text-accent underline underline-offset-2" href={EXPORT_TOOL} target="_blank" rel="noreferrer">
          {t('yourSky.lastfm.tool')}
        </a>
      </p>
      <input
        ref={input}
        type="file"
        accept=".csv,text/csv"
        className="sr-only"
        tabIndex={-1}
        aria-hidden="true"
        onChange={event => {
          const file = event.target.files?.[0]
          // Cleared at once, so choosing the same file again is a change.
          event.target.value = ''
          if (file) bring(file)
        }}
      />
      <div className="flex flex-wrap items-center gap-1.5">
        <Button size="sm" disabled={busy !== null} onClick={() => input.current?.click()}>
          {busy === 'bring' ? t('yourSky.lastfm.reading') : t('yourSky.lastfm.choose')}
        </Button>
        {listening.from_file > 0 && (
          <Button size="sm" variant="danger" disabled={busy !== null} onClick={forget}>
            {busy === 'forget' ? t('account.working') : t('yourSky.lastfm.forget', { count: listening.from_file, number: number(listening.from_file) })}
          </Button>
        )}
      </div>
      {brought && (
        <p className="m-0 text-xs text-good" role="status">
          {t('yourSky.lastfm.brought', {
            scrobbles: number(brought.scrobbles),
            listens: number(brought.listens),
            named: number(brought.named),
            opened: number(brought.opened),
          })}
          {brought.unreadable > 0 && ` ${t('yourSky.lastfm.unreadable', { count: brought.unreadable, number: number(brought.unreadable) })}`}
        </p>
      )}
      {error && (
        <p role="alert" className="m-0 text-xs text-bad">
          {error}
        </p>
      )}
    </section>
  )
}

function LetterSection({ me, onMe }: { me: Me; onMe: (me: Me) => void }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // The letter is written in the language lyrid is read in now; asking again
  // in another language moves it there.
  const language = resolved.startsWith('ru') ? 'ru' : 'en'

  const change = (wanted: boolean) => {
    setBusy(true)
    setError(null)
    const request = wanted ? askForLetter(language) : stopLetter()
    request.then(
      ({ letter }) => {
        if (wanted) count('letter_asked')
        setBusy(false)
        onMe({ ...me, letter })
      },
      (failure: unknown) => {
        setBusy(false)
        setError(failure instanceof Error ? failure.message : t('account.failed'))
      }
    )
  }

  return (
    <section className="flex flex-col gap-1.5" aria-labelledby="your-sky-letter">
      <SectionLabel id="your-sky-letter">{t('yourSky.letter.heading')}</SectionLabel>
      <Switch checked={me.letter !== null} disabled={busy || (!me.email_confirmed && me.letter === null)} onCheckedChange={change}>
        {t('yourSky.letter.ask')}
      </Switch>
      <p className="m-0 text-xs text-dim">
        {!me.email_confirmed && me.letter === null
          ? t('yourSky.letter.confirmFirst')
          : me.letter && me.letter.language !== language
            ? t('yourSky.letter.otherLanguage')
            : t('yourSky.letter.how')}
      </p>
      {me.letter && me.letter.language !== language && (
        <Button size="sm" className="self-start" disabled={busy} onClick={() => change(true)}>
          {t('yourSky.letter.moveHere')}
        </Button>
      )}
      {error && (
        <p role="alert" className="m-0 text-xs text-bad">
          {error}
        </p>
      )}
    </section>
  )
}
