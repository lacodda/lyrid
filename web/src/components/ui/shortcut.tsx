import { useEffect } from 'react'

/*
 * Shortcut.
 *
 * Two things every product writes and half of them get subtly wrong: deciding
 * whether a keystroke is the shortcut, and deciding whether now is a moment to
 * act on it.
 *
 * The second is the one that bites. A shortcut that fires while someone is
 * typing in a field is a bug nobody can describe afterwards - `Mod+K` inside a
 * text editor means "delete to end of line", and a palette that opens on top
 * of it looks like the application misheard. So a keystroke aimed at an input,
 * a textarea or anything `contenteditable` belongs to that thing, always.
 *
 * A shortcut is written the way it is read: `['Mod', 'K']`. `Mod` is command
 * on Apple platforms and control everywhere else, which is the same rule Kbd
 * draws it by, so what is bound and what is shown cannot disagree.
 */

/** Whether the event landed somewhere that owns its own keys. */
export function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null
  if (!element) return false
  const tag = element.tagName
  // Coerced: `isContentEditable` is computed from layout, and an engine that
  // does not lay out - jsdom, and any server render - leaves it `undefined`
  // rather than `false`. Truthiness is enough for the branch below, but a
  // function that says it returns a boolean has to.
  return tag === 'INPUT' || tag === 'TEXTAREA' || Boolean(element.isContentEditable)
}

/** Does this event match the shortcut? Exported because a product sometimes
 * has to ask the question inside a handler it already owns. */
export function matchesShortcut(event: KeyboardEvent, shortcut: string[]): boolean {
  const wants = new Set(shortcut.map((key) => key.toLowerCase()))
  const mod = wants.delete('mod')
  const shift = wants.delete('shift')
  const alt = wants.delete('alt')
  const [key] = [...wants]
  if (key === undefined) return false

  // An exact match on every modifier, in both directions: `Mod+K` must not
  // fire on `Mod+Shift+K`, which is usually a different command entirely.
  if (mod !== (event.metaKey || event.ctrlKey)) return false
  if (shift !== event.shiftKey) return false
  if (alt !== event.altKey) return false
  return event.key.toLowerCase() === key
}

export interface UseShortcutOptions {
  /** Bind it at all. For a shortcut that only exists on some screens. */
  enabled?: boolean
  /**
   * Fire even while someone is typing in a field.
   *
   * Off by default, and the default is the point. Turn it on for a shortcut
   * that belongs to the field itself - `Escape` closing the box it is typed
   * in - and never for one that takes the person somewhere else.
   */
  whileTyping?: boolean
}

/**
 * Run something when a shortcut is pressed.
 *
 * The handler is read from a ref, so a caller that writes it inline does not
 * rebind the listener on every render - which is how these end up firing twice.
 */
export function useShortcut(
  shortcut: string[],
  onPress: (event: KeyboardEvent) => void,
  { enabled = true, whileTyping = false }: UseShortcutOptions = {},
): void {
  useEffect(() => {
    if (!enabled) return

    const onKeyDown = (event: KeyboardEvent) => {
      if (!whileTyping && isTypingTarget(event.target)) return
      if (!matchesShortcut(event, shortcut)) return
      event.preventDefault()
      onPress(event)
    }

    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
    // `shortcut` is an array literal at most call sites, so it is joined
    // rather than compared by identity: otherwise every render rebinds.
  }, [shortcut.join('+'), onPress, enabled, whileTyping]) // eslint-disable-line react-hooks/exhaustive-deps
}
