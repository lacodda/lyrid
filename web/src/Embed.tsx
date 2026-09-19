import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
  const { t } = useTranslation()
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
    return <Frame>{t('embed.missing')}</Frame>
  }

  if (!artist) {
    return <Frame>{t('embed.loading')}</Frame>
  }

  const years = [artist.begin_year, artist.end_year].filter(year => year !== null)

  return (
    <main className="flex h-full items-stretch bg-void p-3">
      {/* `target="_blank"` because the whole element is inside someone else's
          page: navigating the frame would leave a dead rectangle where the
          widget was, and the reader with no way back. */}
      <a
        className="glass flex w-full flex-col gap-0.5 px-4 py-3.5 text-text no-underline hover:border-accent"
        href={`/star/${String(artistId)}`}
        target="_blank"
        rel="noopener noreferrer"
      >
        <h1 className="m-0 text-lg font-semibold tracking-tight">{artist.name}</h1>
        {artist.comment && <p className="m-0 text-2xs text-dim">{artist.comment}</p>}

        <p className="m-0 text-2xs text-dim">
          {[artist.kind, artist.area, years.length > 0 ? years.join('–') : null].filter(Boolean).join(' · ')}
        </p>

        {artist.genres.length > 0 && (
          <p className="m-0 text-2xs text-accent">
            {artist.genres
              .slice(0, 4)
              .map(genre => genre.name)
              .join(' · ')}
          </p>
        )}

        <p className="mt-auto mb-0 flex items-center gap-1.5 pt-2.5 text-2xs text-dim">
          <img className="size-3.5" src="/favicon.svg" alt="" />
          {t('embed.more')}
        </p>
      </a>
    </main>
  )
}

/** The rectangle with one sentence in it: loading, or nothing to show. */
function Frame({ children }: { children: React.ReactNode }) {
  return (
    <main className="flex h-full items-stretch bg-void p-3">
      <p className="m-auto text-xs text-dim">{children}</p>
    </main>
  )
}
