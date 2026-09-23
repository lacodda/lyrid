import type { ReactNode } from 'react'
import { Switch as Base } from '@base-ui/react/switch'
import { cn } from 'dowel-ui'

/*
 * Switch - a setting that takes effect when you flip it.
 *
 * The difference from Checkbox is not how it looks, and getting it wrong is
 * the commonest mistake in the pair. A checkbox is an answer collected now
 * and submitted later, with the rest of the form; a switch is a setting that
 * applies the moment it moves. Put a switch in a form with a Save button and
 * the reader cannot tell whether anything happened - they flipped it, and
 * nothing said so.
 *
 * The rule, then: if there is a Save button, it is a Checkbox. If the change
 * is the action, it is a Switch.
 *
 * The thumb slides with `--duration-quick`, and the track carries the accent
 * when on - the only colour in the control, so a row of settings reads as a
 * column of on-and-off rather than a field of decoration. Under reduced
 * motion the theme drops the transition; the position still changes, which is
 * the part that carries the meaning.
 */

export interface SwitchProps {
  /** The words next to the switch. A switch with no label is a light with no
   * caption - give `aria-label` if the meaning is genuinely in the context. */
  children?: ReactNode
  checked?: boolean
  defaultChecked?: boolean
  onCheckedChange?: (checked: boolean) => void
  disabled?: boolean
  required?: boolean
  readOnly?: boolean
  name?: string
  'aria-label'?: string
  className?: string
}

export function Switch({ children, className, ...props }: SwitchProps) {
  const control = (
    <Base.Root
      className={cn(
        'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-line-2 bg-soft',
        'transition-colors duration-quick ease-out',
        'hover:border-accent',
        'data-[checked]:border-accent data-[checked]:bg-accent',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
        'disabled:cursor-not-allowed disabled:opacity-50',
        children === undefined && className,
      )}
      {...props}
    >
      <Base.Thumb
        className={cn(
          'size-3.5 rounded-full bg-dim shadow-lift',
          'transition-[transform,background-color] duration-quick ease-out',
          'translate-x-0.5 data-[checked]:translate-x-[1.125rem]',
          'data-[checked]:bg-on-accent',
        )}
      />
    </Base.Root>
  )

  if (children === undefined) return control

  return (
    <label
      className={cn(
        'flex cursor-pointer items-center gap-2.5 text-sm text-text',
        'has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50',
        className,
      )}
    >
      {control}
      {children}
    </label>
  )
}
