import type { HTMLAttributes } from 'react'
import { cn } from 'dowel-ui'

/*
 * Truncate.
 *
 * Text that does not fit, cut with an ellipsis - and, importantly, still
 * readable in full: the element carries its own text as a `title`, so hovering
 * shows what was cut. Every product wrote the one-line version of this and
 * none of them remembered the title.
 *
 * `lines` truncates after that many instead of one, which needs a different
 * mechanism (`line-clamp`) rather than a different value.
 */
export interface TruncateProps extends HTMLAttributes<HTMLSpanElement> {
  /** The text. A string, because the component has to be able to put it in a
   * `title` - arbitrary children could not be. */
  children: string
  /** Cut after this many lines. One by default. */
  lines?: number
  /** Say what the full text is on hover. On by default; turn it off where the
   * text is already visible elsewhere, or the tooltip is noise. */
  title?: string | undefined
}

export function Truncate({ children, lines = 1, className, title, ...props }: TruncateProps) {
  return (
    <span
      // The browser shows this only when the text is actually cut, so it costs
      // nothing when everything fits.
      title={title ?? children}
      className={cn(
        lines === 1 ? 'block truncate' : 'block overflow-hidden',
        className,
      )}
      style={
        lines > 1
          ? { display: '-webkit-box', WebkitLineClamp: lines, WebkitBoxOrient: 'vertical' }
          : undefined
      }
      {...props}
    >
      {children}
    </span>
  )
}
