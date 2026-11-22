import { afterEach, describe, expect, it, vi } from 'vitest'

import { fetchMe, saveProfile, worthSaving, type Camera } from './account'
import { Refusal } from './refusal'

describe('deciding when a camera is worth a round trip', () => {
  const at = (x: number, y: number, scale: number): Camera => ({ x, y, scale })

  it('saves the first camera there has ever been', () => {
    expect(worthSaving(null, at(0, 0, 1))).toBe(true)
  })

  it('does not save a camera that has barely moved', () => {
    // A pan of a few pixels is not a new place to reopen at, and the camera
    // changes on every frame: saving this would be a request per frame.
    expect(worthSaving(at(0, 0, 10), at(0.01, 0.01, 10))).toBe(false)
  })

  it('saves once the view has moved by half a screen', () => {
    // Half a screen at scale 10 is 0.05 world units; 0.2 is four screens.
    expect(worthSaving(at(0, 0, 10), at(0.2, 0, 10))).toBe(true)
  })

  it('measures the same pan differently at different zooms', () => {
    // The rule is about what the eye sees, not about world units: the same
    // 0.02 is nothing when zoomed out and a long way when zoomed in.
    expect(worthSaving(at(0, 0, 1), at(0.02, 0, 1))).toBe(false)
    expect(worthSaving(at(0, 0, 100), at(0.02, 0, 100))).toBe(true)
  })

  it('compares zoom as a ratio, not as a difference', () => {
    // Zoom is multiplicative. A fixed difference would treat 1 -> 2 as
    // nothing and 50 -> 51 as a change, when it is the other way round.
    expect(worthSaving(at(0, 0, 1), at(0, 0, 2))).toBe(true)
    expect(worthSaving(at(0, 0, 50), at(0, 0, 51))).toBe(false)
    expect(worthSaving(at(0, 0, 100), at(0, 0, 50))).toBe(true)
  })
})

describe('asking who is signed in', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reads an anonymous visitor as nobody, not as a failure', async () => {
    // The sky is public: 401 is the ordinary answer for a visitor, and
    // treating it as an error would put a red message on a working page.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{"code":"not_signed_in","error":"not signed in"}', { status: 401 })))
    await expect(fetchMe()).resolves.toBeNull()
  })

  it('treats a real fault as one', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{"error":"nope"}', { status: 500 })))
    await expect(fetchMe()).rejects.toThrow()
  })

  it('sends the session cookie', async () => {
    // Same-origin credentials are what carry the HttpOnly cookie; without
    // this every request is anonymous and nobody is ever signed in.
    const fetcher = vi.fn().mockResolvedValue(new Response('{}', { status: 200 }))
    vi.stubGlobal('fetch', fetcher)
    await fetchMe()
    expect(fetcher.mock.calls[0]?.[1]).toMatchObject({ credentials: 'same-origin' })
  })
})

describe('reporting what the server said', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('rejects with the refusal the server gave, code and all', async () => {
    // The code is what the screen says it by, in the reader's language; the
    // English rides along for a code the page does not know yet.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response('{"code":"halo_shape_unknown","error":"that marker shape is not one of ours"}', { status: 400 }))
    )
    const failure: unknown = await saveProfile({ halo_shape: 'ring' }).catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(Refusal)
    expect((failure as Refusal).code).toBe('halo_shape_unknown')
    expect((failure as Refusal).message).toBe('that marker shape is not one of ours')
  })

  it('does not show undefined when the body is not ours', async () => {
    // A proxy answering with an HTML error page must not become the string
    // "undefined" in front of the user: the refusal carries nothing, and the
    // screen says its own words for what it was doing.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('<html>502</html>', { status: 502 })))
    const failure: unknown = await saveProfile({ halo_shape: 'ring' }).catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(Refusal)
    expect((failure as Refusal).code).toBeNull()
  })

  it('says why the account could not be read, rather than a fixed English line', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response('{"code":"profile_unreadable","error":"the profile could not be read"}', { status: 500 }))
    )
    await expect(fetchMe()).rejects.toMatchObject({ code: 'profile_unreadable' })
  })

  it('sends only the fields it was given', async () => {
    // Saving a camera must not clear a marker the caller never touched.
    const fetcher = vi.fn().mockResolvedValue(new Response('{}', { status: 200 }))
    vi.stubGlobal('fetch', fetcher)
    await saveProfile({ camera: { x: 1, y: 2, scale: 3 } })
    expect(JSON.parse(String(fetcher.mock.calls[0]?.[1]?.body))).toEqual({ camera: { x: 1, y: 2, scale: 3 } })
  })
})
