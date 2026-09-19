import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { confirmEmail, resetPassword } from '@/account'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { PasswordField } from '@/components/ui/password-field'
import { Spinner } from '@/components/ui/spinner'

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

export function ConfirmPage({ token, onClose }: { token: string; onClose: () => void }) {
  const { t } = useTranslation()
  // A missing token needs no request and no effect: it is known while
  // rendering, so it is the initial state rather than something an effect
  // corrects on the frame after.
  const [outcome, setOutcome] = useState<Outcome>(token ? { state: 'working' } : { state: 'failed', message: t('letter.noToken') })

  // Confirming happens on arrival, with no button in the way: the person
  // already pressed the button -- it was the link in the letter.
  useEffect(() => {
    if (!token) return
    let live = true
    confirmEmail(token).then(
      () => {
        if (live) setOutcome({ state: 'done', message: t('letter.confirm.done') })
      },
      (failure: unknown) => {
        if (live) setOutcome({ state: 'failed', message: failure instanceof Error ? failure.message : t('letter.confirm.failed') })
      }
    )
    return () => {
      live = false
    }
  }, [token, t])

  return (
    <Letter title={t('letter.confirm.title')}>
      {outcome.state === 'working' ? (
        <p className="flex items-center gap-2">
          <Spinner size="sm" label={t('letter.confirm.working')} />
          {t('letter.confirm.working')}
        </p>
      ) : (
        <p role={outcome.state === 'failed' ? 'alert' : undefined} className={outcome.state === 'failed' ? 'text-bad' : undefined}>
          {outcome.message}
        </p>
      )}
      <Button onClick={onClose}>{t('letter.toTheSky')}</Button>
    </Letter>
  )
}

export function ResetPage({ token, onClose }: { token: string; onClose: () => void }) {
  const { t } = useTranslation()
  const [password, setPassword] = useState('')
  const [outcome, setOutcome] = useState<Outcome | null>(null)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!token) {
      setOutcome({ state: 'failed', message: t('letter.noToken') })
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
          message: t('letter.reset.done'),
        }),
      (failure: unknown) => setOutcome({ state: 'failed', message: failure instanceof Error ? failure.message : t('letter.reset.failed') })
    )
  }

  if (outcome?.state === 'done') {
    return (
      <Letter title={t('letter.reset.doneTitle')}>
        <p>{outcome.message}</p>
        <Button onClick={onClose}>{t('letter.toTheSky')}</Button>
      </Letter>
    )
  }

  return (
    <Letter title={t('letter.reset.title')}>
      <form className="not-prose flex w-full flex-col gap-3" onSubmit={submit}>
        <Field label={t('account.newPassword')} labelHidden>
          <PasswordField
            value={password}
            onValueChange={setPassword}
            placeholder={t('account.newPassword')}
            showLabel={t('account.showPassword')}
            hideLabel={t('account.hidePassword')}
            autoComplete="new-password"
            required
          />
        </Field>
        {outcome?.state === 'failed' && (
          <p role="alert" className="text-sm text-bad">
            {outcome.message}
          </p>
        )}
        <Button type="submit" variant="primary" disabled={outcome?.state === 'working'}>
          {outcome?.state === 'working' ? t('account.working') : t('letter.reset.submit')}
        </Button>
      </form>
      <Button onClick={onClose}>{t('letter.toTheSky')}</Button>
    </Letter>
  )
}

function Letter({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <main className="grid h-full place-items-center bg-void px-6">
      <article className="prose w-full max-w-md">
        <h1>{title}</h1>
        {children}
      </article>
    </main>
  )
}
