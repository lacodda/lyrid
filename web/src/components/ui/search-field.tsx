import { useCallback, useRef, type InputHTMLAttributes, type Ref } from 'react'
import { cn } from 'dowel-ui'
import { fieldClasses } from './input'
import { Kbd } from './kbd'
import { useShortcut } from './shortcut'

/*
 * SearchField.
 *
 * An Input that knows it is a search box, which is three small things the
 * products kept not doing:
 *
 *   - a magnifier, so the field is recognisable before it is read;
 *   - a way to clear it that is not "select all and delete" - and one that a
 *     keyboard can reach, which a decorative `<span>` cannot;
 *   - the shortcut that focuses it, shown in the field rather than learned.
 *
 * `type="search"` is deliberate: it is what tells a browser to offer previous
 * queries, and what makes Escape clear the field on the platforms where that
 * is the convention. The browser's own clear button is hidden, because it is
 * drawn in the operating system's chrome and cannot be made to match - the
 * same reason the line does not use a native `<select>`.
 */

export interface SearchFieldProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type' | 'value' | 'onChange'> {
  /** The query. Controlled, because a search box that owns its own text
   * cannot be cleared by the thing that owns the results. */
  value: string
  /** Told the new query on every keystroke. */
  onValueChange: (value: string) => void
  /**
   * What the clear button is called, for a screen reader. No default: a word
   * the component invents is a word the product cannot translate.
   *
   * Leave it out and no clear button is drawn - which is the right shape for a
   * field that filters as you type and is cleared by other means.
   */
  clearLabel?: string
  /**
   * The shortcut that focuses the field, as `['Mod', 'K']`. Shown at the right
   * of the field, and bound: pressing it focuses and selects, from anywhere
   * that is not already a field.
   */
  shortcut?: string[]
  ref?: Ref<HTMLInputElement>
}

export function SearchField({
  value,
  onValueChange,
  clearLabel,
  shortcut,
  className,
  ref,
  ...props
}: SearchFieldProps) {
  const own = useRef<HTMLInputElement>(null)

  const setRefs = useCallback(
    (element: HTMLInputElement | null) => {
      own.current = element
      if (typeof ref === 'function') ref(element)
      else if (ref) ref.current = element
    },
    [ref],
  )

  // Focus and select, so the shortcut replaces a stale query rather than
  // appending to it. Not while someone is typing elsewhere - that is
  // `useShortcut`'s default, and it is the half of this people forget.
  const focusAndSelect = useCallback(() => {
    own.current?.focus()
    own.current?.select()
  }, [])
  useShortcut(shortcut ?? [], focusAndSelect, { enabled: shortcut !== undefined })

  const showClear = clearLabel !== undefined && value !== ''

  return (
    <div className={cn('relative', className)}>
      <MagnifierIcon />

      <input
        ref={setRefs}
        type="search"
        value={value}
        onChange={(event) => onValueChange(event.target.value)}
        className={cn(
          fieldClasses,
          'h-control pl-8',
          // Room on the right for whatever sits there, and none when nothing
          // does - a field with a permanent gap looks broken.
          showClear && 'pr-8',
          !showClear && shortcut && 'pr-14',
          // The browser's own clear affordance, in the operating system's
          // chrome. Ours is below.
          '[&::-webkit-search-cancel-button]:appearance-none',
        )}
        {...props}
      />

      {showClear && (
        <button
          type="button"
          aria-label={clearLabel}
          onClick={() => {
            onValueChange('')
            own.current?.focus()
          }}
          className={cn(
            'absolute right-1.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center',
            'rounded-sm text-faint transition-colors hover:bg-soft hover:text-text',
            'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent',
          )}
        >
          <CrossIcon />
        </button>
      )}

      {!showClear && shortcut && (
        // Decorative: the shortcut works whether or not it is read out, and a
        // screen reader announcing "Control K" inside a search box is noise.
        <Kbd
          keys={shortcut}
          aria-hidden
          className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2"
        />
      )}
    </div>
  )
}

function MagnifierIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      aria-hidden
      className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-faint"
    >
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5L14 14" strokeLinecap="round" />
    </svg>
  )
}

function CrossIcon() {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
      <path d="M4 4l8 8M12 4l-8 8" strokeLinecap="round" />
    </svg>
  )
}
