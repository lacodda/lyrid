import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { count } from '@/metrics'
import { Button } from '@/components/ui/button'
import {
  ConfirmDialog,
  ConfirmDialogActions,
  ConfirmDialogClose,
  ConfirmDialogDescription,
  ConfirmDialogHeader,
  ConfirmDialogPopup,
  ConfirmDialogTitle,
} from '@/components/ui/confirm-dialog'
import { Dialog, DialogActions, DialogBody, DialogClose, DialogDescription, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { PasswordField } from '@/components/ui/password-field'
import { useLanguage } from '@/lib/language'
import { ago, linkListenBrainz, readNow, TOKEN_PAGE, unlinkListenBrainz, type Listening, type ReadNow } from '@/scrobbling'

/**
 * Real listening, brought to the sky: linking ListenBrainz, and what the
 * listening has opened and gathered since.
 *
 * Under the account in the corner stack, because it is the account's: it
 * appears only once someone is signed in, and it is the one piece of the
 * account that changes on its own -- the server reads ListenBrainz every
 * quarter of an hour whether or not anyone is looking.
 *
 * Bare lines rather than a panel, like the account above it. The stack is
 * bounded by the window and the list of nearby stars is the piece that yields
 * to everything else in it; a glass panel here, with its padding, took that
 * list down to nothing at 1200x750 -- measured, not guessed. The form that
 * links an account is a dialog for the same reason: it is read once, and it
 * has no business occupying the corner for good.
 */

interface Props {
  /** The summary, or `null` while it is first being read. */
  listening: Listening | null
  onChange: (listening: Listening) => void
  /** Rings every heard star on the sky. */
  onShowOnSky: () => void
  /** Opens a star the listening named. */
  onOpen: (id: number) => void
  className?: string
}

export function ListeningPanel({ listening, onChange, onShowOnSky, onOpen, className }: Props) {
  if (!listening) return null
  if (!listening.link) return <Unlinked className={className} listening={listening} onChange={onChange} onShowOnSky={onShowOnSky} />
  return <Linked className={className} listening={listening} onChange={onChange} onShowOnSky={onShowOnSky} onOpen={onOpen} />
}

const QUIET = 'cursor-pointer text-2xs text-faint underline-offset-2 hover:text-dim hover:underline disabled:cursor-default disabled:opacity-60'

/** An amount of light, in the currency's own colour. */
function Light({ value }: { value: number }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  return <span className="font-medium text-light">{t('listening.light', { number: value.toLocaleString(resolved) })}</span>
}

/** Light, stars and listens on one line. */
function Tally({ listening }: { listening: Listening }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  return (
    <p className="m-0 flex flex-wrap items-baseline gap-x-2 text-xs text-dim">
      <Light value={listening.light} />
      <span>{t('listening.stars', { count: listening.stars, number: listening.stars.toLocaleString(resolved) })}</span>
      <span>{t('listening.listens', { count: listening.listens, number: listening.listens.toLocaleString(resolved) })}</span>
    </p>
  )
}

/** "5 minutes ago" in the reader's language, or `justNow` under a minute. */
function since(then: Date, now: Date, language: string, justNow: string): string {
  const [value, unit] = ago(then, now)
  if (value === 0) return justNow
  return new Intl.RelativeTimeFormat(language, { numeric: 'auto' }).format(value, unit)
}

/**
 * An account with nothing linked: what linking does, and the door to it.
 *
 * An account that was linked once and has since been unlinked still shows
 * what it gathered -- unlinking stops the reading, it does not take the light
 * back.
 */
function Unlinked({
  listening,
  onChange,
  onShowOnSky,
  className,
}: {
  listening: Listening
  onChange: (listening: Listening) => void
  onShowOnSky: () => void
  className?: string
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <div className={cn('flex max-w-64 flex-col items-start gap-1', className)}>
      {listening.listens > 0 && <Tally listening={listening} />}
      <div className="flex flex-wrap items-center gap-1.5">
        <Button size="sm" onClick={() => setOpen(true)}>
          {t('listening.link')}
        </Button>
        {listening.stars > 0 && (
          <Button size="sm" onClick={onShowOnSky}>
            {t('listening.showOnSky')}
          </Button>
        )}
      </div>
      <p className="m-0 text-2xs text-faint">{t('listening.pitch')}</p>
      <LinkDialog
        open={open}
        onOpenChange={setOpen}
        onLinked={next => {
          count('listening_linked')
          setOpen(false)
          onChange(next)
        }}
      />
    </div>
  )
}

/** Linking, as a dialog: what it does, where the token is, and the field. */
function LinkDialog({ open, onOpenChange, onLinked }: { open: boolean; onOpenChange: (open: boolean) => void; onLinked: (listening: Listening) => void }) {
  const { t } = useTranslation()
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError(null)
    linkListenBrainz(token).then(
      next => {
        // Not kept in this page any longer than the request needed it.
        setToken('')
        setBusy(false)
        onLinked(next)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : t('account.failed'))
        setBusy(false)
      }
    )
  }

  return (
    <Dialog
      open={open}
      onOpenChange={next => {
        // Closing forgets what was typed: a token left in a closed form is a
        // token left lying about.
        if (!next) {
          setToken('')
          setError(null)
        }
        onOpenChange(next)
      }}
    >
      <DialogPopup size="sm">
        <form onSubmit={submit} className="flex min-h-0 flex-col">
          <DialogHeader>
            <DialogTitle>{t('listening.linkHeading')}</DialogTitle>
            <DialogDescription>{t('listening.linkBody')}</DialogDescription>
          </DialogHeader>
          <DialogBody className="flex flex-col gap-2">
            <a className="text-sm text-accent underline underline-offset-2" href={TOKEN_PAGE} target="_blank" rel="noreferrer">
              {t('listening.tokenWhere')}
            </a>
            <Field label={t('listening.token')}>
              <PasswordField
                value={token}
                onValueChange={setToken}
                showLabel={t('listening.showToken')}
                hideLabel={t('listening.hideToken')}
                // A token is not a password a manager should offer to save.
                autoComplete="off"
                required
              />
            </Field>
            {error && (
              <p role="alert" className="m-0 text-sm text-bad">
                {error}
              </p>
            )}
            <p className="m-0 text-xs text-faint">{t('listening.tokenKept')}</p>
          </DialogBody>
          <DialogActions>
            <DialogClose render={<Button>{t('listening.cancel')}</Button>} />
            <Button type="submit" variant="primary" disabled={busy || token.trim() === ''}>
              {busy ? t('account.working') : t('listening.linkAction')}
            </Button>
          </DialogActions>
        </form>
      </DialogPopup>
    </Dialog>
  )
}

