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

/** Where an act comes from, as Wikidata records it. */
export interface Origin {
  place: string | null
  country: string | null
  /** A person's birthplace rather than a group's place of formation. */
  is_birth: boolean
  inception_year: number | null
}

/**
 * A Wikipedia lead and the credit its licence requires.
 *
 * The fields arrive together because the row they come from stores them
 * together: there is no server response carrying the extract alone, and the
 * card renders none of it without the attribution below it.
 */
export interface Prose {
  extract: string
  source_title: string
  source_url: string
  licence: string
}

/** One release group, as a discography line. */
export interface Release {
  name: string
  primary_type: string | null
  year: number | null
}

/** One outbound link, with the service named by the server. */
export interface Link {
  service: string
  url: string
}

/** Why two stars are near each other, in the canon's own words. */
export interface Why {
  /** Genres and styles both carry, most shared first, three at most. */
  genres: string[]
  /** The co-listening score of the edge, when there is one. */
  co_listening: number | null
  /** An influence between them, read from the first star's side. */
  influence: 'shaped_by' | 'went_on_to_shape' | 'mutual' | null
}

/** A neighbour in the similarity graph, and why it is one. */
export interface Alongside extends Neighbour {
  why: Why
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
  similar: Alongside[]
  origin: Origin | null
  labels: string[]
  /** Directed, so the two lists are different facts and stay apart. */
  influenced_by: Neighbour[]
  influenced: Neighbour[]
  prose: Prose | null
  releases: Release[]
  /** Where this artist can be heard, from the canon's own links. */
  listen: Link[]
  /** Playlist id that embeds the artist's YouTube channel, when there is one. */
  youtube_uploads: string | null
  /**
   * The radio this star belongs to, decided by the server: its main style when
   * that radio has enough to play, else its main genre. `null` when neither has
   * a channel to play.
   */
  radio: Nebula | null
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

/** One band of a comparison: how much of each star's work carries a genre. */
export interface SpectrumLine {
  name: string
  is_style: boolean
  a: number
  b: number
}

/** Two stars side by side. */
export interface Comparison {
  why: Why
  spectrum: SpectrumLine[]
  shared_neighbours: Neighbour[]
}

export async function fetchComparison(a: number, b: number, signal?: AbortSignal): Promise<Comparison> {
  const response = await fetch(`/api/compare?a=${String(a)}&b=${String(b)}`, { signal })
  if (!response.ok) throw new Error(response.status === 404 ? 'no such artist' : 'the canon could not be read')
  return (await response.json()) as Comparison
}

/**
 * Several stars by id, in the order asked, with their places: what a route
 * needs to draw itself. Ids the canon does not know are left out.
 */
export async function fetchStars(ids: readonly number[], signal?: AbortSignal): Promise<Hit[]> {
  if (ids.length === 0) return []
  const response = await fetch(`/api/stars?ids=${ids.join(',')}`, { signal })
  if (!response.ok) throw new Error('the canon could not be read')
  return (await response.json()) as Hit[]
}

/**
 * A star in view: enough to name it in a list and to open its card.
 *
 * Distinct from `Hit` although the fields nearly match: a hit has a place only
 * sometimes (a search can find an artist the layout left out), and a star in
 * view has one by definition — it was found *by* its place. Merging the two
 * would make the position optional for both and put a `?? 0` in the caller.
 */
export interface NearbyStar {
  id: number
  name: string
  comment: string | null
  x: number
  y: number
}

/** What a patch of sky is: the commonest main style and genre of its stars. */
export interface Region {
  style: string | null
  genre: string | null
  /** What listening here plays, decided the same way a card's radio is. */
  radio: Nebula | null
}

/** A genre or a style, by the name the sky writes on it. */
export interface Nebula {
  name: string
  kind: 'genre' | 'style'
}

/** A star that can be played: where it is, and its channel's uploads. */
export interface Station {
  id: number
  name: string
  x: number
  y: number
  uploads: string
}

/**
 * A nebula's radio: its stars with a channel, bright ones more often, in an
 * order the seed makes repeatable. Empty when the nebula has no channels.
 */
export async function fetchRadio(nebula: Nebula, seed: number, signal?: AbortSignal): Promise<Station[]> {
  const query = new URLSearchParams({ name: nebula.name, kind: nebula.kind, seed: String(seed) })
  const response = await fetch(`/api/radio?${query.toString()}`, { signal })
  if (!response.ok) throw new Error(response.status === 404 ? 'no such genre or style' : 'the sky could not be read')
  const body = (await response.json()) as { stations: Station[] }
  return body.stations
}

/**
 * The signal of the day: one faint star with a channel, the same for everyone
 * on the same calendar day, with the next few behind it for when a channel
 * will not play. Empty when the sky has nothing dark to play.
 */
export async function fetchSignal(day: string, signal?: AbortSignal): Promise<Station[]> {
  const response = await fetch(`/api/signal?day=${encodeURIComponent(day)}`, { signal })
  if (!response.ok) throw new Error('the sky could not be read')
  const body = (await response.json()) as { stars: Station[] }
  return body.stars
}

export async function fetchRegion(
  bounds: { minX: number; minY: number; maxX: number; maxY: number },
  signal?: AbortSignal
): Promise<Region> {
  const query = new URLSearchParams({
    min_x: String(bounds.minX),
    min_y: String(bounds.minY),
    max_x: String(bounds.maxX),
    max_y: String(bounds.maxY),
  })
  const response = await fetch(`/api/region?${query.toString()}`, { signal })
  if (!response.ok) throw new Error('the sky could not be read')
  return (await response.json()) as Region
}

/** The named stars inside a rectangle of the layout, most prominent first. */
export async function fetchNearby(
  bounds: { minX: number; minY: number; maxX: number; maxY: number },
  signal?: AbortSignal
): Promise<NearbyStar[]> {
  const query = new URLSearchParams({
    min_x: String(bounds.minX),
    min_y: String(bounds.minY),
    max_x: String(bounds.maxX),
    max_y: String(bounds.maxY),
  })
  const response = await fetch(`/api/nearby?${query.toString()}`, { signal })
  if (!response.ok) throw new Error('the sky could not be read')
  return (await response.json()) as NearbyStar[]
}
