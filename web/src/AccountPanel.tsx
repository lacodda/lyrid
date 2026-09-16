import { useState } from 'react'

import { forgotPassword, logIn, logOut, register, resendConfirmation, type Me } from '@/account'

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
  const [open, setOpen] = useState(false)

  if (me) {
    return (
      <div className="account">
        <span className="account__who" title={me.email}>
          {me.email}
        </span>
        {!me.email_confirmed && <Unconfirmed />}
        <button
          onClick={() => {
            void logOut().then(onSignedOut, onSignedOut)
          }}
        >
          sign out
        </button>
        <button className="account__quiet" onClick={onCharter}>
          what lyrid keeps
        </button>
      </div>
    )
  }

  if (!open) {
    return (
      <div className="account">
        <button onClick={() => setOpen(true)}>sign in</button>
        <button className="account__quiet" onClick={onCharter}>
          what lyrid keeps
        </button>
      </div>
    )
  }

  return <AccountForm onSignedIn={onSignedIn} onClose={() => setOpen(false)} onCharter={onCharter} />
}

/**
 * The one thing an unconfirmed address actually costs, said where it costs it.
 *
 * Not a banner over the whole app: the account works, and nagging about a
 * letter on every screen would be charging for something that has not been
 * withheld. What is withheld is the password reset, so that is what it says.
 */
function Unconfirmed() {
  const [sent, setSent] = useState<'idle' | 'sending' | 'sent' | 'failed'>('idle')

  return (
    <span className="account__unconfirmed">
      <span>Address not confirmed — we cannot send you a password reset until it is.</span>
      <button
        className="account__quiet"
        disabled={sent === 'sending' || sent === 'sent'}
        onClick={() => {
          setSent('sending')
          resendConfirmation().then(
            () => setSent('sent'),
            () => setSent('failed')
          )
        }}
      >
        {sent === 'sent' ? 'letter sent' : sent === 'failed' ? 'could not send — try later' : sent === 'sending' ? 'one moment' : 'send it again'}
      </button>
    </span>
  )
}

type Tab = 'new' | 'in' | 'forgot'

function AccountForm({ onSignedIn, onClose, onCharter }: { onSignedIn: (me: Me) => void; onClose: () => void; onCharter: () => void }) {
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
          setNote('If we know that address, a way back in is on its way to it.')
          setBusy(false)
        },
        (failure: unknown) => {
          setError(failure instanceof Error ? failure.message : 'something went wrong')
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
        setError(failure instanceof Error ? failure.message : 'something went wrong')
        setBusy(false)
      }
    )
  }

  return (
    <form className="account account__form" onSubmit={submit}>
      <div className="account__tabs">
        <button type="button" className={tab === 'new' ? 'account__on' : ''} onClick={() => go('new')}>
          new account
        </button>
        <button type="button" className={tab === 'in' ? 'account__on' : ''} onClick={() => go('in')}>
          sign in
        </button>
        <button type="button" className="account__close" onClick={onClose} aria-label="close">
          ×
        </button>
      </div>

      <input
        type="email"
        value={email}
        onChange={event => setEmail(event.target.value)}
        placeholder="email"
        autoComplete="email"
        required
      />
      {tab !== 'forgot' && (
        <input
          type="password"
          value={password}
          onChange={event => setPassword(event.target.value)}
          placeholder="password"
          // Tells a password manager which of the two this is; without it, a
          // sign-in form gets offered a new password.
          autoComplete={tab === 'new' ? 'new-password' : 'current-password'}
          required
        />
      )}

      {tab === 'forgot' && <p className="account__hint">We will send a way back in to that address, if we know it.</p>}
      {error && <p className="account__error">{error}</p>}
      {note && <p className="account__note">{note}</p>}

      <button type="submit" disabled={busy}>
        {busy ? 'one moment' : tab === 'new' ? 'create account' : tab === 'in' ? 'sign in' : 'send me a way back'}
      </button>

      <div className="account__asides">
        {tab === 'forgot' ? (
          <button type="button" className="account__quiet" onClick={() => go('in')}>
            back to signing in
          </button>
        ) : (
          <button type="button" className="account__quiet" onClick={() => go('forgot')}>
            forgot your password?
          </button>
        )}
        <button type="button" className="account__quiet" onClick={onCharter}>
          what lyrid keeps
        </button>
      </div>
    </form>
  )
}