function Linked({
  listening,
  onChange,
  onShowOnSky,
  onOpen,
  className,
}: {
  listening: Listening
  onChange: (listening: Listening) => void
  onShowOnSky: () => void
  onOpen: (id: number) => void
  className?: string
}) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const [busy, setBusy] = useState<'read' | 'unlink' | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  // The "read five minutes ago" line has to move on its own while the panel
  // sits there, or it lies by the next quarter hour.
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 30_000)
    return () => window.clearInterval(timer)
  }, [])

  // `link` is not null here; the parent only renders this for a linked account.
  const link = listening.link
  if (!link) return null

  const status = link.failure
    ? t(`listening.failure.${link.failure}`, { name: link.name })
    : link.read_at
      ? t('listening.readAt', { name: link.name, when: since(new Date(link.read_at), now, resolved, t('listening.justNow')) })
      : t('listening.notReadYet', { name: link.name })

  const read = () => {
    setBusy('read')
    setNote(null)
    setError(null)
    readNow().then(
      (result: ReadNow) => {
        onChange(result.listening)
        setNote(
          result.listening.link?.failure
            ? null
            : result.listens === 0
              ? t('listening.nothingNew')
              : t('listening.gathered', {
                  number: result.listens.toLocaleString(resolved),
                  light: result.light.toLocaleString(resolved),
                  opened: result.opened.toLocaleString(resolved),
                })
        )
        setBusy(null)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : t('account.failed'))
        setBusy(null)
      }
    )
  }

  const unlink = () => {
    setAsking(false)
    setBusy('unlink')
    setError(null)
    unlinkListenBrainz().then(
      next => {
        setBusy(null)
        onChange(next)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : t('account.failed'))
        setBusy(null)
      }
    )
  }

  return (
    <section className={cn('flex max-w-64 flex-col items-start gap-1', className)} aria-label={t('listening.heading')}>
      <Tally listening={listening} />

      <p className={cn('m-0 text-2xs', link.failure ? 'text-warn' : 'text-faint')}>{status}</p>

      {listening.listens === 0 && !link.failure && <p className="m-0 text-2xs text-dim">{t('listening.fromNowOn')}</p>}

      {listening.recent.length > 0 && (
        // The latest openings, by name: the listening's news, and a way to
        // fly to it. Three, on one line -- the rest are a press of "show on
        // the sky" away.
        <p className="m-0 line-clamp-2 text-2xs text-dim">
          {t('listening.latest')}{' '}
          {listening.recent.slice(0, 3).map((opening, index) => (
            <span key={opening.id}>
              {index > 0 && ', '}
              <button type="button" className="cursor-pointer text-text underline-offset-2 hover:underline" onClick={() => onOpen(opening.id)}>
                {opening.name}
              </button>
            </span>
          ))}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-x-1.5 gap-y-1">
        <Button size="sm" onClick={read} disabled={busy !== null}>
          {busy === 'read' ? t('account.working') : t('listening.readNow')}
        </Button>
        {listening.stars > 0 && (
          <Button size="sm" onClick={onShowOnSky}>
            {t('listening.showOnSky')}
          </Button>
        )}
        <button type="button" className={QUIET} onClick={() => setAsking(true)} disabled={busy !== null}>
          {busy === 'unlink' ? t('account.working') : t('listening.unlink')}
        </button>
      </div>

      {note && <p className="m-0 text-2xs text-good">{note}</p>}
      {error && (
        <p role="alert" className="m-0 text-2xs text-bad">
          {error}
        </p>
      )}

      <ConfirmDialog open={asking} onOpenChange={setAsking}>
        <ConfirmDialogPopup>
          <ConfirmDialogHeader>
            <ConfirmDialogTitle>{t('listening.unlinkTitle', { name: link.name })}</ConfirmDialogTitle>
            <ConfirmDialogDescription>{t('listening.unlinkBody')}</ConfirmDialogDescription>
          </ConfirmDialogHeader>
          <ConfirmDialogActions>
            <ConfirmDialogClose render={<Button>{t('listening.keepLinked')}</Button>} />
            <Button variant="danger" onClick={unlink}>
              {t('listening.unlinkAction')}
            </Button>
          </ConfirmDialogActions>
        </ConfirmDialogPopup>
      </ConfirmDialog>
    </section>
  )
}
