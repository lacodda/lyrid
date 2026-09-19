import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { forgotPassword, logIn, logOut, register, resendConfirmation, type Me } from '@/account'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { panelVariants } from '@/components/ui/panel'
import { PasswordField } from '@/components/ui/password-field'
import { Truncate } from '@/components/ui/truncate'

/**
 * Signing in, signing up, and signing out.
 *
 * The panel is small on purpose. The sky is the product and it works without
 * an account; this is a door beside it, not a gate in front of it — which is
 * why a visitor sees "sign in" rather than a form they must pass.
 *
 * The form asks for two things and nothing else. It used to ask for a mode as
 * well, and that question is gone (decision of 2026-09-11): only one of the
 * two modes is built, so the choice was between a thing and a description of a
 * thing. It returns with the fog (v0.22), asked once, of everyone.
 */

interface Props {
  me: Me | null
  onSignedIn: (me: Me) => void
  onSignedOut: () => void
  onCharter: () => void
}

export function AccountPanel({ me, onSignedIn, onSignedOut, onCharter }: Props) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)

  if (me) {
    return (
      <div className="flex flex-col items-start gap-1.5">
        <div className="flex items-center gap-2">
          <Truncate className="max-w-40 text-xs text-dim">{me.email}</Truncate>
          <Button
            size="sm"
            onClick={() => {
              void logOut().then(onSignedOut, onSignedOut)
            }}
          >
            {t('account.signOut')}
          </Button>
        </div>
        {!me.email_confirmed && <Unconfirmed />}
        <CharterLink onCharter={onCharter} />
      </div>
    )
  }

  if (!open) {
    return (
      <div className="flex flex-col items-start gap-1.5">
        <Button size="sm" onClick={() => setOpen(true)}>
          {t('account.signIn')}
        </Button>
        <CharterLink onCharter={onCharter} />
      </div>
    )
  }

  return <AccountForm onSignedIn={onSignedIn} onClose={() => setOpen(false)} onCharter={onCharter} />
}

/** The quiet link to the charter, in all three states of the panel. */
function CharterLink({ onCharter }: { onCharter: () => void }) {
  const { t } = useTranslation()
  return (
    <button type="button" className={QUIET} onClick={onCharter}>
      {t('account.charterLink')}
    </button>
  )
}

/** The aside that is a link in everything but element: it changes what is on
 * screen rather than where the browser is, so it is a button that reads like
 * a link. Written once because three of them would drift. */
const QUIET = 'cursor-pointer text-2xs text-faint underline-offset-2 hover:text-dim hover:underline'

/**
 * The one thing an unconfirmed address actually costs, said where it costs it.
 *
 * Not a banner over the whole app: the account works, and nagging about a
 * letter on every screen would be charging for something that has not been
 * withheld. What is withheld is the password reset, so that is what it says.
 */
function Unconfirmed() {
  const { t } = useTranslation()
  const [sent, setSent] = useState<'idle' | 'sending' | 'sent' | 'failed'>('idle')

  const label =
    sent === 'sent'
      ? t('account.resent')
      : sent === 'failed'
        ? t('account.resendFailed')
        : sent === 'sending'
          ? t('account.working')
          : t('account.resend')

  return (
    <div className="max-w-56 text-2xs text-warn">
      <span>{t('account.unconfirmed')}</span>{' '}
      <button
        type="button"
        className="cursor-pointer underline underline-offset-2 disabled:no-underline disabled:opacity-60"
        disabled={sent === 'sending' || sent === 'sent'}
        onClick={() => {
          setSent('sending')
          resendConfirmation().then(
            () => setSent('sent'),
            () => setSent('failed')
          )
        }}
      >
        {label}
      </button>
    </div>
  )
}

type Tab = 'new' | 'in' | 'forgot'

function AccountForm({ onSignedIn, onClose, onCharter }: { onSignedIn: (me: Me) => void; onClose: () => void; onCharter: () => void }) {
  const { t } = useTranslation()
  const [tab, setTab] = useState<Tab>('new')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const go = (next: Tab) => {
    setTab(next)
    setError(null)
    setNote(null)
  }

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError(null)
    setNote(null)

    if (tab === 'forgot') {
      forgotPassword(email).then(
        () => {
          // The same words whichever it was. The server answers identically
          // on purpose -- so that this form cannot be used to ask whether
          // someone has an account here -- and an interface that said "sent!"
          // only for real addresses would give away what the server refused
          // to.
          setNote(t('account.forgotSent'))
          setBusy(false)
        },
        (failure: unknown) => {
          setError(failure instanceof Error ? failure.message : t('account.failed'))
          setBusy(false)
        }
      )
      return
    }

    const attempt = tab === 'new' ? register(email, password) : logIn(email, password)
    attempt.then(
      me => {
        onSignedIn(me)
      },
      (failure: unknown) => {
        // The server's own words: it wrote them for a person to read.
        setError(failure instanceof Error ? failure.message : t('account.failed'))
        setBusy(false)
      }
    )
  }

  const submitLabel = busy
    ? t('account.working')
    : tab === 'new'
      ? t('account.createAccount')
      : tab === 'in'
        ? t('account.signIn')
        : t('account.sendWayBack')

  return (
    // The panel's own classes on the `<form>` rather than a Panel wrapping it:
    // a form is the element here, and a div between it and its fields would be
    // a box that exists only to be styled.
    <form onSubmit={submit} className={cn(panelVariants(), 'glass flex w-64 flex-col gap-2.5 p-3')}>
      <div className="flex items-center gap-1">
        {/* Two buttons rather than a tab list: these are not two views of one
            thing, they are two different requests to the server, and a reader
            arriving at a form wants to see which one is about to be sent. */}
        <Button size="sm" variant={tab === 'new' ? 'soft' : 'ghost'} onClick={() => go('new')}>
          {t('account.newAccount')}
        </Button>
        <Button size="sm" variant={tab === 'in' ? 'soft' : 'ghost'} onClick={() => go('in')}>
          {t('account.signIn')}
        </Button>
        <Button variant="icon" size="icon-sm" className="ml-auto" onClick={onClose} aria-label={t('account.close')}>
          ×
        </Button>
      </div>

      <Field label={t('account.email')} labelHidden>
        <Input
          type="email"
          value={email}
          onChange={event => setEmail(event.target.value)}
          placeholder={t('account.email')}
          autoComplete="email"
          required
        />
      </Field>

      {tab !== 'forgot' && (
        <Field label={t('account.password')} labelHidden>
          <PasswordField
            value={password}
            onValueChange={setPassword}
            placeholder={t('account.password')}
            showLabel={t('account.showPassword')}
            hideLabel={t('account.hidePassword')}
            // Tells a password manager which of the two this is; without it, a
            // sign-in form gets offered a new password.
            autoComplete={tab === 'new' ? 'new-password' : 'current-password'}
            required
          />
        </Field>
      )}

      {tab === 'forgot' && <p className="text-2xs text-dim">{t('account.forgotHint')}</p>}
      {error && (
        <p role="alert" className="text-2xs text-bad">
          {error}
        </p>
      )}
      {note && <p className="text-2xs text-good">{note}</p>}

      <Button type="submit" variant="primary" size="sm" disabled={busy}>
        {submitLabel}
      </Button>

      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {tab === 'forgot' ? (
          <button type="button" className={QUIET} onClick={() => go('in')}>
            {t('account.backToSignIn')}
          </button>
        ) : (
          <button type="button" className={QUIET} onClick={() => go('forgot')}>
            {t('account.forgot')}
          </button>
        )}
        <CharterLink onCharter={onCharter} />
      </div>
    </form>
  )
}
