import { Fragment, type HTMLAttributes } from 'react'
import { cn } from 'dowel-ui'
import { isApplePlatform, parseKeys, strokeParts } from './shortcut'

/*
 * Kbd.
 *
 * A key, as printed in a menu or a hint: `Ctrl` `K`. It is a `<kbd>` element
 * because that is what the element is for - a screen reader announces it as
 * keyboard input rather than reading a stray capital letter.
 *
 * It reads the line's own notation, the one a command is bound with, so what
 * is shown and what is bound are the same string: `Mod+K` is drawn `Ctrl` `K`
 * here and `⌘` `K` on a Mac, and a product that writes the branch itself is
 * a product whose hint is wrong on one of them.
 *
 * A sequence - `G D` - is drawn as its steps with a mark between them, so
 * G-then-D does not read as G-with-D, and two sequences side by side do not
 * read as one of four steps. The mark is `›`, not a word: a word here would be
 * English the product cannot translate, and the notation has no key by that
 * name, so it cannot be mistaken for one.
 */

/** What a key is called on this platform. `Mod` is the one that differs most:
 * command on Apple platforms, control everywhere else. */
export function keyLabel(key: string, apple: boolean = isApplePlatform()): string {
  const shared: Record<string, string> = {
    Enter: '↵',
    Escape: 'Esc',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Backspace: '⌫',
    Delete: 'Del',
    Tab: '⇥',
    Space: '␣',
    PageUp: 'PgUp',
    PageDown: 'PgDn',
  }
  const perPlatform: Record<string, [apple: string, other: string]> = {
    Mod: ['⌘', 'Ctrl'],
    Ctrl: ['⌃', 'Ctrl'],
    Alt: ['⌥', 'Alt'],
    Shift: ['⇧', 'Shift'],
  }

  const platform = perPlatform[key]
  if (platform) return apple ? platform[0] : platform[1]
  return shared[key] ?? key
}

export interface KbdProps extends HTMLAttributes<HTMLElement> {
  /** A shortcut in the line's notation - `Mod+K`, `G D`, `?`. Given this, the
   * component draws each key under this platform's own name. Without it, the
   * children are the key. */
  keys?: string
}

export function Kbd({ keys, className, children, ...props }: KbdProps) {
  const cap = cn(
    'inline-flex min-w-5 items-center justify-center rounded-sm border border-line bg-soft',
    'px-1 py-0.5 font-mono text-2xs leading-none text-dim',
  )

  if (keys === undefined) {
    return (
      <kbd className={cn(cap, className)} {...props}>
        {children}
      </kbd>
    )
  }

  const apple = isApplePlatform()
  return (
    <span className={cn('inline-flex items-center gap-1', className)} {...props}>
      {parseKeys(keys, apple).map((stroke, step) => (
        <Fragment key={step}>
          {step > 0 && (
            <span aria-hidden className="text-2xs leading-none text-dim">
              ›
            </span>
          )}
          <span data-step={step} className="inline-flex items-center gap-0.5">
            {strokeParts(stroke).map((key) => (
              <kbd key={key} className={cap}>
                {keyLabel(key, apple)}
              </kbd>
            ))}
          </span>
        </Fragment>
      ))}
    </span>
  )
}
