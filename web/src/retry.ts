/**
 * Asking again when the first answer was not one.
 *
 * A page opened in the seconds a server starts, or just after a stand's canon
 * was replaced under it, gets errors or an empty sky for its first questions --
 * and the nearby list, the compass, the radio of the place and the edition in
 * the status line used to keep that first answer until the person happened to
 * move. Found twice on 2026-10-22. A fresh visitor never saw it; a person
 * arriving at the wrong second saw a sky that said there was nothing in it.
 *
 * So the questions a first view depends on go through this: a failure is asked
 * again after a pause that grows, and an answer the caller calls suspect --
 * empty where the first view is never empty -- is asked again the same way.
 * Past the last pause the failure is the caller's to say plainly, rather than
 * dressed up as an empty sky.
 */

/** Pauses before each further attempt, in milliseconds. */
export const RETRY_DELAYS: readonly number[] = [1000, 3000, 8000]

/**
 * The answer of `attempt`, asked again after each of `delays` while it fails or
 * `again` calls it suspect. Rejects with the last failure once the pauses are
 * spent, and at once when `signal` is aborted. A suspect answer that is still
 * suspect after the last pause is returned: it may simply be the truth.
 */
export async function retrying<T>(
  attempt: () => Promise<T>,
  signal?: AbortSignal,
  again: (answer: T) => boolean = () => false,
  delays: readonly number[] = RETRY_DELAYS
): Promise<T> {
  for (let tried = 0; ; tried++) {
    const last = tried >= delays.length
    try {
      const answer = await attempt()
      if (last || !again(answer)) return answer
    } catch (error) {
      if (signal?.aborted || last) throw error
    }
    await pause(delays[tried] ?? 0, signal)
  }
}

/** Resolves after `ms`, or rejects as soon as `signal` is aborted. */
function pause(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason as Error)
      return
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', stop)
      resolve()
    }, ms)
    const stop = () => {
      clearTimeout(timer)
      reject(signal?.reason as Error)
    }
    signal?.addEventListener('abort', stop, { once: true })
  })
}
