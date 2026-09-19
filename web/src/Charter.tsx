import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { deleteAccount, exportAccount, type Me } from '@/account'
import { count } from '@/metrics'
import { Button } from '@/components/ui/button'
import {
  ConfirmDialog,
  ConfirmDialogActions,
  ConfirmDialogClose,
  ConfirmDialogDescription,
  ConfirmDialogPopup,
  ConfirmDialogTitle,
} from '@/components/ui/confirm-dialog'

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
  const { t } = useTranslation()

  return (
    <main className="h-full overflow-y-auto bg-void">
      <article className="prose mx-auto max-w-2xl px-6 py-10">
        <header className="mb-6 flex items-baseline justify-between gap-4">
          <h1 className="m-0">{t('charter.title')}</h1>
          <Button size="sm" onClick={onClose}>
            {t('charter.back')}
          </Button>
        </header>

        <p className="text-base text-text">{t('charter.lede')}</p>

        <h2>{t('charter.noAccount.heading')}</h2>
        <p>{t('charter.noAccount.body')}</p>

        <h2>{t('charter.withAccount.heading')}</h2>
        <ul>
          <li>
            <strong>{t('charter.withAccount.address')}</strong> {t('charter.withAccount.addressBody')}
          </li>
          <li>
            <strong>{t('charter.withAccount.place')}</strong> {t('charter.withAccount.placeBody')}
          </li>
          <li>
            <strong>{t('charter.withAccount.sessions')}</strong> {t('charter.withAccount.sessionsBody')}
          </li>
        </ul>

        <h2>{t('charter.counted.heading')}</h2>
        <p>{t('charter.counted.body')}</p>
        <p>{t('charter.counted.never')}</p>

        <h2>{t('charter.whose.heading')}</h2>
        <p>{t('charter.whose.body')}</p>

        {me ? <Controls onSignedOut={onSignedOut} /> : <p>{t('charter.controls.signedOut')}</p>}
      </article>
    </main>
  )
}

/** The promise, as two buttons. */
function Controls({ onSignedOut }: { onSignedOut: () => void }) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState<'export' | 'delete' | null>(null)
  const [error, setError] = useState<string | null>(null)
  // Deleting an account cannot be undone, so it is not one press. The second
  // press used to be a button that changed its own label; it is a dialog now,
  // which is what a choice that cannot be taken back is made of in this line -
  // and, unlike the armed button, it announces itself to a screen reader as an
  // interruption rather than as a label that quietly changed.
  const [asking, setAsking] = useState(false)

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
        setError(failure instanceof Error ? failure.message : t('charter.controls.exportFailed'))
        setBusy(null)
      }
    )
  }

  const destroy = () => {
    setAsking(false)
    setBusy('delete')
    setError(null)
    count('data_requested')
    deleteAccount().then(
      () => {
        onSignedOut()
        setBusy(null)
      },
      (failure: unknown) => {
        setError(failure instanceof Error ? failure.message : t('charter.controls.deleteFailed'))
        setBusy(null)
      }
    )
  }

  return (
    <section className="mt-10 border-t border-line pt-6">
      <h2 className="mt-0">{t('charter.controls.heading')}</h2>
      <div className="flex flex-wrap gap-2">
        <Button onClick={save} disabled={busy !== null}>
          {busy === 'export' ? t('account.working') : t('charter.controls.export')}
        </Button>
        <Button variant="danger" onClick={() => setAsking(true)} disabled={busy !== null}>
          {busy === 'delete' ? t('account.working') : t('charter.controls.delete')}
        </Button>
      </div>

      <ConfirmDialog open={asking} onOpenChange={setAsking}>
        <ConfirmDialogPopup>
          <ConfirmDialogTitle>{t('charter.controls.confirmTitle')}</ConfirmDialogTitle>
          <ConfirmDialogDescription>{t('charter.controls.confirmBody')}</ConfirmDialogDescription>
          <ConfirmDialogActions>
            <ConfirmDialogClose render={<Button>{t('charter.controls.cancel')}</Button>} />
            <Button variant="danger" onClick={destroy}>
              {t('charter.controls.confirmAction')}
            </Button>
          </ConfirmDialogActions>
        </ConfirmDialogPopup>
      </ConfirmDialog>

      {error && (
        <p role="alert" className="mt-3 text-sm text-bad">
          {error}
        </p>
      )}
    </section>
  )
}
