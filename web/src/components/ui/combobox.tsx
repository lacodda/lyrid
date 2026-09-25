import type { Ref } from 'react'
import { Combobox as Base } from '@base-ui/react/combobox'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'
import { usePopupContainer } from './layer'
import { fieldClasses } from './input'
import { selectItemVariants, selectPopupVariants } from './select'

/*
 * Combobox.
 *
 * A Select you can type in. The list narrows as the query is typed, which is
 * the only difference that matters and the reason to reach for this one: a
 * Select stops being usable somewhere around thirty options, and a country
 * picker or a tag field is well past that.
 *
 * Everything the Select comment says about the native element applies here
 * too - there is no `<select>` under it, and the input is a real `<input
 * role="combobox">` so autofill, spellcheck and the phone keyboard still
 * work.
 *
 * Filtering is Base UI's: give the root an `items` array and it matches the
 * query against them with `Intl.Collator`, so accents and case behave the way
 * a reader in that language expects rather than the way `toLowerCase` does.
 * `filter` replaces the comparison; `filter={null}` turns it off for a list
 * that is filtered on a server.
 *
 * The chips are Base UI's too - Chips, Chip, ChipRemove - and that is worth
 * saying because inventing them is the obvious move and it goes wrong in one
 * specific way: hand-made chips end up as `<div>`s with an X that only a
 * pointer can reach, and the multi-select becomes keyboard-inaccessible at
 * exactly the point where it holds the most state. Base UI's are focusable,
 * walk with the arrows, and delete with Backspace.
 *
 * `Empty` renders only when nothing matched, and announces itself politely.
 * Its element stays mounted for that announcement to work, so it must not be
 * hidden with `display: none` or removed conditionally - which is why it is a
 * component here rather than a `{items.length === 0 && …}` in the product.
 */

/* Two bases, chosen by `bare`, rather than one base and an override.
 *
 * Inside `ComboboxChips` the container is the field, so the input has no
 * border, no background and no focus ring of its own - a bordered box inside a
 * bordered box reads as two controls, and two focus rings appear as one thick
 * one. The obvious way to write that is `fieldClasses` plus a few `-none`
 * classes, and it does not work: `tailwind-merge` does not treat
 * `focus-visible:outline-none` as conflicting with
 * `focus-visible:outline-2 … outline-accent`, so both survive and the later
 * one in the stylesheet wins. The same trap took `w-full` versus `w-auto`
 * earlier in this file.
 *
 * So the variant picks which set applies instead of trying to subtract from
 * one - nothing is left to a merge that has no opinion. */
export const comboboxInputVariants = cva('', {
  variants: {
    size: {
      sm: 'h-control-sm text-xs',
      md: 'h-control',
      lg: 'h-control-lg text-base',
    },
    bare: {
      true: 'h-7 w-auto min-w-24 flex-1 bg-transparent px-1 text-sm text-text placeholder:text-faint outline-none',
      false: fieldClasses,
    },
  },
  defaultVariants: { size: 'md', bare: false },
})

/** The list, and a row in it, are Select's - imported rather than copied.
 *
 * The two popups are the same object seen twice: a dropdown of options, one
 * of which can be chosen. A reader who uses both on one screen should not be
 * able to tell which is which until they type. Two `cva` calls that started
 * identical do not stay that way - one gets the padding fix - and then the
 * form has two dropdowns that are almost the same. */
export const comboboxPopupVariants = selectPopupVariants
export const comboboxItemVariants = selectItemVariants

/** The root. `items` is what gets filtered; `multiple` turns the value into an
 * array and makes the chips meaningful. */
export const Combobox = Base.Root

/** The wrapper for an input with something beside it - a clear button, an
 * icon, the chips. */
export const ComboboxInputGroup = Base.InputGroup

/** The button that opens the list without typing, for a reader who wants to
 * see everything there is. */
export const ComboboxTrigger = Base.Trigger

/** The chevron. Decorative. */
export const ComboboxIcon = Base.Icon

/** A labelled group of rows. */
export const ComboboxGroup = Base.Group

/** The rows of one group, as a render function over that group's items.
 *
 * A `List` is the listbox and there is one per combobox, so a grouped list is
 * a `List` over the groups with a `Collection` inside each - not a `List`
 * inside a `List`. Mapping by hand instead works, but the component then has
 * to be told how to match an item to a value, which is a second place for that
 * knowledge to live. */
export const ComboboxCollection = Base.Collection

/** The tick, drawn only on a chosen row. */
export const ComboboxItemIndicator = Base.ItemIndicator

/** A polite live region for the state of an asynchronous list. Stays mounted,
 * like `Empty`, so the announcement actually fires. */
export const ComboboxStatus = Base.Status

/** The container the chips sit in. Its children are plain nodes, not a render
 * function - the chosen values are mapped by `ComboboxValue` inside it.
 *
 * It wears the field's clothes and lays the chips out in a row that wraps,
 * which is the whole difference between a control and a list: unstyled, the
 * chips stack one per line and the box grows into a column of pills with the
 * input stranded underneath. The input sits on the same line as the last
 * chip and takes the rest of the width, so a half-filled field still looks
 * like a field. */
