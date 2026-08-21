/** Shape of `GET /health` as served by the axum backend. */
export interface Health {
  status: 'ok' | 'degraded'
  version: string
  database: 'ok' | 'unavailable'
}

/**
 * Same-origin fetch: in development Vite proxies these paths to the API, and
 * in production the SPA is served by the same server.
 *
 * A degraded backend answers 503 with a valid body, so the status code alone
 * does not decide whether the payload is usable.
 */
export async function fetchHealth(signal?: AbortSignal): Promise<Health> {
  const response = await fetch('/health', { signal })
  const body: unknown = await response.json()
  if (!isHealth(body)) {
    throw new Error('unexpected /health payload')
  }
  return body
}

function isHealth(value: unknown): value is Health {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.status === 'string' && typeof candidate.version === 'string' && typeof candidate.database === 'string'
}

/** A genre as Discogs sees it, with the weight behind it. */
export interface Genre {
  name: string
  is_style: boolean
  releases: number
}

/** A neighbour in the similarity graph. */
export interface Neighbour {
  id: number
  name: string
  score: number
}

/** Everything a card shows about one star. */
export interface Artist {
  id: number
  mbid: string
  name: string
  comment: string | null
  kind: string | null
  area: string | null
  begin_year: number | null
  end_year: number | null
  position: { x: number; y: number; brightness: number } | null
  genres: Genre[]
  similar: Neighbour[]
}

/** A search result, with a place to fly to. */
export interface Hit {
  id: number
  name: string
  comment: string | null
  x: number | null
  y: number | null
}

export async function fetchArtist(id: number, signal?: AbortSignal): Promise<Artist> {
  const response = await fetch(`/api/artists/${String(id)}`, { signal })
  if (!response.ok) throw new Error(response.status === 404 ? 'no such artist' : 'the canon could not be read')
  return (await response.json()) as Artist
}

export async function searchArtists(term: string, signal?: AbortSignal): Promise<Hit[]> {
  const response = await fetch(`/api/search?q=${encodeURIComponent(term)}`, { signal })
  if (!response.ok) throw new Error('the canon could not be searched')
  return (await response.json()) as Hit[]
}
