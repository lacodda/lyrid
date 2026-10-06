export {}
// Code first, so the description below survives `shadcn add`: the CLI writes
// a file from its first token on and drops every comment above it.

/*
 * The arithmetic behind Track, with no React in it.
 *
 * Split out for the reason `table-sort` and `tree-rows` are: a product that
 * needs the numbers - to label a segment, to test its own domain code, to draw
 * the same shape somewhere that is not the DOM - should not import a component
 * to get them.
 *
 * What lives here is only the geometry. Turning a day of work into segments,
 * or a set of tiers into them, is the product's own arithmetic and stays with
 * the product: this knows about spans and percentages and nothing else.
 */

/** A segment as the caller states it, in whatever units the caller counts in. */
export interface SegmentInput {
  /** Where it starts. Same units as `end` and as the track's span. */
  start: number
  /** Where it ends. A segment ending before it starts is empty, not backwards. */
  end: number
}

/** A segment as it is drawn: percentages of the track. */
export interface Placed {
  /** Distance from the left edge, 0-100. */
  left: number
  /** Width, 0-100. */
  width: number
  /** Whether the width is the real one, or the floor standing in for it.
   *
   * Worth knowing rather than hiding: a caller labelling segments may want to
   * say "under a minute" instead of a duration the bar is no longer drawing to
   * scale. */
  widened: boolean
}

/** The smallest a segment may be drawn, in percent.
 *
 * A ten-second pause in an eight-hour day is 0.03% of the track, which rounds
 * to no pixels at all: the segment is real, was measured, and would simply not
 * be there. The floor is the width at which a sliver is still visible on a
 * track a few hundred pixels wide. */
export const MIN_SEGMENT_WIDTH = 0.6

export interface PlaceOptions {
  /** The value the left edge stands for. Defaults to the first segment's start. */
  from?: number
  /** The value the right edge stands for. Defaults to the last segment's end. */
  to?: number
  /** Override the floor - `0` to draw every segment exactly to scale. */
  minWidth?: number
}

/**
 * Lay segments out along a track, as percentages.
 *
 * The scale is stated by `from` and `to` rather than inferred, because the two
 * readings differ and both are wanted: a day of work is read against itself
 * (an eight-hour day drawn across a third of the width wastes the space where
 * the breaks are), while a set of tiers is read against the whole 0-100 scale,
 * where the distance to the next tier is the distance you have to close.
 *
 * Widening a sliver to the floor is what makes this worth having once. It also
 * introduces the only subtlety here: a widened segment can run into the one
 * after it, and two segments drawn overlapping is a worse lie than a segment
 * drawn slightly too wide. So the pass is done in order, and each segment is
 * held back to where the next one starts - a floor is a request, not a
 * guarantee, and the last one may be trimmed by the end of the track.
 */
export function place(segments: SegmentInput[], options: PlaceOptions = {}): Placed[] {
  if (segments.length === 0) return []

  const from = options.from ?? Math.min(...segments.map((s) => s.start))
  const to = options.to ?? Math.max(...segments.map((s) => s.end))
  const span = to - from

  // A track with no span has nothing to scale against: every segment would be
  // at the same place with the same width, which is not a drawing of anything.
  if (!(span > 0)) return []

  const floor = options.minWidth ?? MIN_SEGMENT_WIDTH

  const raw = segments.map((segment) => {
    const start = Math.min(Math.max(segment.start, from), to)
    const end = Math.min(Math.max(segment.end, start), to)
    return { left: ((start - from) / span) * 100, width: ((end - start) / span) * 100 }
  })

  if (floor <= 0) return raw.map((segment) => ({ ...segment, widened: false }))

  /*
   * Widening is a layout pass, not a per-segment decision, and the first
   * version of this got it wrong in a way worth recording: it capped each
   * sliver at "where the next segment starts", which protects against overlap
   * and also means a segment that touches its neighbour can never grow at all.
   * Both shapes this component exists for are contiguous - work then break
   * then work, one tier after another - so the floor did nothing for either of
   * them, and the tests missed it because their crowded case had a gap.
   *
   * So a widened segment pushes what follows instead. That borrows room the
   * track does not have, and the debt is paid back at the end by the segments
   * wide enough to afford it - never by another sliver, which would undo the
   * widening we just did.
   */
  const widened: Placed[] = []
  let shift = 0

  for (const segment of raw) {
    const left = segment.left + shift
    if (segment.width >= floor) {
      widened.push({ left, width: segment.width, widened: false })
      continue
    }
    shift += floor - segment.width
    widened.push({ left, width: floor, widened: true })
  }

  if (shift === 0) return widened

  /* The debt is only owed if the track has actually overflowed. A sliver on an
   * otherwise empty track - one segment, or several with gaps between them -
   * grows into room nobody was using, and nothing needs to give way. */
  const end = widened.at(-1)!
  const overflow = end.left + end.width - 100
  if (overflow <= 0) return widened

  /* Who can pay: everything above the floor, by however much it has to spare.
   * Taken in proportion, so one long stretch is not singled out to absorb the
   * whole debt while its neighbour keeps its exact width. */
  const spare = widened.reduce((sum, s) => sum + (s.widened ? 0 : Math.max(0, s.width - floor)), 0)

  /* Nobody can pay: every segment is at or under the floor, and the track is
   * genuinely too crowded to draw honestly at this width. The floor loses -
   * a bar running off its own end is worse than slivers too thin to see. */
  if (spare <= 0) return raw.map((segment) => ({ ...segment, widened: false }))

  let paid = 0
  return widened.map((segment) => {
    const left = segment.left - paid
    if (segment.widened) return { ...segment, left }

    const contribution = (Math.max(0, segment.width - floor) / spare) * overflow
    paid += contribution
    return { left, width: segment.width - contribution, widened: false }
  })
}

/** Where a marker sits on the same scale, as a percentage, or `null` when it
 * falls outside the track.
 *
 * Outside rather than clamped: a marker pinned to the edge says "here, at the
 * very end", which is a different statement from "not on this track at all". */
export function markerAt(value: number, from: number, to: number): number | null {
  const span = to - from
  if (!(span > 0)) return null
  if (value < from || value > to) return null
  return ((value - from) / span) * 100
}
