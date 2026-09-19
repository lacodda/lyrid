import { Select as Base } from '@base-ui/react/select'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'
import { fieldClasses } from './input'

/*
 * Select.
 *
 * The component the oldest rule in the line is about. A native `<select>`
 * cannot be dressed: the browser draws its popup itself, in the operating
 * system's chrome, and no CSS reaches inside. One native dropdown on a screen
 * of the product's own controls reads as a foreign object, and on Windows it
 * reads as a foreign object from 1998.
 *
 * So this renders `<button role="combobox">` and a portalled list of
 * `role="option"` - zero native elements, which is asserted in the test,
 * because it is the entire reason the component exists.
 *
 * What that costs is everything the browser was doing for free: the keyboard,
 * type-ahead, the announcement of the selected value, the scroll into view,
 * and on a phone the whole native picker. Base UI does all of it, which is
 * the only reason this trade is worth making - a hand-rolled dropdown is how
 * a product ships a control that a screen reader cannot see.
 *
 * The trigger wears Input's `fieldClasses`, imported rather than copied. A
 * select and a text field sit next to each other in every form there has ever
 * been, and two class lists that started the same drift within a release.
 *
 * `multiple` is a prop on the Root: it changes what `value` means - an array
 * rather than a single value - so it belongs where the value lives and not on
 * the trigger.
 */

export const selectTriggerVariants = cva([fieldClasses, 'flex items-center justify-between gap-2'], {
  variants: {
    size: {
      sm: 'h-control-sm text-xs',
      md: 'h-control',
      lg: 'h-control-lg text-base',
    },
  },
  defaultVariants: { size: 'md' },
})

export const selectPopupVariants = cva(
  [
    'max-h-[min(24rem,var(--available-height))] overflow-y-auto',
    'rounded-md border border-line bg-raise p-1 text-text shadow-raise',
    'focus-visible:outline-none',
    '[transition:opacity_var(--duration-quick)_var(--ease-out),transform_var(--duration-quick)_var(--ease-out)]',
    'data-[closed]:scale-[0.98] data-[closed]:opacity-0',
    'data-[starting-style]:scale-[0.98] data-[starting-style]:opacity-0',
  ],
  {
    variants: {
      size: {
        // The popup matches the trigger's width by default, which is what a
        // dropdown should do; the sizes are a floor for a narrow one.
        sm: 'min-w-[max(8rem,var(--anchor-width))]',
        md: 'min-w-[max(10rem,var(--anchor-width))]',
        lg: 'min-w-[max(14rem,var(--anchor-width))]',
      },
    },
    defaultVariants: { size: 'md' },
  },
)

/** One option. */
export const selectItemVariants = cva([
  'relative flex cursor-pointer select-none items-center gap-2 rounded-sm py-1.5 pl-2 pr-7 text-sm',
  'outline-none transition-colors',
  // Base UI marks the item under the pointer or the keyboard the same way,
  // so one rule covers both and they cannot disagree.
  'data-[highlighted]:bg-soft data-[highlighted]:text-text',
  // What is already chosen has to be visible in the list, and colour alone
  // will not do it: `text-text` on an item that is already `text-text` says
  // nothing. The tick below is the state; this is the emphasis that goes with
  // it, so the row reads as chosen at a glance and not only under the eye.
  'data-[selected]:font-medium data-[selected]:text-accent',
  'data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
  '[&_svg]:size-3.5 [&_svg]:shrink-0',
])

/** The root. `multiple` turns `value` into an array; otherwise controlled with
 * `value` and `onValueChange`, or left to manage itself. */
export const Select = Base.Root

/** What the trigger shows: the selected item's label, and the `placeholder`
 * the product gives it until there is one.
 *
 * Two traps, both Base UI's and both quiet. Its `children` is a *function* of
 * the value, not a node - passing a node pins the trigger to that node
 * forever and the selection never appears, so the placeholder goes in
 * `placeholder`. And what it shows is the raw value, `plum` rather than
 * `Plum`, unless the root is given an `items` map to look the label up in.
 *
 * It truncates, and that matters most for `multiple`: nine chosen fruits are
 * one long string, and without this the trigger either grows into a paragraph
 * or spills its text past its own border. One line, an ellipsis, and the full
 * set is still in the list where the ticks are. */
