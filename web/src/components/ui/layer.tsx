import {
  createContext,
  useContext,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
  type RefObject,
} from 'react'

/*
 * Layer.
 *
 * Which layer a popup opened inside an overlay lands on, so that a select in
 * a dialog, a menu in a drawer and a date picker in a popover open above the
 * thing that opened them.
 *
 * The stacking scale is a flat ladder - `--z-menu` 30, `--z-floating` 40,
 * `--z-modal` 60 - and every popup portals to the document body, so the ladder
 * alone decides who covers whom. It answers the common question right and has
 * nothing to say about the one that bites: a popup opened from inside an
 * overlay. A date picker in a dialog is a 40 beside a 60, so the month drew
 * under the dialog that asked for it; a menu in a drawer is a 30 beside a 60;
 * a select in a popover is a 30 beside a 40. Each reads as a control that does
 * nothing.
 *
 * Two other fixes were measured in kilna first, and both look right until the
 * screen is looked at. Portalling the popup INTO the overlay puts it in the
 * overlay's stacking context, where it does paint on top - but a dialog's body
 * scrolls, and a scroll box clips a popup inside it: the month came out with
 * one row of days. Raising the popup's own number works exactly once: it is
 * then above every overlay including the ones it is not inside, and the next
 * pairing arrives with no number left for it.
 *
 * So the popup stays at the body, where nothing clips it, and it is the LAYER
 * that travels. An overlay opens a host of its own on the body and raises the
 * popup rungs inside it; a popup opened anywhere within that overlay portals
 * into the host (`usePopupContainer`) and reads the raised value through
 * ordinary inheritance - its class still says `[z-index:var(--z-floating)]`
 * and has no idea this exists.
 *
 * **The floor is worked out by the stylesheet, not written in numbers.** The
 * donor in kilna kept a mirror of the ladder in TypeScript and a test that the
 * mirror had not drifted. Here the floor is `calc(var(--z-modal) + 1)`, read
 * from the theme where it is declared, so renumbering the ladder moves every
 * floor with it and there is no second copy to forget.
 *
 * **A layer inside a layer keeps climbing.** A popover opened in a dialog is a
 * layer of its own - a select inside it must clear the popover, not only the
 * dialog - so a nested host goes inside its parent's host and inherits the
 * raised rungs. Its floor is one above the larger of its own rung and its
 * parent's floor, which is right for both directions: a popover in a dialog
 * rides above the dialog's floor, and a dialog opened from inside a popover
 * gives its popups a floor above the modal rung rather than the popover's.
 *
 * **Two nodes, not one, and the reason is the cascade.** The floor is computed
 * from the rung the host inherits, and the host then redefines that same rung.
 * On one element that is a cycle - `--z-floating` defined through a floor that
 * is defined through `--z-floating` - and CSS answers a cycle by making both
 * invalid, which sends the z-index back to `auto`. So an outer frame works the
 * floor out from what it inherited, and the host inside it publishes it.
 * Neither node draws anything or makes a stacking context: they stay static,
 * so a popup inside is placed and measured exactly as it would be on the body.
 */

/** The rungs of the stacking scale an overlay stands on. */
export type LayerRung = 'popup' | 'menu' | 'floating' | 'overlay' | 'modal' | 'palette'

/** The rungs a popup reads its z-index from. Every one is raised, not only the
 * one a particular popup happens to read: a popover reads floating, a menu and
 * a select read menu, a tooltip reads popup - and for as long as the donor
 * lifted only floating, the select in a dialog stayed on the page's floor. */
const POPUP_RUNGS = ['--z-popup', '--z-menu', '--z-floating'] as const

interface Layer {
  readonly host: RefObject<HTMLElement | null>
}

const LayerContext = createContext<Layer | null>(null)

/** Where a popup opened here should portal to, or `undefined` when there is no
 * overlay around it and the body is right. A popup passes it as its portal's
 * `container`, after the caller's own. */
export function usePopupContainer(): RefObject<HTMLElement | null> | undefined {
  return useContext(LayerContext)?.host
}

/** The floor of a layer above `rung`, as the frame declares it: one above the
 * larger of the rung and the floor it is nested in. `--z-layer` is unset
 * outside every layer, and the fallback makes the rung win there. */
export function layerFloor(rung: LayerRung): string {
  return `calc(max(var(--z-${rung}), var(--z-layer, 0)) + 1)`
}

export interface LayerProviderProps {
  /** The rung the overlay itself stands on: `modal` for a dialog or a drawer,
   * `palette` for the command palette, `floating` for a popover, `overlay`
   * for a product's own full-screen stage. */
  above: LayerRung
  /** Where the layer goes: the overlay's own portal node. Every overlay of
   * the set passes it, and it matters for the modal ones. A modal dialog
   * hides everything outside its portal from assistive technology - Base UI
   * marks the rest of the body `aria-hidden` - and a layer beside the portal
   * rather than in it was hidden with the page: the menu opened in a drawer
   * worked for a pointer and did not exist for a screen reader. Inside the
   * portal the layer counts as the overlay's own. Without it the layer goes
   * into the layer it was opened in, or onto the body. */
  mount?: RefObject<HTMLElement | null>
  children: ReactNode
}

/** Opens a raised host beside an overlay and offers it to every popup opened
 * inside. Put it inside the overlay's popup, around its content. */
export function LayerProvider({ above, mount, children }: LayerProviderProps) {
  const parent = useContext(LayerContext)

  // The nodes are made during the first render rather than in the effect. A
  // popup reads its `container` as it mounts, and a host that only appeared
  // on a second pass would let the first paint go to the body - the one frame
  // in which the bug is still there. A lazy state initialiser rather than a
  // ref filled in during render: it runs once, and nothing is read or written
  // through a ref while rendering. The pair never changes, so nothing ever
  // re-renders because of it.
  const [layer] = useState<Layer & { frame: HTMLElement | null }>(() => {
    if (typeof document === 'undefined') return { frame: null, host: { current: null } }
    const frame = document.createElement('div')
    const host = document.createElement('div')
    frame.append(host)
    return { frame, host: { current: host } }
  })

  // A layout effect, so the host is in the document before the browser paints
  // anything portalled into it. A child's effect runs before its parent's,
  // which is fine: the parent's host already exists from its render, and the
  // parent attaches the whole branch when its own effect runs.
  useLayoutEffect(() => {
    const outer = layer.frame
    const inner = layer.host.current
    if (outer === null || inner === null) return
    outer.setAttribute('data-dowel-layer', above)
    // Marked as a portal host, which is what it is - and what keeps a reader
    // able to reach it. A modal overlay hides every sibling of its popup from
    // assistive technology when it opens, inside its own portal as much as
    // on the body, and spares only the portal nodes nested in it. The layer
    // is empty at that moment - the select inside is opened later - so
    // without the mark the whole layer was hidden, and every popup opened
    // into it after: a menu that worked for a pointer and was not there for
    // a screen reader.
    outer.setAttribute('data-base-ui-portal', '')
    outer.style.setProperty('--z-layer-next', layerFloor(above))
    inner.style.setProperty('--z-layer', 'var(--z-layer-next)')
    for (const rung of POPUP_RUNGS) inner.style.setProperty(rung, 'var(--z-layer-next)')
    ;(mount?.current ?? parent?.host.current ?? document.body).append(outer)
    return () => outer.remove()
  }, [above, layer, mount, parent])

  const value = useMemo<Layer>(() => ({ host: layer.host }), [layer])
  return <LayerContext.Provider value={value}>{children}</LayerContext.Provider>
}
