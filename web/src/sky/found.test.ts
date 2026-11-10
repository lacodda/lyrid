import { describe, expect, it } from 'vitest'

import type { Found } from '@/api'
import { order } from './found'

const found = (stars: string[], labels: string[], scenes: string[]): Found => ({
  stars: stars.map((name, id) => ({ id, mbid: `00000000-0000-0000-0000-${String(id).padStart(12, '0')}`, name, comment: null, x: 0, y: 0 })),
  labels: labels.map((name, id) => ({ id, name, artists: 1 })),
  scenes: scenes.map((name, qid) => ({ qid, name, artists: 1 })),
})

describe('order', () => {
  it('leads with stars when nothing matches exactly', () => {
    expect(order('detr', found(['Detroit Emeralds'], ['Detroit Underground'], ['Detroit']))).toEqual(['stars', 'labels', 'scenes'])
  })

  it('leads with the kind that holds the exact name', () => {
    expect(order('Detroit', found(['Detroit Emeralds'], ['Detroit Underground'], ['Detroit']))).toEqual(['scenes', 'stars', 'labels'])
    expect(order(' motown ', found(['Motown Sound'], ['Motown'], []))).toEqual(['labels', 'stars', 'scenes'])
  })

  it('keeps stars first when a star is the exact name too', () => {
    expect(order('nirvana', found(['Nirvana'], ['Nirvana'], []))).toEqual(['stars', 'labels', 'scenes'])
  })
})
