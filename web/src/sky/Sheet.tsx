import { useRef, type ReactNode } from 'react'
import { Drawer as Base } from '@base-ui/react/drawer'
import { cn } from 'dowel-ui'

import { drawerPopupVariants } from '@/components/ui/drawer'
import { LayerProvider } from '@/components/ui/layer'

interface Props {
  open: boolean
  onClose: () => void
  /** What the sheet is, for a reader who cannot see it rise. */
  label: string
  /** How tall it may be, in CSS pixels: see `sheetHeight`. */
  maxHeight: number
  children: ReactNode
}

/**
 * A sheet over the sky: dowel's bottom drawer, with the sky left in reach.
 *
 * The line's `DrawerPopup` is a modal: a scrim, a focus trap, and a frame the
 * size of the window that takes every press. That is right for a form and
 * wrong here -- the sheet holds a star's card, and the sky above it is where
 * the next star is picked, and the instruments in the other sheet change a sky
 * the person has to be able to see. So this is the same drawer -- Base UI's
 * parts and dowel's own look for the popup -- put together without the modal
 * half: no scrim, no trap, and a viewport that lets presses through
 * everywhere but on the sheet itself (ADR 0031).
 *
 * It is closed by its own button, by a swipe down, or by Escape; a press on
 * the sky does not close it, because a press on the sky is a pan.
 */
export function Sheet({ open, onClose, label, maxHeight, children }: Props) {
  const portal = useRef<HTMLDivElement>(null)
  const popup = useRef<HTMLDivElement>(null)

  return (
    <Base.Root
      open={open}
      onOpenChange={next => {
        if (!next) onClose()
      }}
      modal={false}
      disablePointerDismissal
    >
      <Base.Portal ref={portal}>
        <Base.Viewport className="pointer-events-none fixed inset-x-0 bottom-0 flex items-end [z-index:var(--z-floating)]">
          <Base.Popup
            aria-label={label}
            className={cn(drawerPopupVariants({ side: 'bottom' }), 'pointer-events-auto')}
            style={{ maxHeight }}
            ref={popup}
            // The sheet itself, not its first control: a card that opened by
            // a press on a star should not answer with a ring round its close
            // button, nor a sheet with a field in it with the keyboard.
            initialFocus={popup}
          >
            {/* The grip a sheet is known by. Decoration: the swipe works
                anywhere on the sheet, and the close button is the one with a
                name. */}
            <span aria-hidden="true" className="mx-auto mt-2 block h-1 w-9 shrink-0 rounded-full bg-line" />
            {/* A menu or a select opened in the sheet rides above it. */}
            <LayerProvider above="floating" mount={portal}>
              {children}
            </LayerProvider>
          </Base.Popup>
        </Base.Viewport>
      </Base.Portal>
    </Base.Root>
  )
}
