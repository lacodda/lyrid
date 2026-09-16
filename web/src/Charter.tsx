import { useState } from 'react'

import { deleteAccount, exportAccount, type Me } from '@/account'
import { count } from '@/metrics'

/**
 * The privacy charter: a promise, and the two buttons that make it checkable.
 *
 * A page of prose with an email address at the bottom is not a promise, it is
 * an intention. What makes this one different is that both halves of it are
 * pressable from here: the export hands back the rows themselves, and the
 * deletion is immediate and total rather than a request someone processes.
 *
 * Written before the scrobbling (v0.17) rather than after, deliberately. The
 * moment this service starts reading what someone actually listens to is the
 * wrong moment to be deciding what it promises about it.
 */

interface Props {
  me: Me | null
  onSignedOut: () => void
  onClose: () => void
}

export function Charter({ me, onSignedOut, onClose }: Props) {
  return (
    <main className="page">
      <article className="page__body">
        <header className="page__head">
          <h1>What lyrid keeps</h1>
          <button className="page__close" onClick={onClose}>
            back to the sky
          </button>
        </header>

        <p className="page__lede">
          The sky is public and works without an account. An account exists to remember three things across your machines, and this page is
          the whole list — not a summary of one.
        </p>

        <h2>If you have no account</h2>
        <p>
          Nothing about you is stored on the server. Your marker's shape and colour live in your own browser, and the view you are looking
          at lives in the address bar. Neither reaches us.
        </p>

        <h2>If you do</h2>
        <ul className="page__list">
          <li>
            <strong>Your address.</strong> To sign you in, and to send you a way back if you lose your password. Two letters, ever: one to
            confirm the address, one to reset a password. No announcements, no newsletter.
          </li>
          <li>
            <strong>Where you left the sky, and how you like your marked star drawn.</strong> The reason an account is worth having.
          </li>
          <li>
            <strong>Your open sessions.</strong> A random token per browser, so signing out actually ends the session rather than waiting
            for it to expire.
          </li>
        </ul>

        <h2>What is counted, and what is not</h2>
        <p>
          How often each part of lyrid gets used is counted — how many times the radio was opened on a given day, not who opened it. The
          counters are numbers that go up. There is no row per event, so there is nothing to join back to a person, and no clever query
          later can turn these into a history of what you looked at.
        </p>
        <p className="page__plain">
          What lyrid does not do: build a profile of your taste, sell or share anything with anyone, run third-party analytics or
          advertising, or track you across other sites.
        </p>

        <h2>Whose sky is it</h2>
        <p>
          The map itself is built from open data — MusicBrainz, ListenBrainz, Discogs, Wikidata and Wikipedia — and belongs to nobody. Your
          account holds no part of it.
        </p>

        {me ? <Controls onSignedOut={onSignedOut} /> : <p className="page__plain">Sign in to take your data back or destroy it.</p>}
      </article>
    </main>
  )
}

/** The promise, as two buttons. */
function Controls({ onSignedOut }: { onSignedOut: () => void }) {
  const [busy, setBusy] = useState<'export' | 'delete' | null>(null)
  const [error, setError] = useState<string | null>(null)
  // Deleting an account cannot be undone, so it is not one press. The second
  // press is the confirmation -- and the button says what will happen rather
  // than asking "are you sure?", which is a question nobody reads.
  const [armed, setArmed] = useState(false)

  const save = () => {
    setBusy('export')
    setError(null)
    count('data_requested')
    exportAccount().then(
      blob => {
        const url = URL.createObjectURL(blob)
        const link = document.createElement('a')
        link.href = url
        link.download = 'lyrid-account.json'
        link.click()
        URL.revokeObjectURL(url)
        setBusy(null)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : 'the file could not be made')
        setBusy(null)
      }
    )
  }

  const destroy = () => {
    if (!armed) {
      setArmed(true)
      return
    }
    setBusy('delete')
    setError(null)
    count('data_requested')
    deleteAccount().then(
      () => {
        onSignedOut()
        setBusy(null)
        setArmed(false)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : 'the account could not be deleted')
        setBusy(null)
        setArmed(false)
      }
    )
  }

  return (
    <section className="page__controls">
      <h2>Your data, in your hands</h2>
      <div className="page__buttons">
        <button onClick={save} disabled={busy !== null}>
          {busy === 'export' ? 'one moment' : 'download everything'}
        </button>
        <button className="page__danger" onClick={destroy} disabled={busy !== null}>
          {busy === 'delete' ? 'one moment' : armed ? 'press again to delete it all' : 'delete my account'}
        </button>
      </div>
      {armed && !busy && (
        <p className="page__warn">
          This removes the account, the profile and every session, right now. It cannot be undone and there is no copy to restore from —
          download your data first if you want it.
        </p>
      )}
      {error && <p className="account__error">{error}</p>}
    </section>
  )
}
