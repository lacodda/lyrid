import { useEffect, useState } from 'react'

import { fetchArtist, type Artist, type Neighbour, type Origin } from '@/api'

interface Props {
  artistId: number
  onClose: () => void
}

/**
 * What a star turns out to be.
 *
 * Every block comes from a different source — the name and years from
 * MusicBrainz, the lead from Wikipedia, origin and influence from Wikidata,
 * genres from Discogs with a release count behind each, the neighbours from
 * co-listening — which is the point: the card is where the import pipelines
 * meet on one screen.
 *
 * Each block hides itself when its source has nothing. Most artists in a canon
 * of three million have no encyclopaedia article and no influence links, so an
 * empty section is the normal case, not a failure to render.
 */
export function StarCard({ artistId, onClose }: Props) {
  const [artist, setArtist] = useState<Artist | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const abort = new AbortController()
    // No reset here: App gives this component a key of the artist id, so a
    // different star mounts a fresh card rather than clearing this one.
    fetchArtist(artistId, abort.signal)
      .then(setArtist)
      .catch((cause: unknown) => {
        if (abort.signal.aborted) return
        setError(cause instanceof Error ? cause.message : 'could not read this star')
      })
    return () => abort.abort()
  }, [artistId])

  return (
    <aside className="card">
      <button className="card__close" onClick={onClose} aria-label="close">
        ×
      </button>

      {!artist && !error && <p className="card__muted">reading…</p>}
      {error && <p className="card__muted card__muted--error">{error}</p>}

      {artist && (
        <>
          <h2 className="card__name">{artist.name}</h2>
          {artist.comment && <p className="card__comment">{artist.comment}</p>}

          <p className="card__facts">{facts(artist).join(' · ')}</p>

          {artist.prose && (
            <div className="card__prose">
              <p className="card__extract">{artist.prose.extract}</p>
              {/* The credit is not decoration and not optional: the extract
                  above is CC BY-SA, and this line is the condition on which it
                  may be shown at all. It renders with the words or not at
                  all, because the two arrive as one value. */}
              <p className="card__credit">
                From{' '}
                <a href={artist.prose.source_url} target="_blank" rel="noreferrer">
                  {artist.prose.source_title}
                </a>{' '}
                on Wikipedia, licensed {artist.prose.licence}
              </p>
            </div>
          )}

          {artist.genres.length > 0 && (
            <ul className="card__genres">
              {artist.genres.map(genre => (
                <li key={`${genre.name}-${String(genre.is_style)}`} className="card__genre">
                  {genre.name}
                  {/* The weight is what makes the genre honest: how many of
                      this artist's releases carry it. */}
                  <span className="card__genre-count">{genre.releases}</span>
                </li>
              ))}
            </ul>
          )}

          {artist.labels.length > 0 && (
            <>
              <h3 className="card__section">labels</h3>
              <p className="card__labels">{artist.labels.join(' · ')}</p>
            </>
          )}

          {artist.releases.length > 0 && (
            <>
              <h3 className="card__section">releases</h3>
              <ul className="card__releases">
                {artist.releases.map(release => (
                  <li key={`${release.name}-${String(release.year)}`} className="card__release">
                    <span className="card__release-name">{release.name}</span>
                    <span className="card__release-year">{release.year ?? ''}</span>
                  </li>
                ))}
              </ul>
            </>
          )}

          {/* Influence is directed, so the two lists are separate claims and
              never merged into one "related" pile. */}
          <Names heading="shaped by" people={artist.influenced_by} />
          <Names heading="went on to shape" people={artist.influenced} />
          <Names heading="listened to alongside" people={artist.similar.slice(0, 6)} />
        </>
      )}
    </aside>
  )
}

function Names({ heading, people }: { heading: string; people: Neighbour[] }) {
  if (people.length === 0) return null
  return (
    <>
      <h3 className="card__section">{heading}</h3>
      <ul className="card__similar">
        {people.map(person => (
          <li key={person.id}>{person.name}</li>
        ))}
      </ul>
    </>
  )
}

/**
 * The one-line summary under the name.
 *
 * Origin comes from Wikidata and area from MusicBrainz, and they answer
 * different questions — a city against a country — so the more specific one
 * wins when both are known rather than both being printed.
 */
function facts(artist: Artist): string[] {
  const line = [artist.kind, place(artist), years(artist)]
  return line.filter((part): part is string => Boolean(part))
}

function place(artist: Artist): string | null {
  const origin: Origin | null = artist.origin
  if (!origin?.place) return artist.area
  // "Formed in Seattle" and "born in Seattle" are different claims, and the
  // card says which one it is showing rather than flattening both to "from".
  const verb = origin.is_birth ? 'born in' : 'formed in'
  return `${verb} ${origin.place}`
}

function years(artist: Artist): string {
  // MusicBrainz is curated and wins over Wikidata's inception year; the
  // crowdsourced value only fills a gap rather than overwriting a fact.
  const begin = artist.begin_year ?? artist.origin?.inception_year
  if (!begin) return ''
  return artist.end_year ? `${String(begin)}–${String(artist.end_year)}` : `since ${String(begin)}`
}