export function ComboboxChips({
  ref,
  className,
  ...props
}: Base.Chips.Props & { ref?: Ref<HTMLDivElement> }) {
  return (
    <Base.Chips
      // Taken out of `...props` and passed on deliberately: a product needs a
      // handle on this box to anchor the list to it, because the input inside
      // is only as wide as what has been typed.
      ref={ref}
      className={cn(
        fieldClasses,
        'flex min-h-9 flex-wrap items-center gap-1 py-1',
        'focus-within:outline-2 focus-within:outline-offset-0 focus-within:outline-accent',
        className,
      )}
      {...props}
    />
  )
}

/** The current value, as a render function of it. This is what turns a
 * `multiple` value into one chip per entry. */
export const ComboboxValue = Base.Value

/** `size` is taken from the native `<input size>` - a width in characters,
 * which nothing here wants - and given to the variant instead. */
export interface ComboboxInputProps
  extends Omit<Base.Input.Props, 'size'>,
    VariantProps<typeof comboboxInputVariants> {}

/** Where the query is typed. A real `<input role="combobox">`.
 *
 * Inside `ComboboxChips` it drops its own border and background: the
 * container is the field there, and a bordered input inside a bordered box
 * reads as two controls. */
export function ComboboxInput({ size, bare, className, ...props }: ComboboxInputProps) {
  // `bare` is pulled out and handed to `cva`. Left in `...props` it would be
  // spread onto the `<input>` as an unknown attribute and change nothing -
  // which is exactly what it did: the variant existed, the prop was passed,
  // and the class list came out without a trace of either.
  return <Base.Input className={cn(comboboxInputVariants({ size, bare }), className)} {...props} />
}

const iconButtonClasses = cn(
  'rounded-sm p-1 text-faint transition-colors hover:text-text',
  'focus-visible:outline-2 focus-visible:outline-accent',
  'data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
)

/** Empties the value. Base UI hides it while there is nothing to clear. */
export function ComboboxClear({ className, ...props }: Base.Clear.Props) {
  return <Base.Clear className={cn(iconButtonClasses, className)} {...props} />
}

export interface ComboboxPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof comboboxPopupVariants> {
  /** Preferred side of the input. Base UI flips it when it does not fit. */
  side?: Base.Positioner.Props['side']
  /** Alignment along that side. */
  align?: Base.Positioner.Props['align']
  /** Distance from the input, in pixels. */
  sideOffset?: Base.Positioner.Props['sideOffset']
  /**
   * What to line the list up with. Defaults to the input that owns it.
   *
   * Pass the `ComboboxChips` box when there is one: with chips, the input is
   * only as wide as what has been typed - an empty one measured 214px inside
   * a 288px field - so a list anchored to it hangs short of the box a reader
   * sees. Base UI publishes the anchor's width as `--anchor-width`, which is
   * how the mismatch is visible from outside.
   */
  anchor?: Base.Positioner.Props['anchor']
  /** Where to portal to. Defaults to the raised host of the overlay this is
   * opened inside (`layer.tsx`), and to the document body when there is none -
   * either way not the element it was opened from, whose `overflow` would clip
   * it. Pass an element to put it somewhere else, such as a container being
   * screenshotted. */
  container?: Base.Portal.Props['container']
}

/** The list. Portalled and positioned against the input, or the given anchor. */
export function ComboboxPopup({
  size,
  side,
  align,
  sideOffset = 4,
  anchor,
  container,
  className,
  children,
  ...props
}: ComboboxPopupProps) {
  // Inside an overlay, the overlay's raised host rather than the body - or
  // this popup draws under the dialog, drawer or popover that opened it. See
  // `layer.tsx`. Outside every overlay the hook gives `undefined`: the body.
  const host = usePopupContainer()

  return (
    <Base.Portal container={container ?? host}>
      <Base.Positioner
        side={side}
        align={align}
        sideOffset={sideOffset}
        anchor={anchor}
        className="[z-index:var(--z-menu)]"
      >
        <Base.Popup className={cn(comboboxPopupVariants({ size }), className)} {...props}>
          {children}
        </Base.Popup>
      </Base.Positioner>
    </Base.Portal>
  )
}

/** The rows, as a list. Undressed: it is a wrapper, and the popup around it
 * already carries the border and the padding. */
export const ComboboxList = Base.List

/** A row. */
export function ComboboxItem({ className, ...props }: Base.Item.Props) {
  return <Base.Item className={cn(comboboxItemVariants(), className)} {...props} />
}

/** What is shown when nothing matched. The words are the product's. */
export function ComboboxEmpty({ className, ...props }: Base.Empty.Props) {
  return <Base.Empty className={cn('px-2 py-3 text-center text-sm text-faint', className)} {...props} />
}

/** One chosen value, in a multiple combobox. Focusable, so it can be reached
 * and removed without a pointer. */
export function ComboboxChip({ className, ...props }: Base.Chip.Props) {
  return (
    <Base.Chip
      className={cn(
        'flex items-center gap-1 rounded-sm bg-soft px-1.5 py-0.5 text-xs text-text',
        'outline-none data-[highlighted]:bg-accent-soft data-[highlighted]:text-accent',
        className,
      )}
      {...props}
    />
  )
}

/** The X on a chip. A real button, which is what makes Backspace and Enter
 * both work on it. */
export function ComboboxChipRemove({ className, ...props }: Base.ChipRemove.Props) {
  return <Base.ChipRemove className={cn(iconButtonClasses, 'p-0', className)} {...props} />
}

/** The caption above a group. */
export function ComboboxGroupLabel({ className, ...props }: Base.GroupLabel.Props) {
  return (
    <Base.GroupLabel
      className={cn('caption px-2 py-1.5', className)}
      {...props}
    />
  )
}
