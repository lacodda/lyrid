import type { HTMLAttributes } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Spinner.
 *
 * Something is happening and the answer has not arrived. It carries no text of
 * its own - what is loading is the product's word, not the system's - but it
 * does have to say *something* to a screen reader, or a page that is busy is
 * silently identical to a page that is empty.
 *
 * Under `prefers-reduced-motion` the ring stands still, and says so itself
 * (`motion-reduce:animate-none`) rather than leaving it to the theme's rule,
 * which would run the turn once and stop wherever it ended. That is not a
 * detail: for some readers a spinning thing is not decoration but a symptom.
 * A still ring keeps its gap and still says "busy" through `role`, which is
 * the part that carried the meaning all along.
 */
export const spinnerVariants = cva('inline-block animate-spin motion-reduce:animate-none rounded-full border-2 border-current', {
  variants: {
    size: {
      sm: 'size-3.5 border-[1.5px]',
      md: 'size-4',
      lg: 'size-6',
    },
    tone: {
      /* Follows the text it sits in. */
      current: 'text-current',
      dim: 'text-dim',
      accent: 'text-accent',
    },
  },
  defaultVariants: { size: 'md', tone: 'current' },
})

export interface SpinnerProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof spinnerVariants> {
  /** What is being waited for, for a screen reader. The product's word. */
  label?: string
}

export function Spinner({ size, tone, label, className, ...props }: SpinnerProps) {
  return (
    <span role="status" aria-live="polite" className={cn('inline-flex items-center gap-2', className)} {...props}>
      <span
        // The gap in the ring is what makes the rotation visible.
        className={cn(spinnerVariants({ size, tone }), 'border-r-transparent')}
        aria-hidden
      />
      {label && <span className="sr-only">{label}</span>}
    </span>
  )
}
