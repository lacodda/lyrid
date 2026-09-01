/**
 * The account, as the browser sees it.
 *
 * The session lives in an HttpOnly cookie, which means this file can never
 * read it: whether someone is signed in is answered by asking the server, not
 * by looking in storage. That is the point of the cookie being HttpOnly — a
 * script that could read the token is a script that can leak it — and it is
 * why every call here is `credentials: 'same-origin'` and why the app starts
 * by asking `/api/me` rather than by consulting a local flag.
 */

/** The two modes, chosen once when the account is made and never again. */
export const MODES = ['create', 'explore'] as const

export type Mode = (typeof MODES)[number]

/** Where the sky was left. */
export interface Camera {
  x: number
  y: number
  scale: number
}

/** Who is signed in, and what they asked to be remembered. */
export interface Me {
  id: number
  email: string
  mode: Mode
  halo_shape: string | null
  halo_colour: string | null
  /** Absent when nothing is saved, or when the sky has been rebuilt since. */
  camera: Camera | null
}

/** What may be saved back. Deliberately without a mode. */
export interface ProfileUpdate {
  halo_shape?: string
  halo_colour?: string
  camera?: Camera
}

/**
 * Who is signed in, or `null` when nobody is.
 *
 * A 401 is the ordinary answer for a visitor, not a failure: the sky is
 * public, and being anonymous is a supported state rather than an error to
 * report. Anything else is a real fault and is thrown.
 */
export async function fetchMe(signal?: AbortSignal): Promise<Me | null> {
  const response = await fetch('/api/me', { signal, credentials: 'same-origin' })
  if (response.status === 401) return null
  if (!response.ok) throw new Error('the account could not be read')
  return (await response.json()) as Me
}

export async function register(email: string, password: string, mode: Mode): Promise<Me> {
  return send('/api/auth/register', { email, password, mode })
}

export async function logIn(email: string, password: string): Promise<Me> {
  return send('/api/auth/login', { email, password })
}

export async function logOut(): Promise<void> {
  await fetch('/api/auth/logout', { method: 'POST', credentials: 'same-origin' })
}

/**
 * Saves part of the profile and returns the whole of it.
 *
 * Only the named fields are sent, so saving a camera does not overwrite a
 * marker the caller never touched.
 */
export async function saveProfile(update: ProfileUpdate): Promise<Me> {
  const response = await fetch('/api/me', {
    method: 'PATCH',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(update),
  })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Me
}

async function send(path: string, body: Record<string, string>): Promise<Me> {
  const response = await fetch(path, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!response.ok) throw new Error(await messageOf(response))
  return (await response.json()) as Me
}

/**
 * The server's own words for what went wrong.
 *
 * The API answers a refusal with `{"error": "..."}` written for a person to
 * read, so it is shown as it is. A response that carries no such message —
 * a proxy's HTML error page, a truncated body — falls back to something
 * honest rather than showing "undefined" to the user.
 */
async function messageOf(response: Response): Promise<string> {
  try {
    const body: unknown = await response.json()
    if (typeof body === 'object' && body !== null) {
      const error = (body as Record<string, unknown>).error
      if (typeof error === 'string' && error !== '') return error
    }
  } catch {
    // Not JSON: falls through to the generic message below.
  }
  return 'something went wrong'
}

/**
 * Whether a camera is worth saving over the one already stored.
 *
 * The camera moves on every pan and zoom; sending each frame would be a
 * request per frame. A move is worth a round trip only once it is big enough
 * that reopening at the old place would feel wrong — a screen's worth of
 * panning, or a real change of zoom.
 *
 * Scale is compared as a ratio rather than a difference, because zoom is
 * multiplicative: 1 → 2 is the same move as 50 → 100, and a fixed threshold
 * would be every frame at one end of the range and never at the other.
 */
export function worthSaving(saved: Camera | null, now: Camera): boolean {
  if (!saved) return true
  // A move of more than half a screen at the current zoom. The world distance
  // that covers shrinks as you zoom in, which is what makes one rule work at
  // every zoom.
  const moved = Math.hypot(now.x - saved.x, now.y - saved.y) * now.scale
  if (moved > 0.5) return true
  const ratio = now.scale / saved.scale
  return ratio > 1.25 || ratio < 0.8
}
