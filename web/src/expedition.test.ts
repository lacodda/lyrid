import { describe, expect, it } from 'vitest'

import { asksForHome, contractToShow, echoesOf, newlyFulfilled, type Contract, type Expedition } from './expedition'
import type { PersonalSky } from './sky/fog'

function contract(id: number, done: boolean, progress = 0): Contract {
  return {
    id,
    kind: 'home_sector',
    star: { mbid: 'm', id: 1, name: 'Home', x: 0, y: 0 },
    goal: 3,
    progress: done ? 3 : progress,
    issued_at: '2026-11-10T10:00:00Z',
    done_at: done ? '2026-11-10T11:00:00Z' : null,
  }
}

function expedition(...contracts: Contract[]): Expedition {
  return { home: null, contracts }
}

describe('contractToShow', () => {
  it('shows the contract still to do', () => {
    expect(contractToShow(expedition(contract(1, true), contract(2, false)), new Set(), new Set())?.id).toBe(2)
  })

  it('shows a contract fulfilled here until it is put away, and never one fulfilled before', () => {
    const done = expedition(contract(1, true))
    expect(contractToShow(done, new Set(), new Set())).toBeNull()
    expect(contractToShow(done, new Set([1]), new Set())?.id).toBe(1)
    expect(contractToShow(done, new Set([1]), new Set([1]))).toBeNull()
  })
})

describe('newlyFulfilled', () => {
  it('names the contracts that were open and are fulfilled now', () => {
    expect(newlyFulfilled(expedition(contract(1, false, 2)), expedition(contract(1, true)))).toEqual([1])
  })

  it('does not call a contract fulfilled long ago new, nor one still open', () => {
    expect(newlyFulfilled(expedition(contract(1, true)), expedition(contract(1, true)))).toEqual([])
    expect(newlyFulfilled(expedition(contract(1, false)), expedition(contract(1, false, 1)))).toEqual([])
    // The first answer of a visit has nothing before it to compare with.
    expect(newlyFulfilled(expedition(), expedition(contract(1, true)))).toEqual([])
  })
})

describe('echoesOf', () => {
  const sky: PersonalSky = {
    opened: [],
    echoes: [
      { artistId: 5, name: 'Five', x: 0, y: 0, of: [1] },
      { artistId: 6, name: 'Six', x: 0, y: 0, of: [2] },
      { artistId: 7, name: 'Seven', x: 0, y: 0, of: [1, 2] },
    ],
  }

  it('keeps the echoes the home port reflects, and only those', () => {
    expect(echoesOf(sky, 1).map(echo => echo.artistId)).toEqual([5, 7])
    expect(echoesOf(sky, null)).toEqual([])
  })
})

describe('asksForHome', () => {
  it('asks an explorer who has not set out, and nobody else', () => {
    expect(asksForHome({ mode: 'explore', home_port: null })).toBe(true)
    expect(asksForHome({ mode: 'explore', home_port: 'abc' })).toBe(false)
    expect(asksForHome({ mode: 'create', home_port: null })).toBe(false)
    expect(asksForHome({ mode: null, home_port: null })).toBe(false)
    expect(asksForHome(null)).toBe(false)
  })
})
