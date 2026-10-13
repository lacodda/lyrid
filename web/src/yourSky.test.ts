import { describe, expect, it } from 'vitest'

import { readYear } from './YourSky'

describe('readYear', () => {
  it('takes four digits that make a year someone listening now was born in', () => {
    expect(readYear('1983', 2026)).toBe(1983)
    expect(readYear(' 2001 ', 2026)).toBe(2001)
  })

  it('refuses anything the profile would refuse', () => {
    expect(readYear('83', 2026)).toBeNull()
    expect(readYear('1899', 2026)).toBeNull()
    expect(readYear('2027', 2026)).toBeNull()
    expect(readYear('19a3', 2026)).toBeNull()
    expect(readYear('', 2026)).toBeNull()
  })
})
