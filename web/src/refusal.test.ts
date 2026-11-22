import { afterEach, describe, expect, it } from 'vitest'

import i18n from '@/i18n'
import { explain, Refusal, refusalOf, refusedAs } from '@/refusal'

const answer = (body: string, status = 400) => new Response(body, { status })

describe('reading what the server refused', () => {
  it('keeps the code, the English and the numbers apart', async () => {
    const refusal = await refusalOf(
      answer('{"code":"password_too_short","error":"a password needs at least 10 characters","min":10}'),
    )
    expect(refusal).toBeInstanceOf(Refusal)
    expect(refusal.code).toBe('password_too_short')
    expect(refusal.message).toBe('a password needs at least 10 characters')
    expect(refusal.values).toEqual({ min: 10 })
  })

  it('reads an answer that is not ours as a refusal with nothing to say', async () => {
    // A proxy's error page, or an empty body: no code, no words, and no
    // "undefined" for anyone to read.
    for (const body of ['<html>502</html>', '', '[]', '"text"', '{"error": 7}']) {
      const refusal = await refusalOf(answer(body, 502))
      expect(refusal.code).toBeNull()
      expect(refusal.message).toBe('')
    }
  })
})

describe('saying a refusal in the reader’s language', () => {
  afterEach(async () => {
    await i18n.changeLanguage('en')
  })

  it('says a known code from the catalogue, not the server’s English', async () => {
    await i18n.changeLanguage('ru')
    const refusal = new Refusal('token_invalid', 'that link is not valid any more - ask for a new one')
    expect(explain(refusal, 'account.failed')).toBe('эта ссылка больше не действует — попросите новую')
  })

  it('puts the numbers a refusal carries into its message', async () => {
    await i18n.changeLanguage('ru')
    const refusal = new Refusal('password_too_short', 'a password needs at least 12 characters', { min: 12 })
    expect(explain(refusal, 'account.failed')).toContain('12')
  })

  it('falls back to the server’s words for a code newer than the page', () => {
    const refusal = new Refusal('a_code_from_tomorrow', 'something new went wrong')
    expect(explain(refusal, 'account.failed')).toBe('something new went wrong')
  })

  it('falls back to the screen’s own words when the failure said nothing', async () => {
    await i18n.changeLanguage('ru')
    expect(explain(new Refusal(null, ''), 'account.failed')).toBe(i18n.t('account.failed'))
    // A failure that is not an answer at all: the network gone.
    expect(explain(new TypeError('Failed to fetch'), 'account.failed')).toBe(i18n.t('account.failed'))
  })
})

describe('telling one refusal from another', () => {
  it('matches by code and nothing else', () => {
    expect(refusedAs(new Refusal('not_admin', 'not an admin'), 'not_signed_in', 'not_admin')).toBe(true)
    expect(refusedAs(new Refusal('not_signed_in', 'not an admin'), 'not_admin')).toBe(false)
    expect(refusedAs(new Error('not_admin'), 'not_admin')).toBe(false)
    expect(refusedAs(new Refusal(null, 'not_admin'), 'not_admin')).toBe(false)
  })
})
