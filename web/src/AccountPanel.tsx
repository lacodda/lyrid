import { useState } from 'react'

import { MODES, logIn, logOut, register, type Me, type Mode } from '@/account'

/**
 * Signing in, signing up, and signing out.
 *
 * The panel is small on purpose. The sky is the product and it works without
 * an account; this is a door beside it, not a gate in front of it — which is
 * why a visitor sees "sign in" rather than a form they must pass.
 *
 * The one thing the form insists on is the mode. It is chosen here and never
 * again (Vision, principle 5), so the choice is put in front of the person
 * making it, with what each mode means written beside it, rather than left as
 * a default they discover afterwards.
 */

interface Props {
  me: Me | null
  onSignedIn: (me: Me) => void
  onSignedOut: () => void
}

export function AccountPanel({ me, onSignedIn, onSignedOut }: Props) {
  const [open, setOpen] = useState(false)

  if (me) {
    return (
      <div className="account">
        <span className="account__who" title={me.email}>
          {me.email}
        </span>
        <span className="account__mode">{me.mode}</span>
        <button
          onClick={() => {
            void logOut().then(onSignedOut, onSignedOut)
          }}
        >
          sign out
        </button>
      </div>
    )
  }

  if (!open) {
    return (
      <div className="account">
        <button onClick={() => setOpen(true)}>sign in</button>
      </div>
    )
  }

  return <AccountForm onSignedIn={onSignedIn} onClose={() => setOpen(false)} />
}

/** What each mode is, in the words the person choosing needs. */
const MODE_BLURB: Record<Mode, string> = {
  create: 'Everything open. The sky as an instrument: look at anything, any time.',
  explore: 'The sky starts dark. You uncover it by listening — a ship, fuel, and somewhere to go.',
}

function AccountForm({ onSignedIn, onClose }: { onSignedIn: (me: Me) => void; onClose: () => void }) {
  const [isNew, setIsNew] = useState(true)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [mode, setMode] = useState<Mode>('create')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError(null)
    const attempt = isNew ? register(email, password, mode) : logIn(email, password)
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
        <button type="button" className={isNew ? 'account__on' : ''} onClick={() => setIsNew(true)}>
          new account
        </button>
        <button type="button" className={isNew ? '' : 'account__on'} onClick={() => setIsNew(false)}>
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
      <input
        type="password"
        value={password}
        onChange={event => setPassword(event.target.value)}
        placeholder="password"
        // Tells a password manager which of the two this is; without it, a
        // sign-in form gets offered a new password.
        autoComplete={isNew ? 'new-password' : 'current-password'}
        required
      />

      {isNew && (
        <fieldset className="account__modes">
          <legend>chosen once, and never again</legend>
          {MODES.map(option => (
            <label key={option} className={option === mode ? 'account__on' : ''}>
              <input type="radio" name="mode" value={option} checked={option === mode} onChange={() => setMode(option)} />
              <span className="account__mode-name">{option}</span>
              <span className="account__mode-blurb">{MODE_BLURB[option]}</span>
            </label>
          ))}
        </fieldset>
      )}

      {error && <p className="account__error">{error}</p>}

      <button type="submit" disabled={busy}>
        {busy ? 'one moment' : isNew ? 'create account' : 'sign in'}
      </button>
    </form>
  )
}
