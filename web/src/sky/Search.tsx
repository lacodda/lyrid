import { useEffect, useRef, useState } from 'react'

import { searchArtists, type Hit } from '@/api'
import type { Star } from './renderer'

interface Props {
  onPick: (star: Star) => void
}

/**
 * Finding a star by name.
 *
 * Hits are ordered by how woven into the graph an artist is, so searching a
 * name shared by several acts leads with the one most people mean rather than
 * with whichever comes first alphabetically.
 */
export function Search({ onPick }: Props) {
  const [term, setTerm] = useState('')
  const [hits, setHits] = useState<Hit[]>([])
  const timer = useRef<number | undefined>(undefined)

  useEffect(() => {
    window.clearTimeout(timer.current)
    if (term.trim().length < 2) return

    // Debounced: a keystroke is not a query, and the canon has three million
    // rows to match against.
    const abort = new AbortController()
    timer.current = window.setTimeout(() => {
      searchArtists(term, abort.signal)
        .then(setHits)
        .catch(() => {
          if (!abort.signal.aborted) setHits([])
        })
    }, 180)

    return () => {
      window.clearTimeout(timer.current)
      abort.abort()
    }
  }, [term])

  return (
    <div className="search">
      <input
        className="search__input"
        value={term}
        onChange={event => {
          setTerm(event.target.value)
          if (event.target.value.trim().length < 2) setHits([])
        }}
        placeholder="find a star"
        aria-label="find a star"
        spellCheck={false}
      />

      {hits.length > 0 && (
        <ul className="search__hits">
          {hits.map(hit => (
            <li key={hit.id}>
              <button
                className="search__hit"
                onClick={() => {
                  onPick({ artistId: hit.id, x: hit.x ?? 0, y: hit.y ?? 0, brightness: 1 })
                  setTerm('')
                  setHits([])
                }}
              >
                <span className="search__hit-name">{hit.name}</span>
                {hit.comment && <span className="search__hit-comment">{hit.comment}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
