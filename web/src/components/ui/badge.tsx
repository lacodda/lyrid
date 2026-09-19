import type { HTMLAttributes } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'

/*
 * Badge.
 *
 * A small piece of state attached to something else: a count, a status, a
 * label. It is not a button and never was - if it can be clicked it is a Chip.
 *
 * Status colour is emphasis, never the message. A badge that means "failed"
 * says so in words as well, because colour alone is invisible to a reader who
 * does not separate red from green, and to anyone printing the screen.
 */
export const badgeVariants = cva(
  'inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs whitespace-nowrap',
  {
    variants: {
      variant: {
        outline: 'border border-line text-dim',
        soft: 'bg-soft font-medium text-dim',
        accent: 'bg-accent-soft font-medium text-accent',
        good: 'bg-good-soft font-medium text-good',
        warn: 'bg-warn-soft font-medium text-warn',
        bad: 'bg-bad-soft font-medium text-bad',
        info: 'bg-info-soft font-medium text-info',
      },
    },
    defaultVariants: { variant: 'outline' },
  },
)

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ variant, className, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />
}
