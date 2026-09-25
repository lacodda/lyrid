/**
 * The official YouTube player, driven from here.
 *
 * The card used to hold a bare iframe of the channel, which can play but
 * cannot say when a song has ended or that a video refuses to be embedded — a
 * radio needs both. The IFrame Player API is YouTube's own embed with a script
 * beside it; it is not the Data API, so nothing here asks YouTube about the
 * canon, and ADR 0011 still holds: the ids come from MusicBrainz.
 *
 * The script is fetched on the first press of play and never before, the same
 * promise the card's player made: a visitor who never listens never meets
 * YouTube.
 */

/** The part of `YT.Player` this product uses. */
export interface YouTubePlayer {
  cuePlaylist(options: { list: string; listType: 'playlist'; index?: number }): void
  loadPlaylist(options: { list: string; listType: 'playlist'; index?: number }): void
  loadVideoById(videoId: string): void
  getPlaylist(): string[] | null
  destroy(): void
}

/** The player's states, as the API numbers them. */
export const STATE = { ended: 0, playing: 1, cued: 5 } as const

interface PlayerOptions {
  host: string
  width: string
  height: string
  playerVars: Record<string, number | string>
  events: {
    onReady: (event: { target: YouTubePlayer }) => void
    onStateChange: (event: { data: number; target: YouTubePlayer }) => void
    onError: (event: { data: number; target: YouTubePlayer }) => void
  }
}

interface YouTubeNamespace {
  Player: new (element: HTMLElement, options: PlayerOptions) => YouTubePlayer
}

declare global {
  interface Window {
    YT?: YouTubeNamespace
    onYouTubeIframeAPIReady?: () => void
  }
}

let loading: Promise<YouTubeNamespace> | null = null

/**
 * The API, loaded once for the whole page.
 *
 * A failed load is forgotten rather than kept, so the next press of play tries
 * again instead of every later press inheriting one bad moment on the network.
 */
export function loadYouTube(): Promise<YouTubeNamespace> {
  if (window.YT?.Player) return Promise.resolve(window.YT)
  loading ??= new Promise<YouTubeNamespace>((resolve, reject) => {
    const previous = window.onYouTubeIframeAPIReady
    window.onYouTubeIframeAPIReady = () => {
      previous?.()
      if (window.YT?.Player) resolve(window.YT)
      else reject(new Error('the YouTube player did not start'))
    }
    const script = document.createElement('script')
    script.src = 'https://www.youtube.com/iframe_api'
    script.async = true
    script.onerror = () => {
      loading = null
      script.remove()
      reject(new Error('the YouTube player could not be loaded'))
    }
    document.head.append(script)
  })
  return loading
}

/**
 * Makes a player in `element`.
 *
 * On `youtube-nocookie.com`, as the card's iframe was: the privacy-enhanced
 * host sets no cookies until a video actually plays.
 */
export function createPlayer(api: YouTubeNamespace, element: HTMLElement, events: PlayerOptions['events']): YouTubePlayer {
  return new api.Player(element, {
    host: 'https://www.youtube-nocookie.com',
    width: '100%',
    height: '100%',
    // `autoplay` because every player here is made by a press of play;
    // `rel: 0` keeps the end screen to the artist's own channel.
    playerVars: { autoplay: 1, rel: 0, playsinline: 1 },
    events,
  })
}
