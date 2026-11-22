import i18n from '@/i18n'

/**
 * What the server says when it says no, and how the interface says it to the
 * reader.
 *
 * Every failure of the API answers `{"code": "...", "error": "...", ...}`: a
 * stable code, the server's English, and any number the message needs (see
 * the Refusals reference). The English is for logs and `curl`; the interface
 * says the code in the reader's language, from `errors.<code>` in the
 * catalogue, so a Russian screen is not left with an English line under its
 * form.
 */

/** A failure of the API, as it answered. */
export class Refusal extends Error {
  override readonly name = 'Refusal'
  /**
   * The stable code, or `null` when the answer carried none: a proxy's error
   * page, a body cut short.
   */
  readonly code: string | null
  /** The numbers its message is said with: `min`, `max`. */
  readonly values: Readonly<Record<string, unknown>>

  /** `phrase` is the server's English, or `''` when there was none. */
  constructor(code: string | null, phrase: string, values: Readonly<Record<string, unknown>> = {}) {
    super(phrase)
    this.code = code
    this.values = values
  }
}

/**
 * Reads a failed response into the refusal it carries.
 *
 * Never throws: an answer that is not ours — not JSON, not an object — is a
 * refusal with no code and no words, which `explain` says with the screen's
 * own fallback rather than as "undefined".
 */
export async function refusalOf(response: Response): Promise<Refusal> {
  let body: unknown
  try {
    body = await response.json()
  } catch {
    return new Refusal(null, '')
  }
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return new Refusal(null, '')
  const { code, error, ...values } = body as Record<string, unknown>
  return new Refusal(
    typeof code === 'string' && code !== '' ? code : null,
    typeof error === 'string' ? error : '',
    values,
  )
}

/**
 * The reader's words for a failure.
 *
 * In order: the catalogue's message for the code, in the reader's language;
 * the server's English, for a code newer than this page; the screen's own
 * `fallback` key, for a failure that said nothing — the network gone, a
 * proxy's page, a fault in the browser rather than an answer from the server.
 */
export function explain(failure: unknown, fallback: string): string {
  if (failure instanceof Refusal) {
    if (failure.code !== null) {
      const key = `errors.${failure.code}`
      if (i18n.exists(key)) return i18n.t(key, { ...failure.values })
    }
    if (failure.message !== '') return failure.message
  }
  return i18n.t(fallback)
}

/** Whether a failure is the refusal named by one of `codes`. */
export function refusedAs(failure: unknown, ...codes: string[]): boolean {
  return failure instanceof Refusal && failure.code !== null && codes.includes(failure.code)
}
