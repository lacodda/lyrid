import { Dialog as Base } from '@base-ui/react/dialog'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Dialog.
 *
 * The parts are exposed rather than wrapped in one component with `title` and
 * `footer` props. The products that did it the other way ended up passing
 * `footer={<>…</>}` within a week, which is a slot with extra steps - and a
 * dialog that owns its own close button owns a word for it, which is a word
 * the product cannot translate.
 *
 * Behaviour is Base UI's: the focus trap, the return of focus to whatever
 * opened it, `Escape`, the scroll lock, and the `aria-labelledby` that ties
 * the popup to its own title. None of that is worth rewriting, and all of it
 * is wrong in the ways nobody notices until someone is navigating by keyboard.
 *
 * What is ours is the clothes, and the enter and leave: the popup arrives with
 * `data-open` and leaves with `data-closed`, both of which Base UI sets, so
 * the animation is a class list rather than a state machine.
 */

export const dialogPopupVariants = cva(
  [
    'fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2',
    'rounded-xl border border-line bg-raise p-5 text-text shadow-float',
    'focus-visible:outline-none',
    // The width was capped from the start and the height was not, so a dialog
    // with more in it than the window is tall centred itself and hung off both
    // ends - the top out of reach above the viewport, the buttons below it.
    // Found on a release editor in kilna, which is exactly the shape that does
    // it: half a dozen fields and a row of actions.
    'max-h-[calc(100dvh-2rem)] overflow-y-auto overscroll-contain',
    // The enter and the leave. `duration-*` reads the token directly because
    // Tailwind's own utility takes a literal number.
    '[transition:opacity_var(--duration-base)_var(--ease-out),transform_var(--duration-base)_var(--ease-out)]',
    'data-[closed]:scale-[0.98] data-[closed]:opacity-0',
    'data-[starting-style]:scale-[0.98] data-[starting-style]:opacity-0',
  ],
  {
    variants: {
      size: {
        sm: 'w-[min(24rem,calc(100vw-2rem))]',
        md: 'w-[min(28rem,calc(100vw-2rem))]',
        lg: 'w-[min(40rem,calc(100vw-2rem))]',
      },
    },
    defaultVariants: { size: 'md' },
  },
)

/** The root. Controlled with `open` and `onOpenChange`, or left to manage
 * itself around a `Dialog.Trigger`. */
export const Dialog = Base.Root

/** What opens it. Give it `render` to use your own button. */
export const DialogTrigger = Base.Trigger

/** What closes it - the same, for a cancel button or an X. */
export const DialogClose = Base.Close

/** The scrim. Dark because a modal is a modal: what is behind it is out of
 * reach, and saying so with contrast is the only thing that reads at a
 * glance. */
export function DialogBackdrop({ className, ...props }: Base.Backdrop.Props) {
  return (
    <Base.Backdrop
      className={cn(
        'fixed inset-0 bg-black/50 backdrop-blur-[2px]',
        '[z-index:var(--z-overlay)]',
        '[transition:opacity_var(--duration-base)_var(--ease-out)]',
        'data-[closed]:opacity-0 data-[starting-style]:opacity-0',
        className,
      )}
      {...props}
    />
  )
}

export interface DialogPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof dialogPopupVariants> {
  /** Where to portal to. Defaults to the document body, which is what keeps
   * the popup from being clipped by whatever it was opened from. Pass an
   * element to put it somewhere else - inside a dialog that is already open,
   * or into a container being screenshotted. */
  container?: Base.Portal.Props['container']
  /** Whether to draw the scrim. On by default, and it should stay on for
   * anything a person actually uses: the dim is what says the page behind is
   * out of reach. Turn it off where the popup is shown alongside other things
   * on purpose - a component gallery, a screenshot - because a scrim is
   * `position: fixed` and covers everything, not only its own container. */
  backdrop?: boolean
}

/** The dialog itself. Portalled, so it is not clipped by whatever it was
 * opened from. */
export function DialogPopup({
  size,
  container,
  backdrop = true,
  className,
  children,
  ...props
}: DialogPopupProps) {
  return (
    <Base.Portal container={container}>
      {backdrop && <DialogBackdrop />}
      <Base.Popup
        className={cn(dialogPopupVariants({ size }), '[z-index:var(--z-modal)]', className)}
        {...props}
      >
        {children}
      </Base.Popup>
    </Base.Portal>
  )
}

/** The heading. Base UI points the popup's `aria-labelledby` at it, so a
 * dialog with one is named for a screen reader without anyone arranging it. */
export function DialogTitle({ className, ...props }: Base.Title.Props) {
  return <Base.Title className={cn('text-base font-semibold', className)} {...props} />
}

/** The line under the heading, and the popup's `aria-describedby`. */
export function DialogDescription({ className, ...props }: Base.Description.Props) {
  return <Base.Description className={cn('mt-1 text-sm text-dim', className)} {...props} />
}

/** Where the actions go. Right-aligned, because the primary action of a
 * dialog belongs where the eye leaves the sentence. */
export function DialogActions({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('mt-5 flex justify-end gap-2', className)} {...props} />
}
