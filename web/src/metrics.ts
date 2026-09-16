/**
 * Telling the server a mechanic was used.
 *
 * Deliberately the thinnest thing that could work: one fire-and-forget POST
 * carrying nothing but the name of the mechanic, which is one of a fixed list
 * the server also holds. There is no queue, no batching, no session id and no
 * timestamp — every one of those would be a way for this to become a record of
 * what one person did, which is exactly what the charter promises it is not.
 *
 * Failures are ignored on purpose. A counter is not worth a retry, an error
 * message, or a single frame of the person's attention.
 */

/** The mechanics the server counts. Kept in step with `src/api/metrics.rs`. */
export type Mechanic = 'sky_opened' | 'card_opened' | 'listen_opened' | 'view_shared' | 'charter_read' | 'data_requested'

/**
 * Counts one use.
 *
 * `keepalive` so the request survives the page being closed in the same
 * moment — which is when "the view was shared" usually happens.
 */
export function count(mechanic: Mechanic): void {
  void fetch(`/api/metrics/${mechanic}`, { method: 'POST', keepalive: true }).catch(() => {
    // Nothing to do and nobody to tell: the person is here to look at the sky.
  })
}
