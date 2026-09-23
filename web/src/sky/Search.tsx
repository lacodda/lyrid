import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { searchArtists, type Hit } from '@/api'
import { panelVariants } from '@/components/ui/panel'
import { SearchField } from '@/components/ui/search-field'
import type { Place } from './renderer'

interface Props {
  onPick: (star: Place) => void
}

/**
 * Finding a star by name.
 *
 * Hits are ordered by how woven into the graph an artist is, so searching a
 * name shared by several acts leads with the one most people mean rather than
 * with whichever comes first alphabetically.
 */
export function Search({ onPick }: Props) {
  const { t } = useTranslation()
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
    <div className="absolute right-6 top-5 w-[min(22rem,calc(100vw-3rem))]">
      <SearchField
        className="glass"
        value={term}
        onValueChange={next => {
          setTerm(next)
          if (next.trim().length < 2) setHits([])
        }}
        placeholder={t('search.placeholder')}
        aria-label={t('search.label')}
        clearLabel={t('search.clear')}
        shortcut={['Mod', 'K']}
        spellCheck={false}
      />

      {hits.length > 0 && (
        <ul className={cn(panelVariants(), 'glass mt-1.5 max-h-[60vh] list-none overflow-y-auto p-1')}>
          {hits.map(hit => (
            <li key={hit.id}>
              <button
                type="button"
                className="block w-full cursor-pointer rounded-sm px-2 py-1.5 text-left hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none"
                onClick={() => {
                  onPick({ artistId: hit.id, x: hit.x ?? 0, y: hit.y ?? 0 })
                  setTerm('')
                  setHits([])
                }}
              >
                <span className="block text-sm text-text">{hit.name}</span>
                {hit.comment && <span className="block text-2xs text-dim">{hit.comment}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
