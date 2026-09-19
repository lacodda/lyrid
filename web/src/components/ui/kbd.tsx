import type { HTMLAttributes } from 'react'
import { cn } from 'dowel-ui'

/*
 * Kbd.
 *
 * A key, as printed in a menu or a hint: `Ctrl` `K`. It is a `<kbd>` element
 * because that is what the element is for - a screen reader announces it as
 * keyboard input rather than reading a stray capital letter.
 *
 * The platform substitution is the useful part. A shortcut written `Ctrl+K` is
 * wrong on a Mac, where the same shortcut is `⌘K`, and every product either
 * hard-codes one of them or writes the branch again.
 */

/** Whether this machine writes shortcuts the Apple way. */
function isApplePlatform(): boolean {
  if (typeof navigator === 'undefined') return false
  return /mac|iphone|ipad|ipod/i.test(navigator.platform || navigator.userAgent)
}

/** What a key is called here. `Mod` is the one that differs: command on Apple
 * platforms, control everywhere else. */
export function keyLabel(key: string, apple: boolean = isApplePlatform()): string {
  const shared: Record<string, string> = {
    Enter: '↵',
    Escape: 'Esc',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Backspace: '⌫',
    Tab: '⇥',
    Space: '␣',
  }
  const perPlatform: Record<string, [apple: string, other: string]> = {
    Mod: ['⌘', 'Ctrl'],
    Alt: ['⌥', 'Alt'],
    Shift: ['⇧', 'Shift'],
  }

  const platform = perPlatform[key]
  if (platform) return apple ? platform[0] : platform[1]
  return shared[key] ?? key
}

export interface KbdProps extends HTMLAttributes<HTMLElement> {
  /** Keys of a shortcut, in order: `['Mod', 'K']`. Given this, the component
   * writes the separators and the platform's own names. */
  keys?: string[]
}

export function Kbd({ keys, className, children, ...props }: KbdProps) {
  const cap = cn(
    'inline-flex min-w-5 items-center justify-center rounded-sm border border-line bg-soft',
    'px-1 py-0.5 font-mono text-2xs leading-none text-dim',
  )

  if (!keys) {
    return (
      <kbd className={cn(cap, className)} {...props}>
        {children}
      </kbd>
    )
  }

  return (
    <span className={cn('inline-flex items-center gap-0.5', className)} {...props}>
      {keys.map((key) => (
        <kbd key={key} className={cap}>
          {keyLabel(key)}
        </kbd>
      ))}
    </span>
  )
}
