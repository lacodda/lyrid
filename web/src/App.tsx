import { useCallback, useState } from 'react'

import { Sky, type SkyState } from '@/sky/Sky'
import { StarCard } from '@/sky/StarCard'
import { Search } from '@/sky/Search'
import type { Star } from '@/sky/renderer'

/**
 * The sky fills the window; everything else floats over it.
 *
 * The map is the product, not a panel inside a page — so the shell is
 * deliberately thin: a mark, a search box, a card when a star is picked, and
 * a line of numbers for whoever wants to know what they are looking at.
 */
export function App() {
  const [state, setState] = useState<SkyState | null>(null)
  const [picked, setPicked] = useState<Star | null>(null)

  // Stable callbacks: the render loop lives outside React, and a new function
  // every render would tear the canvas down and build it again.
  const onState = useCallback((next: SkyState) => setState(next), [])
  const onPick = useCallback((star: Star | null) => setPicked(star), [])

  return (
    <main className="app">
      <Sky onState={onState} onPick={onPick} />

      <header className="app__header">
        <img className="app__mark" src="/mark.svg" alt="" />
        <div>
          <h1 className="app__title">lyrid</h1>
          <p className="app__tagline">a music universe</p>
        </div>
      </header>

      <Search onPick={setPicked} />

      {/* Keyed by the star: picking another one mounts a fresh card rather
          than leaving the previous artist on screen while the new one loads. */}
      {picked && <StarCard key={picked.artistId} artistId={picked.artistId} onClose={() => setPicked(null)} />}

      {state && (
        <p className="app__status">
          {state.stars.toLocaleString('en')} stars · level {state.level} · v{__APP_VERSION__}
        </p>
      )}
    </main>
  )
}
