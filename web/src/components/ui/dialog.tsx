import { useRef, type HTMLAttributes, type ReactNode } from 'react'
import { Dialog as Base } from '@base-ui/react/dialog'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'
import { LayerProvider } from './layer'

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
 * What is ours is the clothes, the enter and leave - the popup arrives with
 * `data-open` and leaves with `data-closed`, both of which Base UI sets, so
 * the animation is a class list rather than a state machine - and the anatomy.
 *
 * **Only the body scrolls.** The popup is a column of three parts: a header
 * that stays at the top, a body that takes what height is left and scrolls
 * inside it, and the actions pinned to the bottom. The popup used to scroll as
 * a whole, and a dialog with more in it than the window is tall took its own
 * buttons with it: in kilna, at a 1280x720 window, "Save" on the style editor
 * stood 137px below the bottom edge, and the only way to reach it was to
 * scroll a form to its end to find out whether it had one. The popup carries
 * no padding of its own now - each part does - so the body's scroll runs edge
 * to edge and the header and actions stay whole while it moves.
 *
 * **A popup opened inside rides above it.** The content sits in a layer
 * (`layer.tsx`): a select, a menu or a date picker in a dialog portals into a
 * host raised one rung above the modal, so it is neither clipped by the
 * body's scroll nor drawn underneath the dialog that opened it.
 */

export const dialogPopupVariants = cva(
  [
    'fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2',
    'flex flex-col overflow-hidden',
    'rounded-xl border border-line bg-raise text-text shadow-float',
    'focus-visible:outline-none',
    // Never taller than the window. The width was capped from the start and
    // the height was not, so a dialog taller than the window centred itself
    // and hung off both ends. The cap is on the column; the body is what
    // gives way.
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
        sm: 'w-[min(24rem,calc(100vw-2rem))]',
        md: 'w-[min(28rem,calc(100vw-2rem))]',
        lg: 'w-[min(40rem,calc(100vw-2rem))]',
        // An editor: a form wide enough for its fields to sit side by side,
        // or a row of choices to fit on one line rather than three.
        xl: 'w-[min(48rem,calc(100vw-2rem))]',
        // The window, less a margin that says it is still a dialog. The
        // height is set rather than capped, so the body fills it even when
        // there is little in it - a picker or a preview should not jump in
        // size as its content loads.
        full: 'h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)]',
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
 * opened from. Put a `DialogHeader`, a `DialogBody` and `DialogActions`
 * inside it. */
export function DialogPopup({
  size,
  container,
  backdrop = true,
  className,
  children,
  ...props
}: DialogPopupProps) {
  const portal = useRef<HTMLDivElement>(null)

  return (
    <Base.Portal ref={portal} container={container}>
      {backdrop && <DialogBackdrop />}
      <Base.Popup
        className={cn(dialogPopupVariants({ size }), '[z-index:var(--z-modal)]', className)}
        {...props}
      >
        <LayerProvider above="modal" mount={portal}>{children}</LayerProvider>
      </Base.Popup>
    </Base.Portal>
  )
}

/*
 * The three parts. Plain elements rather than Base UI parts, because what
 * they do is layout: which one stays and which one scrolls. That is also why
 * the drawer and the confirm dialog take them from here - the anatomy is one
 * decision, and three copies of it would be three answers by the next
 * version.
 */

export interface DialogHeaderProps extends HTMLAttributes<HTMLDivElement> {
  /** Beside the title, at the end of the line: a close button, a menu. The
   * title and description wrap under themselves rather than under it. */
  action?: ReactNode
}

/** The top of the popup: the title, the line under it, and an action beside
 * them. It stays put while the body scrolls. */
export function DialogHeader({ action, className, children, ...props }: DialogHeaderProps) {
  return (
    <div className={cn('flex shrink-0 items-start gap-3 px-5 pt-5 pb-4', className)} {...props}>
      <div className="min-w-0 flex-1">{children}</div>
      {action !== undefined && <div className="-mt-1 -mr-1.5 flex shrink-0 items-center gap-1">{action}</div>}
    </div>
  )
}

/** The part that scrolls. It takes the height the header and the actions
 * leave, and only it moves when there is more than fits. */
export function DialogBody({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'min-h-0 flex-1 overflow-y-auto overscroll-contain px-5',
        // First or last in the popup, it takes the padding the missing part
        // would have given - a dialog with no header still has a top edge.
        'first:pt-5 last:pb-5',
        // A box that scrolls can be focused, and the popup clips what is
        // outside it: the ring is drawn inside.
        'focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent',
        className,
      )}
      {...props}
    />
  )
}

export interface DialogActionsProps extends HTMLAttributes<HTMLDivElement> {
  /** At the other end of the row from the answer: the action that is not the
   * answer to the dialog's question - "Delete" on an editor whose question is
   * "Save?". Kept apart so it is not pressed on the way to "Save". */
  start?: ReactNode
}

/** Where the actions go, pinned to the bottom. Right-aligned, because the
 * primary action of a dialog belongs where the eye leaves the sentence. */
export function DialogActions({ start, className, children, ...props }: DialogActionsProps) {
  return (
    <div
      className={cn('mt-auto flex shrink-0 items-center justify-end gap-2 px-5 pt-4 pb-5', className)}
      {...props}
    >
      {start !== undefined && <div className="mr-auto flex items-center gap-2">{start}</div>}
      {children}
    </div>
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
