import { useEffect, useState } from 'react'

import { fetchArtist, type Artist } from '@/api'

/**
 * One star, small enough to put in someone else's page.
 *
 * `/embed/star/54` renders this and nothing else — no search box, no account
 * panel, no sky. An embed is a quotation: it shows the thing that was quoted
 * and offers a way to the whole, and a widget that brought the entire product
 * with it would be a page inside a page.
 *
 * The sky is deliberately not drawn here. A WebGL context per embed is a real
 * cost on a page carrying three of them, and what an embed is for is the
 * artist, not the map. The link out is where the map lives.
 */

export function Embed({ artistId }: { artistId: number }) {
  const [artist, setArtist] = useState<Artist | null>(null)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    const abort = new AbortController()
    fetchArtist(artistId, abort.signal).then(
      found => setArtist(found),
      () => setFailed(true)
    )
    return () => abort.abort()
  }, [artistId])

  if (failed) {
    return (
      <main className="embed">
        <p className="embed__empty">That star is not in this sky.</p>
      </main>
    )
  }

  if (!artist) {
    return (
      <main className="embed">
        <p className="embed__empty">One moment.</p>
      </main>
    )
  }

  const years = [artist.begin_year, artist.end_year].filter(year => year !== null)

  return (
    <main className="embed">
      {/* `target="_blank"` because the whole element is inside someone else's
          page: navigating the frame would leave a dead rectangle where the
          widget was, and the reader with no way back. */}
      <a className="embed__link" href={`/star/${String(artistId)}`} target="_blank" rel="noopener noreferrer">
        <h1 className="embed__name">{artist.name}</h1>
        {artist.comment && <p className="embed__comment">{artist.comment}</p>}

        <p className="embed__facts">
          {[artist.kind, artist.area, years.length > 0 ? years.join('–') : null].filter(Boolean).join(' · ')}
        </p>

        {artist.genres.length > 0 && (
          <p className="embed__genres">
            {artist.genres
              .slice(0, 4)
              .map(genre => genre.name)
              .join(' · ')}
          </p>
        )}

        <p className="embed__more">
          <img className="embed__mark" src="/favicon.svg" alt="" />
          see it in the sky
        </p>
      </a>
    </main>
  )
}
