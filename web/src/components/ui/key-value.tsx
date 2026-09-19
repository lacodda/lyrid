import { createContext, useContext, type HTMLAttributes, type ReactNode } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * A list of facts: a name, and what it is.
 *
 * The shape every product builds out of two `<div>`s in a flex row, and the
 * reason it is worth having once: it is a `<dl>`, and the pairing is what a
 * screen reader announces. Two divs read as four unrelated pieces of text -
 * "Created", "2 hours ago", "Owner", "Ines" - and nothing says which value
 * belongs to which name. The right element says it for free.
 *
 * Two layouts, and both exist because the same list sits in two places: beside
 * something (a panel of properties, names in one column) and under a heading
 * (a card, name above value). A product moving a panel should not have to
 * change component.
 *
 * The layouts need *different markup*, and that is why the row reads the
 * layout from context rather than taking a prop:
 *
 *   **rows** puts `<dt>`/`<dd>` directly in the grid, so every value lines up
 *   in one column. Wrap each pair in a `<div>` and each pair becomes its own
 *   box - the values then sit at a different place on every line, which is
 *   exactly the ragged look this component exists to stop.
 *
 *   **stacked** wraps each pair, because without a wrapper the column gaps
 *   fall between `dt` and `dd` as readily as between pairs, and a list of six
 *   facts reads as twelve unrelated lines.
 *
 * Both are valid `<dl>`: the spec allows `dt`/`dd` as direct children, and
 * allows a `<div>` per pair. What it does not allow is any other element
 * between them, which is why the pair is a component rather than something the
 * caller assembles.
 */

export const keyValueVariants = cva('text-sm', {
  variants: {
    layout: {
      rows: 'grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] items-baseline gap-x-4 gap-y-2',
      stacked: 'flex flex-col gap-3',
    },
  },
  defaultVariants: { layout: 'rows' },
})

type Layout = NonNullable<VariantProps<typeof keyValueVariants>['layout']>

/* The row has to know the layout to know whether to wrap. Context rather than
 * a prop on every row: a list whose rows disagree with it is not a thing
 * anyone wants, and repeating the value at each row is how they come to. */
const LayoutContext = createContext<Layout>('rows')

export interface KeyValueProps
  extends HTMLAttributes<HTMLDListElement>,
    VariantProps<typeof keyValueVariants> {}

export function KeyValue({ layout, className, ...props }: KeyValueProps) {
  return (
    <LayoutContext.Provider value={layout ?? 'rows'}>
      <dl className={cn(keyValueVariants({ layout }), className)} {...props} />
    </LayoutContext.Provider>
  )
}

export interface KeyValueRowProps {
  /** The name of the fact. */
  label: ReactNode
  /** What it is. A node rather than a string: half the values in a real panel
   * are a Badge, a RelativeTime or a link. */
  children: ReactNode
  className?: string
}

/** One pair. */
export function KeyValueRow({ label, children, className }: KeyValueRowProps) {
  const layout = useContext(LayoutContext)

  const value = <dd className="min-w-0 text-text">{children}</dd>

  // Wrapped, so the column's gap falls between pairs rather than inside one,
  // and the caller's class dresses the pair as a whole.
  if (layout === 'stacked') {
    return (
      <div className={cn('flex flex-col gap-0.5', className)}>
        <dt className="text-dim">{label}</dt>
        {value}
      </div>
    )
  }

  /* Directly in the grid, so every value shares one column - there is no
   * wrapper here to carry the caller's class, so it dresses the term. That is
   * the half a caller styles: the value's own appearance comes from whatever
   * it renders. */
  return (
    <>
      <dt className={cn('text-dim', className)}>{label}</dt>
      {value}
    </>
  )
}
