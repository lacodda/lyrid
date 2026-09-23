/**
 * A route: stars in an order, walked one after another.
 *
 * The rules are small and live here so the panel, the address and the sky all
 * apply the same ones. A route is a list of ids and nothing else — the order is
 * the meaning, a stop may repeat, and it never grows past what an address can
 * carry.
 */

import { MAX_ROUTE } from './location'

/** The route with a star added at the end, or unchanged when it is full. */
export function addStop(route: readonly number[], id: number): number[] {
  if (route.length >= MAX_ROUTE) return [...route]
  // Adding the stop that already ends the route would draw a line of zero
  // length and step to the same card twice; any earlier visit is kept, since
  // coming back to a star is a thing a route may do.
  if (route[route.length - 1] === id) return [...route]
  return [...route, id]
}

/** The route without the stop at `index`. */
export function removeStop(route: readonly number[], index: number): number[] {
  return route.filter((_, at) => at !== index)
}

/**
 * The stop after (or before) the one being looked at, wrapping neither way:
 * the end of a route is an end, and stepping past it back to the start would
 * hide that the walk is over.
 */
export function step(route: readonly number[], at: number, by: 1 | -1): number | null {
  const next = at + by
  return next >= 0 && next < route.length ? next : null
}
