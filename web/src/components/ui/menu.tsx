import { Menu as Base } from '@base-ui/react/menu'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Menu.
 *
 * A list of actions that opens from a button - the row menu, the overflow
 * menu, the one behind the three dots. The products all wrote this by hand,
 * and all of them wrote the same three hundred lines: a click-outside
 * listener, an Escape handler, and a `stopPropagation` on every item so that
 * choosing an action does not also open the row underneath.
 *
 * None of that is what makes a menu hard. What makes it hard is the keyboard:
 * arrows that wrap, Home and End, type-ahead that finds an item by its first
 * letters, a submenu that opens on the right key and closes when the pointer
 * leaves diagonally. Base UI has all of it.
 *
 * The items are exposed rather than taken as an array of `{ label, onSelect }`.
 * An array is enough until the first separator, the first checkbox item and
 * the first submenu - and each of those arrives as another field on the object
 * rather than as the JSX it obviously is.
 */

export const menuPopupVariants = cva(
  [
    'min-w-40 rounded-md border border-line bg-raise p-1 text-text shadow-raise',
    'focus-visible:outline-none',
    '[transition:opacity_var(--duration-quick)_var(--ease-out),transform_var(--duration-quick)_var(--ease-out)]',
    'data-[closed]:scale-[0.98] data-[closed]:opacity-0',
    'data-[starting-style]:scale-[0.98] data-[starting-style]:opacity-0',
  ],
  {
    variants: {
      size: {
        sm: 'min-w-32',
        md: 'min-w-40',
        lg: 'min-w-56',
      },
    },
    defaultVariants: { size: 'md' },
  },
)

/** One row of the menu. `danger` draws the destructive one apart from the
 * rest - in the colour of something that cannot be undone. */
export const menuItemVariants = cva(
  [
    'flex cursor-pointer select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm',
    'outline-none transition-colors',
    // Base UI marks the item under the pointer or the keyboard the same way,
    // so one rule covers both and they cannot disagree.
    'data-[highlighted]:bg-soft data-[highlighted]:text-text',
    'data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    '[&_svg]:size-3.5 [&_svg]:shrink-0',
  ],
  {
    variants: {
      tone: {
        default: 'text-dim data-[highlighted]:text-text',
        danger: 'text-bad data-[highlighted]:bg-bad-soft data-[highlighted]:text-bad',
      },
    },
    defaultVariants: { tone: 'default' },
  },
)

/** The root. Uncontrolled by default; `open` and `onOpenChange` make it
 * controlled. */
export const Menu = Base.Root

/** What opens it. Give it `render` to use your own button. */
export const MenuTrigger = Base.Trigger

/** A labelled group of items, for a menu long enough to need headings. */
export const MenuGroup = Base.Group

/** A submenu, opened from a `MenuSubTrigger` inside the parent. */
export const MenuSub = Base.SubmenuRoot

export interface MenuPopupProps
  extends Base.Popup.Props,
    VariantProps<typeof menuPopupVariants> {
  /** Preferred side of the trigger. Base UI flips it when it does not fit. */
  side?: Base.Positioner.Props['side']
  /** Alignment along that side. */
  align?: Base.Positioner.Props['align']
  /** Distance from the trigger, in pixels. */
  sideOffset?: Base.Positioner.Props['sideOffset']
  /** Where to portal to. Defaults to the document body, which keeps the menu
   * from being clipped by a row with `overflow: hidden` - which is where most
   * hand-written ones go to die. */
  container?: Base.Portal.Props['container']
}

/** The panel. Portalled and positioned against the trigger. */
export function MenuPopup({
  size,
  side,
  align,
  sideOffset = 4,
  container,
  className,
  children,
  ...props
}: MenuPopupProps) {
  return (
    <Base.Portal container={container}>
      <Base.Positioner
        side={side}
        align={align}
        sideOffset={sideOffset}
        className="[z-index:var(--z-menu)]"
      >
        <Base.Popup className={cn(menuPopupVariants({ size }), className)} {...props}>
          {children}
        </Base.Popup>
      </Base.Positioner>
    </Base.Portal>
  )
}

export interface MenuItemProps extends Base.Item.Props, VariantProps<typeof menuItemVariants> {}

/** An action. Closing the menu afterwards is Base UI's default, which is
 * almost always right - `closeOnClick={false}` is for the one that is not. */
export function MenuItem({ tone, className, ...props }: MenuItemProps) {
  return <Base.Item className={cn(menuItemVariants({ tone }), className)} {...props} />
}

/** An item that opens a submenu. Drawn like any other, because it is one. */
export function MenuSubTrigger({ tone, className, ...props }: MenuItemProps) {
  return <Base.SubmenuTrigger className={cn(menuItemVariants({ tone }), className)} {...props} />
}

/** An item that carries a tick. The state is the caller's - a menu does not
 * remember anything. */
export function MenuCheckboxItem({ tone, className, ...props }: MenuItemProps) {
  return <Base.CheckboxItem className={cn(menuItemVariants({ tone }), className)} {...props} />
}

/** The tick itself, drawn only when the item is checked. */
export const MenuCheckboxIndicator = Base.CheckboxItemIndicator

/** A line between groups of items. Decorative, and marked as such: a screen
 * reader announcing "separator" between every pair of actions is noise. */
export function MenuSeparator({ className, ...props }: Base.Separator.Props) {
  return <Base.Separator className={cn('-mx-1 my-1 h-px bg-line', className)} {...props} />
}

/** The caption above a group. */
export function MenuGroupLabel({ className, ...props }: Base.GroupLabel.Props) {
  return (
    <Base.GroupLabel
      className={cn('px-2 py-1.5 text-2xs uppercase tracking-caption text-faint', className)}
      {...props}
    />
  )
}
