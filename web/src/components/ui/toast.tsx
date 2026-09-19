import { Toast as Base } from '@base-ui/react/toast'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Toast.
 *
 * For something that already happened and needs no decision. Anything that
 * needs an answer is a dialog - a toast that asks a question is a question the
 * reader can miss by looking away.
 *
 * The tone is on the toast rather than in five separate components, because
 * the difference between "saved" and "could not save" is emphasis, never
 * meaning: the sentence says which it is, and a reader who cannot separate
 * green from red gets the same message. The stripe down the side exists for
 * the same reason a badge carries a word.
 *
 * `Toast.Root` is `role="dialog"` inside a `role="region"` viewport, and Base
 * UI drives the live region, the timers, the pause on hover and focus, and the
 * swipe. What is here is the clothes and the vocabulary.
 */

export const toastVariants = cva(
  [
    'relative w-[min(22rem,calc(100vw-2rem))] overflow-hidden',
    'rounded-lg border border-line bg-raise p-3 pl-4 text-text shadow-raise',
    // The stripe. `before` rather than a border, so the corner radius stays
    // the panel's own.
    'before:absolute before:inset-y-0 before:left-0 before:w-1',
    '[transition:opacity_var(--duration-base)_var(--ease-out),transform_var(--duration-base)_var(--ease-out)]',
    'data-[starting-style]:translate-x-4 data-[starting-style]:opacity-0',
    'data-[ending-style]:translate-x-4 data-[ending-style]:opacity-0',
  ],
  {
    variants: {
      tone: {
        neutral: 'before:bg-line-2',
        good: 'before:bg-good',
        warn: 'before:bg-warn',
        bad: 'before:bg-bad',
        info: 'before:bg-info',
      },
    },
    defaultVariants: { tone: 'neutral' },
  },
)

/** Wraps the part of the application that can raise a toast. One per screen;
 * nesting them gives a product two queues that do not know about each other. */
export const ToastProvider = Base.Provider

/** The manager: `add`, `update`, `close`, and `promise` for the common case of
 * "say this while it runs, that when it lands". */
export const useToastManager = Base.useToastManager

/** A toast raised outside React - from a store, an event handler, a worker.
 * Pass the result to `ToastProvider`'s `toastManager`. */
export const createToastManager = Base.createToastManager

/** The heading. Base UI points the toast's `aria-labelledby` at it. */
export function ToastTitle({ className, ...props }: Base.Title.Props) {
  return <Base.Title className={cn('text-sm font-semibold', className)} {...props} />
}

/** The sentence under it, and the toast's `aria-describedby`. */
export function ToastDescription({ className, ...props }: Base.Description.Props) {
  return <Base.Description className={cn('mt-0.5 text-xs text-dim', className)} {...props} />
}

/** The one thing the reader can do about it: undo, or go and look. */
export function ToastAction({ className, ...props }: Base.Action.Props) {
  return (
    <Base.Action
      className={cn(
        'mt-2 inline-flex h-7 cursor-pointer items-center rounded-md px-2 text-xs font-medium',
        'bg-accent-soft text-accent transition-colors hover:bg-accent-soft/60',
        'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent',
        className,
      )}
      {...props}
    />
  )
}

/** The dismiss button. Needs a word - a bare cross is announced as nothing. */
export function ToastClose({ className, ...props }: Base.Close.Props) {
  return (
    <Base.Close
      className={cn(
        'absolute right-2 top-2 grid size-6 cursor-pointer place-items-center rounded-sm',
        'text-faint transition-colors hover:bg-soft hover:text-text',
        'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent',
        className,
      )}
      {...props}
    >
      <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
        <path d="M4 4l8 8M12 4l-8 8" strokeLinecap="round" />
      </svg>
    </Base.Close>
  )
}

export interface ToastProps extends Base.Root.Props, VariantProps<typeof toastVariants> {}

/** One toast. The tone comes from the caller or from `toast.type`, so a
 * product that raises them through the manager gets the stripe for free. */
export function Toast({ tone, className, toast, ...props }: ToastProps) {
  const fromType = TONE_FOR_TYPE[toast.type ?? ''] ?? undefined
  return (
    <Base.Root
      toast={toast}
      className={cn(toastVariants({ tone: tone ?? fromType }), className)}
      {...props}
    />
  )
}

/** Base UI's own `type` values, mapped onto the vocabulary. A product calling
 * `manager.add({ type: 'success' })` should not also have to say which colour
 * that is. */
const TONE_FOR_TYPE: Record<string, 'good' | 'warn' | 'bad' | 'info' | undefined> = {
  success: 'good',
  warning: 'warn',
  error: 'bad',
  info: 'info',
  loading: 'info',
}

export interface ToastViewportProps extends Base.Viewport.Props {
  /** Where to portal to. Defaults to the document body, which is what keeps
   * toasts above everything regardless of where they were raised from. */
  container?: Base.Portal.Props['container']
}

/** Where they stack. Bottom right by default, which is the corner that does
 * not cover a form being filled in or a menu being read. */
export function ToastViewport({ container, className, ...props }: ToastViewportProps) {
  return (
    <Base.Portal container={container}>
      <Base.Viewport
        className={cn(
          'fixed bottom-4 right-4 flex w-[min(22rem,calc(100vw-2rem))] flex-col-reverse gap-2',
          '[z-index:var(--z-toast)]',
          className,
        )}
        {...props}
      />
    </Base.Portal>
  )
}
