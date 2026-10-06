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
  /** The place's Wikidata item: the scene whose dossier this star belongs to. */
  qid: number | null
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
  /** What the star sounds like, from AcousticBrainz. `null` for the many stars never measured. */
  spectrum: Spectrum | null
  similar: Alongside[]
  origin: Origin | null
  /** The labels the act released on, most releases first. */
  labels: OnLabel[]
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

/** The bands a spectrum is drawn in, in the order the server sends them. */
export type SpectrumBandName = 'tempo' | 'energy' | 'mood' | 'danceable' | 'sound' | 'voice'

/** Where a star stands on one band, among all the stars AcousticBrainz measured: 0 below every one, 1 above. */
export interface SpectrumBand {
  name: SpectrumBandName
  rank: number
}

/** A star's spectrum: every band measured on the same recordings. */
export interface Spectrum {
  recordings: number
  /** Beats per minute, the median of the recordings' tempos. */
  bpm: number
  bands: SpectrumBand[]
}

/** A label on a card: a station this star belongs to. */
export interface OnLabel {
  id: number
  name: string
  /** How many of the artist's releases carry it. */
  releases: number
  first_year: number | null
  last_year: number | null
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

/** What a search finds: stars, and the stations they gather at. */
export interface Found {
  stars: Hit[]
  labels: LabelRef[]
  scenes: SceneRef[]
}

export async function search(term: string, signal?: AbortSignal): Promise<Found> {
  const response = await fetch(`/api/search?q=${encodeURIComponent(term)}`, { signal })
  if (!response.ok) throw new Error('the canon could not be searched')
  return (await response.json()) as Found
}

/** A label named elsewhere, and how many stars it stands for there. */
export interface LabelRef {
  id: number
  name: string
  artists: number
}

/** A place named elsewhere, and how many stars come from it. */
export interface SceneRef {
  qid: number
  name: string
  artists: number
}

/** A count in one year: releases on a label, or acts beginning in a place. */
export interface YearCount {
  year: number
  count: number
}

/** A style (or a genre) and how many of a roster have it as their main one. */
export interface Sound {
  name: string
  is_style: boolean
  artists: number
}

/**
 * Who is on a station: the first names, and every member's place.
 *
 * `map` holds `[id, x, y, brightness]` for every placed member, however many
 * are listed by name — the map is where a cluster shows itself.
 */
export interface Roster<T> {
  size: number
  listed: T[]
  map: [number, number, number, number][]
}

/** One run of a description, and what it leads to, if anything. */
export interface Segment {
  text: string
  /** A star on this sky. */
  star?: number
  /** A label with a dossier of its own. */
  label?: number
  /** A web address, only ever http or https. */
  url?: string
}

export interface LabelMember {
  id: number
  name: string
  releases: number
  first_year: number | null
  last_year: number | null
}

export interface SceneMember {
  id: number
  name: string
  /** Born here, rather than formed here. */
  born: boolean
  begin_year: number | null
  end_year: number | null
}

/** A label as a page. */
export interface LabelDossier {
  id: number
  name: string
  discogs_url: string
  /** Paragraphs of segments. */
  profile: Segment[][]
  parent: LabelRef | null
  sublabels: LabelRef[]
  /** Releases per year, over all the label's official releases. */
  chronology: YearCount[]
  roster: Roster<LabelMember>
  sound: Sound[]
  /** Where the roster comes from. */
  scenes: SceneRef[]
}

/** A place as a page. */
export interface SceneDossier {
  qid: number
  name: string
  wikidata_url: string
  formed: number
  born: number
  /** Acts beginning per year. */
  chronology: YearCount[]
  roster: Roster<SceneMember>
  sound: Sound[]
  /** The labels the scene's people released on. */
  labels: LabelRef[]
}

export async function fetchLabel(id: number, signal?: AbortSignal): Promise<LabelDossier> {
  const response = await fetch(`/api/labels/${String(id)}`, { signal })
  if (!response.ok) throw new Error(response.status === 404 ? 'no such label' : 'the canon could not be read')
  return (await response.json()) as LabelDossier
}

export async function fetchScene(qid: number, signal?: AbortSignal): Promise<SceneDossier> {
  const response = await fetch(`/api/scenes/${String(qid)}`, { signal })
  if (!response.ok) throw new Error(response.status === 404 ? 'no such scene' : 'the canon could not be read')
  return (await response.json()) as SceneDossier
}

/** One line of a comparison's genre mix: how much of each star's work carries a genre. */
export interface GenreMixLine {
  name: string
  is_style: boolean
  a: number
  b: number
}

/** Two stars side by side. */
export interface Comparison {
  why: Why
  genre_mix: GenreMixLine[]
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
