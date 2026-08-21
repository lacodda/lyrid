import { useEffect, useState } from 'react'

import { fetchArtist, type Artist } from '@/api'

interface Props {
  artistId: number
  onClose: () => void
}

/**
 * What a star turns out to be.
 *
 * Everything here comes from a different source — the name and the years from
 * MusicBrainz, the genres from Discogs with a release count behind each, the
 * neighbours from co-listening — which is the point: the card is where the
 * five import pipelines meet.
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

          <p className="card__facts">
            {[artist.kind, artist.area, years(artist)].filter(Boolean).join(' · ')}
          </p>

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

          {artist.similar.length > 0 && (
            <>
              <h3 className="card__section">listened to alongside</h3>
              <ul className="card__similar">
                {artist.similar.slice(0, 6).map(neighbour => (
                  <li key={neighbour.id}>{neighbour.name}</li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
    </aside>
  )
}

function years(artist: Artist): string {
  if (!artist.begin_year) return ''
  return artist.end_year ? `${artist.begin_year}–${artist.end_year}` : `since ${artist.begin_year}`
}
