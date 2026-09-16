import { useEffect, useState } from 'react'

import { confirmEmail, resetPassword } from '@/account'

/**
 * The two pages a link in a letter lands on.
 *
 * They share a file because they are the same page with a different verb: a
 * token from the address, one request, and one sentence saying what happened.
 * Neither is part of the map, so neither loads the sky — a person arriving
 * from their mail client came to finish one thing, and a WebGL canvas booting
 * behind them is not part of it.
 *
 * A missing token is told apart from a rejected one. An empty `?token=` means
 * the link was cut somewhere between the letter and the browser, and saying
 * "that link has expired" for it would send someone looking for a new letter
 * that will be cut the same way.
 */

type Outcome = { state: 'working' } | { state: 'done'; message: string } | { state: 'failed'; message: string }

const NO_TOKEN = 'That link is missing its token — it was probably cut short on the way here. Copy the whole link out of the letter.'

export function ConfirmPage({ token, onClose }: { token: string; onClose: () => void }) {
  // A missing token needs no request and no effect: it is known while
  // rendering, so it is the initial state rather than something an effect
  // corrects on the frame after.
  const [outcome, setOutcome] = useState<Outcome>(token ? { state: 'working' } : { state: 'failed', message: NO_TOKEN })

  // Confirming happens on arrival, with no button in the way: the person
  // already pressed the button -- it was the link in the letter.
  useEffect(() => {
    if (!token) return
    let live = true
    confirmEmail(token).then(
      () => {
        if (live) setOutcome({ state: 'done', message: 'Your address is confirmed. If you ever lose your password, we can send you a way back in.' })
      },
      (failure: unknown) => {
        if (live) setOutcome({ state: 'failed', message: failure instanceof Error ? failure.message : 'that link could not be used' })
      }
    )
    return () => {
      live = false
    }
  }, [token])

  return (
    <Letter title="Confirming your address">
      {outcome.state === 'working' ? <p>One moment.</p> : <p className={outcome.state === 'failed' ? 'account__error' : undefined}>{outcome.message}</p>}
      <button onClick={onClose}>to the sky</button>
    </Letter>
  )
}

export function ResetPage({ token, onClose }: { token: string; onClose: () => void }) {
  const [password, setPassword] = useState('')
  const [outcome, setOutcome] = useState<Outcome | null>(null)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!token) {
      setOutcome({ state: 'failed', message: NO_TOKEN })
      return
    }
    setOutcome({ state: 'working' })
    resetPassword(token, password).then(
      () =>
        setOutcome({
          state: 'done',
          // Said plainly, because it is surprising and it is the point: a
          // reset is what you do when you think someone else has your
          // password, and leaving their session open would answer the wrong
          // half of that.
          message: 'Your password is changed, and every browser signed into this account has been signed out. Sign in again with the new one.',
        }),
      (failure: unknown) => setOutcome({ state: 'failed', message: failure instanceof Error ? failure.message : 'that link could not be used' })
    )
  }

  if (outcome?.state === 'done') {
    return (
      <Letter title="Password changed">
        <p>{outcome.message}</p>
        <button onClick={onClose}>to the sky</button>
      </Letter>
    )
  }

  return (
    <Letter title="Choose a new password">
      <form className="account account__form" onSubmit={submit}>
        <input
          type="password"
          value={password}
          onChange={event => setPassword(event.target.value)}
          placeholder="new password"
          autoComplete="new-password"
          required
        />
        {outcome?.state === 'failed' && <p className="account__error">{outcome.message}</p>}
        <button type="submit" disabled={outcome?.state === 'working'}>
          {outcome?.state === 'working' ? 'one moment' : 'change it'}
        </button>
      </form>
      <button className="page__close" onClick={onClose}>
        to the sky
      </button>
    </Letter>
  )
}

function Letter({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="page">
      <article className="page__body page__narrow">
        <h1>{title}</h1>
        {children}
      </article>
    </main>
  )
}
