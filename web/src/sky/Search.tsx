import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { search, type Found } from '@/api'
import { panelVariants } from '@/components/ui/panel'
import { SearchField } from '@/components/ui/search-field'
import { useLanguage } from '@/lib/language'
import { order } from './found'
import type { Dossier } from './location'
import type { Place } from './renderer'

interface Props {
  onPick: (star: Place) => void
  /** Opens a label or a scene the search found. */
  onDossier: (dossier: Dossier) => void
}

const NOTHING: Found = { stars: [], labels: [], scenes: [] }

/**
 * Finding a star, or a station, by name.
 *
 * Hits are ordered by how woven into the graph an artist is, so searching a
 * name shared by several acts leads with the one most people mean rather than
 * with whichever comes first alphabetically. Labels and places come after the
 * stars, each under its own heading: "Motown" the label and "Motown Sound" the
 * act are different kinds of answer, and the list says which is which rather
 * than ranking them against each other. A kind holding the exact name typed
 * leads — see `order`.
 */
export function Search({ onPick, onDossier }: Props) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const [term, setTerm] = useState('')
  const [found, setFound] = useState<Found>(NOTHING)
  const timer = useRef<number | undefined>(undefined)

  useEffect(() => {
    window.clearTimeout(timer.current)
    if (term.trim().length < 2) return

    // Debounced: a keystroke is not a query, and the canon has three million
    // rows to match against.
    const abort = new AbortController()
    timer.current = window.setTimeout(() => {
      search(term, abort.signal)
        .then(setFound)
        .catch(() => {
          if (!abort.signal.aborted) setFound(NOTHING)
        })
    }, 180)

    return () => {
      window.clearTimeout(timer.current)
      abort.abort()
    }
  }, [term])

  const done = () => {
    setTerm('')
    setFound(NOTHING)
  }
  const any = found.stars.length + found.labels.length + found.scenes.length > 0
  const count = (artists: number) => t('search.stars', { count: artists, number: artists.toLocaleString(resolved) })

  // Above the column under it: the card opens there and, coming later in the
  // page, would otherwise paint over the results it is asked to replace.
  return (
    <div className="absolute right-6 top-5 z-10 w-[min(22rem,calc(100vw-3rem))]">
      <SearchField
        className="glass"
        value={term}
        onValueChange={next => {
          setTerm(next)
          if (next.trim().length < 2) setFound(NOTHING)
        }}
        placeholder={t('search.placeholder')}
        aria-label={t('search.label')}
        clearLabel={t('search.clear')}
        shortcut={['Mod', 'K']}
        spellCheck={false}
      />

      {any && (
        <div className={cn(panelVariants(), 'glass mt-1.5 max-h-[60vh] overflow-y-auto p-1')}>
          {order(term, found).map((kind, index) => {
            if (kind === 'stars') {
              return (
                // Headed only when something else leads: at the top, a name
                // needs no label to be read as an artist's.
                <Group key={kind} heading={index === 0 ? undefined : t('search.starsHeading')}>
                  {found.stars.map(hit => (
                    <Result
                      key={hit.id}
                      name={hit.name}
                      note={hit.comment}
                      onClick={() => {
                        onPick({ artistId: hit.id, x: hit.x ?? 0, y: hit.y ?? 0 })
                        done()
                      }}
                    />
                  ))}
                </Group>
              )
            }
            if (kind === 'labels') {
              return (
                <Group key={kind} heading={t('search.labels')}>
                  {found.labels.map(label => (
                    <Result
                      key={label.id}
                      name={label.name}
                      note={count(label.artists)}
                      onClick={() => {
                        onDossier({ kind: 'label', id: label.id })
                        done()
                      }}
                    />
                  ))}
                </Group>
              )
            }
            return (
              <Group key={kind} heading={t('search.scenes')}>
                {found.scenes.map(scene => (
                  <Result
                    key={scene.qid}
                    name={scene.name}
                    note={count(scene.artists)}
                    onClick={() => {
                      onDossier({ kind: 'scene', qid: scene.qid })
                      done()
                    }}
                  />
                ))}
              </Group>
            )
          })}
        </div>
      )}
    </div>
  )
}

/** A run of results of one kind, headed when it is not the stars. */
function Group({ heading, children }: { heading?: string; children: ReactNode[] }) {
  if (children.length === 0) return null
  return (
    <section aria-label={heading}>
      {heading && <p className="caption m-0 px-2 pb-0.5 pt-2">{heading}</p>}
      <ul className="m-0 list-none p-0">{children}</ul>
    </section>
  )
}

function Result({ name, note, onClick }: { name: string; note: string | null; onClick: () => void }) {
  return (
    <li>
      <button
        type="button"
        className="block w-full cursor-pointer rounded-sm px-2 py-1.5 text-left hover:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-none"
        onClick={onClick}
      >
        <span className="block text-sm text-text">{name}</span>
        {note && <span className="block text-2xs text-dim">{note}</span>}
      </button>
    </li>
  )
}
