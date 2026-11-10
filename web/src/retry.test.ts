import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { retrying } from './retry'

describe('retrying', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('answers at once when the first answer is good', async () => {
    const attempt = vi.fn(() => Promise.resolve([1]))
    await expect(retrying(attempt)).resolves.toEqual([1])
    expect(attempt).toHaveBeenCalledTimes(1)
  })

  it('asks again after a failure, and keeps the answer that came', async () => {
    const attempt = vi.fn().mockRejectedValueOnce(new Error('starting')).mockResolvedValueOnce([2])
    const answer = retrying(attempt, undefined, () => false, [10])
    await vi.advanceTimersByTimeAsync(10)
    await expect(answer).resolves.toEqual([2])
    expect(attempt).toHaveBeenCalledTimes(2)
  })

  it('asks again while the answer is suspect, and returns the last one when it stays so', async () => {
    const attempt = vi.fn(() => Promise.resolve<number[]>([]))
    const answer = retrying(attempt, undefined, stars => stars.length === 0, [10, 20])
    await vi.advanceTimersByTimeAsync(30)
    await expect(answer).resolves.toEqual([])
    expect(attempt).toHaveBeenCalledTimes(3)
  })

  it('stops asking once a suspect answer turns good', async () => {
    const attempt = vi.fn().mockResolvedValueOnce([]).mockResolvedValueOnce([3]).mockResolvedValue([4])
    const answer = retrying(attempt, undefined, (stars: number[]) => stars.length === 0, [10, 20])
    await vi.advanceTimersByTimeAsync(30)
    await expect(answer).resolves.toEqual([3])
    expect(attempt).toHaveBeenCalledTimes(2)
  })

  it('says the failure once the pauses are spent, rather than an empty answer', async () => {
    const attempt = vi.fn(() => Promise.reject(new Error('down')))
    const answer = retrying(attempt, undefined, () => false, [10, 20])
    const settled = expect(answer).rejects.toThrow('down')
    await vi.advanceTimersByTimeAsync(30)
    await settled
    expect(attempt).toHaveBeenCalledTimes(3)
  })

  it('gives up at once when the question is withdrawn', async () => {
    const abort = new AbortController()
    const attempt = vi.fn(() => Promise.reject(new Error('down')))
    const answer = retrying(attempt, abort.signal, () => false, [1000])
    const settled = expect(answer).rejects.toBeDefined()
    await vi.advanceTimersByTimeAsync(10)
    abort.abort(new Error('moved on'))
    await settled
    await vi.advanceTimersByTimeAsync(2000)
    expect(attempt).toHaveBeenCalledTimes(1)
  })
})
