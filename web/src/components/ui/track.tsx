import type { HTMLAttributes, ReactNode } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from 'dowel-ui'
import { markerAt, place, type SegmentInput } from './track-segments'

/*
 * A bar divided into stretches, with something standing somewhere along it.
 *
 * Two products had written this independently and arrived at the same
 * construction - a rounded track, segments positioned absolutely by percent, a
 * floor under the segment width so a short one does not vanish - differing
 * only in what a segment meant. One drew the tiers of a rubric with the score
 * standing among them; the other drew a working day as alternating work and
 * breaks. Neither could be built from the other, and each knew something the
 * other did not: the tiers had the marker and the three-state reading of a
 * segment (passed, standing in, still ahead), the day had the minimum width
 * and the difference between an empty track and an unknown one.
 *
 * So it is one component, and the two are its two shapes:
 *
 *   **spans** - stretches of a whole, each meaning something in its own right:
 *   work and breaks, phases, occupancy. Adjacent or separated; gaps are the
 *   track showing through.
 *
 *   **thresholds** - a scale cut into bands, with a position on it. Here the
 *   segments are contiguous by construction and the point is not the bands but
 *   where you stand among them: "nearly a clip" is what the reader wants, and
 *   a badge saying which band you are in cannot say it.
 *
 * The arithmetic is in `track-segments`, importable without React.
 *
 * What this does NOT do is own its own height in pixels, and that is
 * deliberate: it is `h-1.5` in one donor and `h-2.5` in the other because the
 * bar carries different weight on the two screens. What it does own is the
 * geometry inside itself - percentages of its own box, never of a parent's -
 * which is the part that broke when a consumer put a percentage-height chart
 * inside a flex row and every bar resolved to zero.
 */

export const trackVariants = cva('relative w-full overflow-hidden rounded-full', {
  variants: {
    size: {
      /* Beside text, where the bar is a detail of a line. */
      sm: 'h-1.5',
      /* On its own row, where the bar is the thing being read. */
      md: 'h-2.5',
    },
  },
  defaultVariants: { size: 'md' },
})

export const trackSegmentVariants = cva('absolute top-0 h-full', {
  variants: {
    tone: {
      /* The subject: work done, the band you are standing in. */
      accent: 'bg-accent',
      /* Behind you, or secondary: a band already passed. Dimmed so the
       * current one carries the eye - a flat wash of "reached" over half the
       * bar says only that you are not at zero. */
      past: 'bg-accent/40',
      /* Ahead, or simply not the subject: a break, a band not yet reached. */
      idle: 'bg-line-2',
      /* Status, for a stretch that is itself a state rather than a quantity. */
      good: 'bg-good',
      warn: 'bg-warn',
      bad: 'bg-bad',
    },
  },
  defaultVariants: { tone: 'accent' },
})

export type TrackTone = NonNullable<VariantProps<typeof trackSegmentVariants>['tone']>

export interface TrackSegment extends SegmentInput {
  /** Distinguishes this segment from its neighbours in the DOM. */
  key: string
  tone?: TrackTone
  /** What this stretch is, in words. Shown on hover, and the only place the
   * segment's meaning exists for a reader who cannot see the colours. */
  label?: string
}

export interface TrackProps
  extends Omit<HTMLAttributes<HTMLDivElement>, 'children'>,
    VariantProps<typeof trackVariants> {
  segments: TrackSegment[]
  /** The value the left edge stands for. Defaults to the earliest segment. */
  from?: number
  /** The value the right edge stands for. Defaults to the latest segment. */
  to?: number
  /** Where the position marker stands, on the same scale as the segments.
   * Omitted when there is no such thing - a day of work has no "you are here". */
  marker?: number
  /** What the whole bar says, for a reader who cannot see it. Required: the
   * segments are decoration to a screen reader, and their titles are not
   * announced in order. */
  label: string
  /** Override the floor under a segment's width - `0` draws everything exactly
   * to scale.
   *
   * The floor is right for a day of work, where a short break is a fact worth
   * seeing. It is wrong wherever the widths are being compared to each other,
   * because a widened segment is no longer to scale and a reader measuring by
   * eye would be measuring the floor. */
  minWidth?: number
  /** Separate adjacent segments with a hairline of the ground.
   *
   * On for thresholds, where the bands touch and the boundary between two
   * reached ones would otherwise be invisible. Off for spans, where a gap in
   * the data is meant to look different from a gap between two stretches. */
  divided?: boolean
}

export function Track({
  segments,
  from,
  to,
  marker,
  label,
  minWidth,
  divided = false,
  size,
  className,
  ...props
}: TrackProps) {
  const placed = place(segments, { from, to, minWidth })

  /* The marker rides the same scale as the segments, so it is resolved against
   * the same bounds rather than against its own reading of them. */
  const bounds = {
    from: from ?? Math.min(...segments.map((s) => s.start), Infinity),
    to: to ?? Math.max(...segments.map((s) => s.end), -Infinity),
  }
  const at = marker === undefined ? null : markerAt(marker, bounds.from, bounds.to)

  return (
    <div
      role="img"
      aria-label={label}
      className={cn(trackVariants({ size }), 'bg-soft', className)}
      {...props}
    >
      {placed.map((geometry, index) => {
        const segment = segments[index]!
        return (
          <span
            key={segment.key}
            title={segment.label}
            style={{ left: `${geometry.left}%`, width: `${geometry.width}%` }}
            className={cn(
              trackSegmentVariants({ tone: segment.tone }),
              // The hairline is drawn in the page's own ground rather than in
              // a border colour, so it reads as a cut between two fills
              // instead of a third colour of its own.
              divided && index > 0 && 'border-l border-bg',
            )}
          />
        )
      })}

      {at !== null && (
        /* A dark core inside a light sheath, so the mark keeps its contrast
         * over the accent band it usually stands on as well as over the empty
         * road ahead. Centred on its position rather than starting at it: the
         * mark says "here", and a mark whose left edge is the position reads
         * as half a step further along than it is. */
        <span
          style={{ left: `${at}%` }}
          className="absolute top-1/2 h-2.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-bg ring-2 ring-text"
        />
      )}
    </div>
  )
}

/*
 * The labels under a track.
 *
 * Separate from the track because the two donors disagreed about whether there
 * are any - the day had none, the tiers had one per band - and because a caller
 * with three bands and a narrow column will want to drop them without giving
 * up the bar.
 */
export interface TrackScaleProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode
}

export function TrackScale({ className, ...props }: TrackScaleProps) {
  return (
    <div
      className={cn('flex justify-between text-2xs text-faint', className)}
      {...props}
    />
  )
}
