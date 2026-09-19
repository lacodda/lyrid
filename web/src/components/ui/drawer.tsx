import { Drawer as Base } from '@base-ui/react/drawer'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Drawer.
 *
 * A panel that slides in from an edge and holds the screen while it is there -
 * a filter sheet, a detail pane, a form too long to centre. Modal like Dialog,
 * placed like nothing else.
 *
 * Base UI positions none of it. Unlike Popover there is no Positioner and no
 * anchor to measure against; the edge it comes from is entirely CSS, which is
 * what the `side` variant is. It drives three things at once and they have to
 * agree: where the Viewport pushes the panel, which border it grows against,
 * and which way it is translated while opening and closing.
 *
 * `swipeDirection` is the other half and cannot be inferred here: it lives on
 * the root, while `side` lives on the popup, so the two are set together by
 * hand - `side="right"` with `swipeDirection="right"`, `side="bottom"` with
 * `down`, which is Base UI's default. Left unmatched, the drawer slides in
 * from one edge and is flicked away towards another.
 *
 * The transitions key off `data-starting-style` and `data-ending-style` rather
 * than `data-closed`. That is Base UI's own convention for the drawer, and the
 * reason is that a drawer is dragged as well as animated: the popup carries a
 * live `--drawer-swipe-movement-*` while a finger is on it, and the transform
 * has to compose with that rather than replace it.
 */

/** The Viewport: fixed to the whole window, pushing the popup to one edge.
 *
 * It is separate from the popup because a drawer that is its own positioner
 * cannot be scrolled independently of where it sits - and the panel needs to
 * scroll while the edge it is pinned to does not move. */
const drawerViewportVariants = cva('fixed inset-0 flex', {
  variants: {
    side: {
      right: 'justify-end',
      left: 'justify-start',
      bottom: 'items-end',
    },
  },
  defaultVariants: { side: 'right' },
})

export const drawerPopupVariants = cva(
  [
    'flex flex-col overflow-y-auto overscroll-contain',
    'border-line bg-raise p-5 text-text shadow-float',
    'focus-visible:outline-none',
    // Composed with the live swipe offset rather than replacing it, so a
    // half-dragged drawer animates from where the finger left it.
    '[transition:transform_var(--duration-base)_var(--ease-out)]',
    'will-change-transform',
  ],
  {
    variants: {
      /* Where it comes from. The three the products of the line reach for:
       * a right-hand pane, its mirror, and a bottom sheet. */
      side: {
        right: [
          'h-full w-[min(24rem,calc(100vw-3rem))] border-l',
          '[transform:translateX(var(--drawer-swipe-movement-x))]',
          'data-[starting-style]:[transform:translateX(100%)]',
          'data-[ending-style]:[transform:translateX(100%)]',
        ],
        left: [
          'h-full w-[min(24rem,calc(100vw-3rem))] border-r',
          '[transform:translateX(var(--drawer-swipe-movement-x))]',
          'data-[starting-style]:[transform:translateX(-100%)]',
          'data-[ending-style]:[transform:translateX(-100%)]',
        ],
        bottom: [
          'max-h-[80vh] w-full rounded-t-xl border-t',
          '[transform:translateY(var(--drawer-swipe-movement-y))]',
          'data-[starting-style]:[transform:translateY(100%)]',
          'data-[ending-style]:[transform:translateY(100%)]',
        ],
      },
    },
    defaultVariants: { side: 'right' },
  },
)

/** The root. Controlled with `open` and `onOpenChange`, or left to manage
 * itself around a `DrawerTrigger`. */
export const Drawer = Base.Root

/** What opens it. Give it `render` to use your own button. */
export const DrawerTrigger = Base.Trigger

/** What closes it - the same, for a cancel button or an X. */
export const DrawerClose = Base.Close

/** The scrim. Dark because a drawer is modal: what is behind it is out of
 * reach, and saying so with contrast is the only thing that reads at a
 * glance. */
export function DrawerBackdrop({ className, ...props }: Base.Backdrop.Props) {
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

export interface DrawerPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof drawerPopupVariants> {
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

/** The panel. Portalled, and wrapped in its own viewport so the edge it is
 * pinned to holds still while the contents scroll. */
export function DrawerPopup({
  container,
  backdrop = true,
  side,
  className,
  children,
  ...props
}: DrawerPopupProps) {
  return (
    <Base.Portal container={container}>
      {backdrop && <DrawerBackdrop />}
      <div className={cn(drawerViewportVariants({ side }), '[z-index:var(--z-modal)]')}>
        <Base.Viewport className="flex w-full">
          <Base.Popup
            className={cn(drawerPopupVariants({ side }), className)}
            {...props}
          >
            {children}
          </Base.Popup>
        </Base.Viewport>
      </div>
    </Base.Portal>
  )
}

/** The heading. Base UI points the popup's `aria-labelledby` at it, so a
 * drawer with one is named for a screen reader without anyone arranging it. */
export function DrawerTitle({ className, ...props }: Base.Title.Props) {
  return <Base.Title className={cn('text-base font-semibold', className)} {...props} />
}

/** The line under the heading, and the popup's `aria-describedby`. */
export function DrawerDescription({ className, ...props }: Base.Description.Props) {
  return <Base.Description className={cn('mt-1 text-sm text-dim', className)} {...props} />
}

/** Where the actions go. Pushed to the bottom of the panel rather than sitting
 * under the content, because a drawer is tall and its buttons should not
 * wander up the page when there is little in it. */
export function DrawerActions({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('mt-auto flex justify-end gap-2 pt-5', className)} {...props} />
}
