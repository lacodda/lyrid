import { useRef } from 'react'
import { AlertDialog as Base } from '@base-ui/react/alert-dialog'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'
import { DialogActions, DialogBody, DialogHeader } from './dialog'
import { LayerProvider } from './layer'

/*
 * ConfirmDialog.
 *
 * The dialog for a choice that cannot be taken back - deleting, discarding,
 * revoking. It looks almost exactly like Dialog, and that is deliberate: the
 * difference is not clothes, it is what the popup is allowed to do.
 *
 * A Dialog is dismissed by clicking away, because a Dialog is a place the user
 * wandered into. This one is not: Base UI's `AlertDialog` announces itself as
 * `role="alertdialog"`, which tells a screen reader the popup is interrupting
 * rather than presenting, and it forces `modal` and `disablePointerDismissal`
 * on - `AlertDialog.Root` omits both from Dialog's props, so there is nothing
 * to turn off. A press outside does nothing at all.
 *
 * `Escape` still closes it, and that is deliberate rather than an oversight: a
 * popup with no keyboard way out is a trap. The difference is between a
 * deliberate keypress and clicking absentmindedly beside a dialog.
 *
 * So the rule for choosing between the two is not how important the content
 * feels. It is whether dismissing it by a stray click would be a loss.
 *
 * The anatomy is the dialog's, taken from it: a header, a body that scrolls
 * if the consequences run long, and the two answers pinned to the bottom. It
 * has the dialog's small sizes and not its large ones on purpose - a question
 * that needs an editor's width to be asked is an editor, and belongs in a
 * Dialog.
 */

export const confirmDialogPopupVariants = cva(
  [
    'fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2',
    'flex flex-col overflow-hidden',
    'rounded-xl border border-line bg-raise text-text shadow-float',
    'focus-visible:outline-none',
    // Never taller than the window, which this one used not to promise: a
    // long list of what a deletion takes with it pushed both answers off
    // the bottom edge.
    'max-h-[calc(100dvh-2rem)]',
    // The enter and the leave. `duration-*` reads the token directly because
    // Tailwind's own utility takes a literal number.
    '[transition:opacity_var(--duration-base)_var(--ease-out),transform_var(--duration-base)_var(--ease-out)]',
    'data-[closed]:scale-[0.98] data-[closed]:opacity-0',
    'data-[starting-style]:scale-[0.98] data-[starting-style]:opacity-0',
  ],
  {
    variants: {
      size: {
        sm: 'w-[min(22rem,calc(100vw-2rem))]',
        md: 'w-[min(26rem,calc(100vw-2rem))]',
        lg: 'w-[min(34rem,calc(100vw-2rem))]',
      },
    },
    defaultVariants: { size: 'md' },
  },
)

/** The root. Controlled with `open` and `onOpenChange`, or left to manage
 * itself around a `ConfirmDialogTrigger`. */
export const ConfirmDialog = Base.Root

/** What opens it. Give it `render` to use your own button. */
export const ConfirmDialogTrigger = Base.Trigger

/** What closes it - and the only thing that does, which is why a confirm
 * dialog with no `ConfirmDialogClose` inside it is a trap. */
export const ConfirmDialogClose = Base.Close

/** The scrim. Darker than the Dialog's, because what is behind it is not just
 * out of reach for a moment - it is waiting on an answer. */
export function ConfirmDialogBackdrop({ className, ...props }: Base.Backdrop.Props) {
  return (
    <Base.Backdrop
      className={cn(
        'fixed inset-0 bg-black/60 backdrop-blur-[2px]',
        '[z-index:var(--z-overlay)]',
        '[transition:opacity_var(--duration-base)_var(--ease-out)]',
        'data-[closed]:opacity-0 data-[starting-style]:opacity-0',
        className,
      )}
      {...props}
    />
  )
}

export interface ConfirmDialogPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof confirmDialogPopupVariants> {
  /** Where to portal to. Defaults to the document body, which is what keeps
   * the popup from being clipped by an ancestor. Pass an element to put it
   * somewhere else - inside an overlay that is already open, or into a
   * container being screenshotted. */
  container?: Base.Portal.Props['container']
  /** Whether to draw the scrim. On by default, and it should stay on for
   * anything a person actually uses: the dim is what says the page behind is
   * out of reach. Turn it off where the popup is shown alongside other things
   * on purpose - a component gallery, a screenshot - because a scrim is
   * `position: fixed` and covers everything, not only its own container. */
  backdrop?: boolean
}

/** The dialog itself, announced as `alertdialog`. Portalled, so it is not
 * clipped by whatever it was opened from. */
export function ConfirmDialogPopup({
  size,
  container,
  backdrop = true,
  className,
  children,
  ...props
}: ConfirmDialogPopupProps) {
  const portal = useRef<HTMLDivElement>(null)

  return (
    <Base.Portal ref={portal} container={container}>
      {backdrop && <ConfirmDialogBackdrop />}
      <Base.Popup
        className={cn(
          confirmDialogPopupVariants({ size }),
          '[z-index:var(--z-modal)]',
          className,
        )}
        {...props}
      >
        <LayerProvider above="modal" mount={portal}>{children}</LayerProvider>
      </Base.Popup>
    </Base.Portal>
  )
}

/** The question. Base UI points the popup's `aria-labelledby` at it, so this
 * is what a screen reader reads out when the dialog interrupts. */
export function ConfirmDialogTitle({ className, ...props }: Base.Title.Props) {
  return <Base.Title className={cn('text-base font-semibold', className)} {...props} />
}

/** What the answer costs, and the popup's `aria-describedby`. On this dialog
 * it is close to required: the title asks, the description says what happens. */
export function ConfirmDialogDescription({ className, ...props }: Base.Description.Props) {
  return <Base.Description className={cn('mt-1 text-sm text-dim', className)} {...props} />
}

/** The top: the question and what it costs. The dialog's part, under this
 * one's name. */
export const ConfirmDialogHeader = DialogHeader

/** The part that scrolls, for consequences too long to fit. The dialog's
 * part, under this one's name. */
export const ConfirmDialogBody = DialogBody

/** Where the two answers go, pinned to the bottom and right-aligned, because
 * the choice belongs where the eye leaves the sentence. The dialog's part,
 * under this one's name. */
export const ConfirmDialogActions = DialogActions
