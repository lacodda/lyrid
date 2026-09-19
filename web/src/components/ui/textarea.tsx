import { useCallback, useLayoutEffect, useRef, type Ref, type TextareaHTMLAttributes } from 'react'
import { cn } from 'dowel-ui'
import { fieldClasses } from './input'

/*
 * Textarea.
 *
 * A multi-line field that can grow with what is typed into it, which is the
 * only interesting part: a fixed box makes someone scroll inside a scroll,
 * and a box that grows without limit pushes the button they are trying to
 * reach off the screen. `autoResize` grows it; `maxRows` says when to stop
 * and let it scroll after all.
 *
 * The measurement is the usual trick and worth stating: height is reset to
 * `auto` before reading `scrollHeight`, because a box already tall enough
 * reports its own height and never shrinks back.
 */
export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  ref?: Ref<HTMLTextAreaElement>
  /** Grow to fit the content instead of scrolling. */
  autoResize?: boolean
  /** Stop growing here, in lines, and scroll instead. */
  maxRows?: number
}

export function Textarea({
  className,
  ref,
  autoResize = false,
  maxRows,
  onChange,
  ...props
}: TextareaProps) {
  const own = useRef<HTMLTextAreaElement>(null)

  const resize = useCallback(() => {
    const element = own.current
    if (!element || !autoResize) return

    // Reset first: a box that is already tall enough reports its own height as
    // `scrollHeight` and would never shrink again.
    element.style.height = 'auto'

    const styles = getComputedStyle(element)
    const lineHeight = Number.parseFloat(styles.lineHeight) || 0
    const vertical =
      Number.parseFloat(styles.paddingTop) +
      Number.parseFloat(styles.paddingBottom) +
      Number.parseFloat(styles.borderTopWidth) +
      Number.parseFloat(styles.borderBottomWidth)

    const wanted = element.scrollHeight
    const ceiling = maxRows && lineHeight ? maxRows * lineHeight + vertical : Infinity

    element.style.height = `${Math.min(wanted, ceiling)}px`
    element.style.overflowY = wanted > ceiling ? 'auto' : 'hidden'
  }, [autoResize, maxRows])

  // Before paint, so the field never appears at the wrong height and then
  // jumps - including on the first render, when it may already have a value.
  useLayoutEffect(resize, [resize, props.value, props.defaultValue])

  return (
    <textarea
      ref={(element) => {
        own.current = element
        if (typeof ref === 'function') ref(element)
        else if (ref) ref.current = element
      }}
      onChange={(event) => {
        resize()
        onChange?.(event)
      }}
      className={cn(fieldClasses, autoResize ? 'resize-none' : 'resize-y', className)}
      {...props}
    />
  )
}