export function SelectValue({ className, ...props }: Base.Value.Props) {
  return <Base.Value className={cn('min-w-0 flex-1 truncate text-left', className)} {...props} />
}

/** The chevron, or whatever the product puts there. Marked decorative by Base
 * UI, since the button is already named by its value. */
export const SelectIcon = Base.Icon

/** A labelled group of options. */
export const SelectGroup = Base.Group

/** The text of an option, which is what the trigger echoes when it is chosen. */
export const SelectItemText = Base.ItemText

/** The tick, drawn only on the chosen option. */
export const SelectItemIndicator = Base.ItemIndicator

export interface SelectTriggerProps
  extends Base.Trigger.Props,
    VariantProps<typeof selectTriggerVariants> {}

/** The control. A `<button role="combobox">` - never a `<select>`. */
export function SelectTrigger({ size, className, ...props }: SelectTriggerProps) {
  return <Base.Trigger className={cn(selectTriggerVariants({ size }), className)} {...props} />
}

export interface SelectPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof selectPopupVariants> {
  /** Preferred side of the trigger. Base UI flips it when it does not fit. */
  side?: Base.Positioner.Props['side']
  /** Alignment along that side. */
  align?: Base.Positioner.Props['align']
  /** Distance from the trigger, in pixels. */
  sideOffset?: Base.Positioner.Props['sideOffset']
  /** Where to portal to. Defaults to the document body, which keeps the list
   * from being clipped by a form with `overflow: hidden`. */
  container?: Base.Portal.Props['container']
}

/** The list. Portalled and positioned against the trigger.
 *
 * `alignItemWithTrigger` is off: Base UI's default lifts the popup so the
 * selected option sits over the button, which is the native macOS behaviour
 * and is disorienting in a web form - the list jumps to a different place
 * depending on what is already chosen. */
export function SelectPopup({
  size,
  side,
  align,
  sideOffset = 4,
  container,
  className,
  children,
  ...props
}: SelectPopupProps) {
  return (
    <Base.Portal container={container}>
      <Base.Positioner
        side={side}
        align={align}
        sideOffset={sideOffset}
        alignItemWithTrigger={false}
        className="[z-index:var(--z-menu)]"
      >
        <Base.Popup className={cn(selectPopupVariants({ size }), className)} {...props}>
          <Base.List>{children}</Base.List>
        </Base.Popup>
      </Base.Positioner>
    </Base.Portal>
  )
}

/** An option, with the tick that says it is the chosen one.
 *
 * The indicator is built in rather than left to the caller. The item already
 * reserves the room for it (`pr-7`), and a dropdown that does not show what is
 * currently selected is the commonest complaint about a styled select: it
 * opens, and the reader has to remember what they picked last time. Passing
 * `indicator={false}` turns it off for a list where the choice is obvious
 * some other way. */
export function SelectItem({
  indicator = true,
  className,
  children,
  ...props
}: Base.Item.Props & { indicator?: boolean }) {
  return (
    <Base.Item className={cn(selectItemVariants(), className)} {...props}>
      {children}
      {indicator && (
        <Base.ItemIndicator className="absolute right-2 flex text-accent">
          <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden>
            <path
              d="M3.5 8.5l3 3 6-6.5"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </Base.ItemIndicator>
      )}
    </Base.Item>
  )
}

/** The caption above a group. */
export function SelectGroupLabel({ className, ...props }: Base.GroupLabel.Props) {
  return (
    <Base.GroupLabel
      className={cn('px-2 py-1.5 text-2xs uppercase tracking-caption text-faint', className)}
      {...props}
    />
  )
}

/** A line between groups of options. */
export function SelectSeparator({ className, ...props }: Base.Separator.Props) {
  return <Base.Separator className={cn('-mx-1 my-1 h-px bg-line', className)} {...props} />
}
