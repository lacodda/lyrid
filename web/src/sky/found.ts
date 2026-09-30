import type { Found } from '@/api'

/** The three kinds of answer a search gives. */
export type Kind = 'stars' | 'labels' | 'scenes'

/**
 * The order the kinds are shown in: a kind holding an exact match for the term
 * first, then stars, labels and places as they come.
 *
 * Stars lead by default, because a name is usually an artist's. But "Detroit"
 * typed in full means the place, and "Motown" the label, far more often than
 * the dozen acts with the word somewhere in their names — and those acts would
 * otherwise push the answer below the fold.
 */
export function order(term: string, found: Found): Kind[] {
  const wanted = term.trim().toLowerCase()
  const exact = (names: { name: string }[]) => names.some(item => item.name.toLowerCase() === wanted)
  const kinds: [Kind, boolean][] = [
    ['stars', exact(found.stars)],
    ['labels', exact(found.labels)],
    ['scenes', exact(found.scenes)],
  ]
  // A stable sort: kinds with an exact match keep their order among themselves.
  return kinds.sort((a, b) => Number(b[1]) - Number(a[1])).map(([kind]) => kind)
}
