import { Fragment, useEffect, useState, type ReactNode } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import { cn } from 'dowel-ui'

import { fetchArtist, type Alongside, type Artist, type Link, type Nebula, type Neighbour, type OnLabel } from '@/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { panelVariants, SectionLabel } from '@/components/ui/panel'
import { Spinner } from '@/components/ui/spinner'
import { useLanguage } from '@/lib/language'
import { fetchHeard, type Heard } from '@/scrobbling'
import type { Side } from './Compare'
import type { Dossier } from './location'
import { Spectrum } from './Spectrum'
import { reasons } from './why'

interface Props {
  artistId: number
  className?: string
  onClose: () => void
  /** Opens another star by id — a neighbour named on this card. */
  onOpen: (id: number) => void
  /** Adds this star to the end of the route. */
  onAddToRoute: (side: Side) => void
  /** The star held for a comparison, if one is. */
  pinned: Side | null
  /** Holds this star for a comparison with the next one opened. */
  onPin: (side: Side) => void
  /** Compares this star with the held one. */
  onCompare: (side: Side) => void
  /** Plays this star's channel in the one player. */
  onListen: (uploads: string, name: string) => void
  /** Starts the radio of a nebula. */
  onRadio: (nebula: Nebula) => void
  /** Opens a station this star belongs to: a label it released on, or where it comes from. */
  onDossier: (dossier: Dossier) => void
  /** The star the player is sounding, so the card can say it is this one. */
  sounding: number | null
  /**
   * Whether the person looking has listening to ask about -- signed in, with
   * ListenBrainz linked or once linked. Without it the card asks nothing
   * personal at all.
   */
  listener: boolean
}

/**
 * What a star turns out to be.
 *
 * Every block comes from a different source — the name and years from
 * MusicBrainz, the lead from Wikipedia, origin and influence from Wikidata,
 * genres from Discogs with a release count behind each, the spectrum from
 * AcousticBrainz, the neighbours from co-listening — which is the point: the
 * card is where the import pipelines meet on one screen.
 *
 * Each block hides itself when its source has nothing. Most artists in a canon
 * of three million have no encyclopaedia article and no influence links, so an
 * empty section is the normal case, not a failure to render.
 */
export function StarCard({ artistId, className, onClose, onOpen, onAddToRoute, pinned, onPin, onCompare, onListen, onRadio, onDossier, sounding, listener }: Props) {
  const { t } = useTranslation()
  const [artist, setArtist] = useState<Artist | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [added, setAdded] = useState(false)

  useEffect(() => {
    const abort = new AbortController()
    // No reset here: App gives this component a key of the artist id, so a
    // different star mounts a fresh card rather than clearing this one.
    fetchArtist(artistId, abort.signal)
      .then(setArtist)
      .catch((cause: unknown) => {
        if (abort.signal.aborted) return
        setError(cause instanceof Error ? cause.message : t('card.failed'))
      })
    return () => abort.abort()
  }, [artistId, t])

  // Computed unconditionally, before any branch: the facts are strings from
  // `t`, and calling for them inside the JSX below would put a hook after an
  // early return.
  const facts = artist ? factsOf(artist, t) : null

  return (
    <aside
      className={cn(
        panelVariants(),
        'glass relative flex w-[min(22rem,calc(100vw-3rem))] flex-col gap-2 overflow-y-auto p-4',
        className
      )}
    >
      <Button variant="icon" size="icon-sm" className="absolute right-2 top-2" onClick={onClose} aria-label={t('card.close')}>
        ×
      </Button>

      {!artist && !error && (
        <p className="flex items-center gap-2 text-xs text-dim">
          <Spinner size="sm" label={t('card.reading')} />
          {t('card.reading')}
        </p>
      )}
      {error && (
        <p role="alert" className="text-xs text-bad">
          {error}
        </p>
      )}

      {artist && (
        <>
          <h2 className="m-0 pr-8 text-xl font-semibold tracking-tight text-text">{artist.name}</h2>
          {artist.comment && <p className="m-0 text-xs text-dim">{artist.comment}</p>}

          {facts && <Facts facts={facts} scene={artist.origin?.place ? (artist.origin.qid ?? null) : null} onScene={qid => onDossier({ kind: 'scene', qid })} />}

          {listener && <HeardLine artistId={artist.id} />}

          {/* The two instruments that act on this star and another: walking
              to it as part of a route, and holding it up against a second. */}
          <div className="flex flex-wrap gap-1.5">
            <Button
              size="sm"
              onClick={() => {
                onAddToRoute({ id: artist.id, name: artist.name })
                setAdded(true)
              }}
            >
              {added ? t('card.addedToRoute') : t('card.addToRoute')}
            </Button>
            {pinned && pinned.id !== artist.id ? (
              <Button size="sm" onClick={() => onCompare({ id: artist.id, name: artist.name })}>
                {t('card.compareWith', { name: pinned.name })}
              </Button>
            ) : (
              <Button size="sm" disabled={pinned?.id === artist.id} onClick={() => onPin({ id: artist.id, name: artist.name })}>
                {pinned?.id === artist.id ? t('card.pinned') : t('card.pin')}
              </Button>
            )}
          </div>

          {artist.prose && (
            <div className="flex flex-col gap-1.5">
              <Extract text={artist.prose.extract} />
              {/* The credit is not decoration and not optional: the extract
                  above is CC BY-SA, and this line is the condition on which it
                  may be shown at all. It renders with the words or not at
                  all, because the two arrive as one value. */}
              <p className="m-0 text-2xs text-faint">
                <Trans
                  i18nKey="card.credit"
                  values={{ title: artist.prose.source_title, licence: artist.prose.licence }}
                  components={[
                    <a key="source" className="text-dim underline underline-offset-2" href={artist.prose.source_url} target="_blank" rel="noreferrer" />,
                  ]}
                />
              </p>
            </div>
          )}

          {artist.genres.length > 0 && (
            <ul className="m-0 flex list-none flex-wrap gap-1 p-0">
              {artist.genres.map(genre => (
                <li key={`${genre.name}-${String(genre.is_style)}`}>
                  <Badge variant="soft" className="gap-1">
                    {genre.name}
                    {/* The weight is what makes the genre honest: how many of
                        this artist's releases carry it. */}
                    <span className="text-faint">{genre.releases}</span>
                  </Badge>
                </li>
              ))}
            </ul>
          )}

          {artist.spectrum && <Spectrum spectrum={artist.spectrum} />}

          <Labels labels={artist.labels} onOpen={id => onDossier({ kind: 'label', id })} />

          {artist.releases.length > 0 && (
            <>
              <SectionLabel>{t('card.releases')}</SectionLabel>
              <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
                {artist.releases.map(release => (
                  <li key={`${release.name}-${String(release.year)}`} className="flex items-baseline justify-between gap-2 text-xs">
                    <span className="text-text">{release.name}</span>
                    <span className="shrink-0 text-faint">{release.year ?? ''}</span>
                  </li>
                ))}
              </ul>
            </>
          )}

          <Listen
            links={artist.listen}
            uploads={artist.youtube_uploads}
            radio={artist.radio}
            playing={sounding === artist.id}
            onPlay={uploads => onListen(uploads, artist.name)}
            onRadio={onRadio}
          />

          {/* Influence is directed, so the two lists are separate claims and
              never merged into one "related" pile. */}
          <Names heading={t('card.shapedBy')} people={artist.influenced_by} onOpen={onOpen} />
          <Names heading={t('card.wentOnToShape')} people={artist.influenced} onOpen={onOpen} />
          <Neighbours people={artist.similar.slice(0, 6)} onOpen={onOpen} />
        </>
      )}
    </aside>
  )
}

/**
 * What your own listening says about this star, when it says anything.
 *
 * Asked for separately from the card, by the person it concerns: the card
 * itself is the same for everybody and stays that way. A star never heard
 * shows nothing rather than "not heard yet" -- in a sky of two hundred
 * thousand stars that would be the line on almost every card.
 */
function HeardLine({ artistId }: { artistId: number }) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const [heard, setHeard] = useState<Heard | null>(null)

  useEffect(() => {
    const abort = new AbortController()
    fetchHeard(artistId, abort.signal)
      .then(setHeard)
      // Personal and optional: a card that could not say what you heard is
      // still the whole card.
      .catch(() => undefined)
    return () => abort.abort()
  }, [artistId])

  if (!heard || heard.listens === 0 || !heard.first_at) return null
  const since = new Date(heard.first_at).toLocaleDateString(resolved, { day: 'numeric', month: 'long', year: 'numeric' })
  return (
    <p className="m-0 text-xs text-light">
      {t('card.heard', { count: heard.listens, number: heard.listens.toLocaleString(resolved), since })}
    </p>
  )
}

/**
 * The lead, opened to its first paragraph.
 *
 * Leads run from 38 to 9,187 characters, median 362 — so most fit, and the
 * famous names people actually click do not: the Beatles' lead is 3,893
 * characters and would fill the card, pushing the genres, the discography and
 * the neighbours off the screen. The first paragraph is the summary Wikipedia
 * leads are written to have; the rest is there for whoever wants it.
 */
function Extract({ text }: { text: string }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  // Paragraphs arrive separated by a blank line, as the parser joins them.
  const [first, ...rest] = text.split('\n\n')
  if (rest.length === 0) return <p className="m-0 text-xs leading-relaxed text-text">{text}</p>

  return (
    <>
      <p className="m-0 text-xs leading-relaxed text-text">{expanded ? text : first}</p>
      <Button size="sm" className="self-start" onClick={() => setExpanded(!expanded)}>
        {expanded ? t('card.less') : t('card.more', { count: rest.length })}
      </Button>
    </>
  )
}

/**
 * Where to go and hear this artist.
 *
 * The links come from the canon rather than from a streaming API, which is
 * what keeps the card offline-shaped: nothing here waits on a service that
 * could be down or rate-limiting. The honest consequence is that they are
 * artist pages, not tracks — MusicBrainz relates an artist to a service, not
 * a recording to one.
 *
 * The channel plays in the one player rather than in the card, so it keeps
 * playing after the card closes; and the star's radio is offered beside it,
 * the nebula decided by the server rather than read off the genres above.
 */
function Listen({
  links,
  uploads,
  radio,
  playing,
  onPlay,
  onRadio,
}: {
  links: Link[]
  uploads: string | null
  radio: Nebula | null
  playing: boolean
  onPlay: (uploads: string) => void
  onRadio: (nebula: Nebula) => void
}) {
  const { t } = useTranslation()
  if (links.length === 0 && !radio) return null
  return (
    <>
      <SectionLabel>{t('card.listen')}</SectionLabel>
      {(uploads || radio) && (
        <div className="flex flex-wrap gap-1.5">
          {uploads && (
            <Button variant="soft" size="sm" disabled={playing} onClick={() => onPlay(uploads)}>
              {playing ? t('card.playing') : t('card.play')}
            </Button>
          )}
          {radio && (
            <Button size="sm" onClick={() => onRadio(radio)}>
              {t('listen.radioOf', { name: radio.name })}
            </Button>
          )}
        </div>
      )}
      <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-xs">
        {links.map(link => (
          <li key={link.url}>
            <a className="text-accent underline-offset-2 hover:underline" href={link.url} target="_blank" rel="noreferrer">
              {link.service}
            </a>
          </li>
        ))}
      </ul>
    </>
  )
}

function Names({ heading, people, onOpen }: { heading: string; people: Neighbour[]; onOpen: (id: number) => void }) {
  if (people.length === 0) return null
  return (
    <>
      <SectionLabel>{heading}</SectionLabel>
      <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-0.5 p-0 text-xs">
        {people.map(person => (
          <li key={person.id}>
            <NameButton name={person.name} onClick={() => onOpen(person.id)} />
          </li>
        ))}
      </ul>
    </>
  )
}

/**
 * The neighbours, each with why it is one.
 *
 * A name alone says only that people listen to the two together; the line
 * under it says what the canon knows about the pair — shared genres, and an
 * influence with its direction — which is what turns an edge on the map into
 * something learned.
 */
function Neighbours({ people, onOpen }: { people: Alongside[]; onOpen: (id: number) => void }) {
  const { t } = useTranslation()
  if (people.length === 0) return null
  return (
    <>
      <SectionLabel>{t('card.alongside')}</SectionLabel>
      <ul className="m-0 flex list-none flex-col gap-1 p-0 text-xs">
        {people.map(person => {
          const said = reasons(person.why, t)
          return (
            <li key={person.id} className="flex flex-col">
              <NameButton name={person.name} onClick={() => onOpen(person.id)} />
              {said.length > 0 && <span className="text-2xs text-dim">{said.join(' · ')}</span>}
            </li>
          )
        })}
      </ul>
    </>
  )
}

/** A name that opens something: a link in look, a button in behaviour. */
function NameButton({ name, onClick, inline = false }: { name: string; onClick: () => void; inline?: boolean }) {
  return (
    <button
      type="button"
      className={cn('cursor-pointer text-left text-accent underline-offset-2 hover:underline', !inline && 'self-start')}
      onClick={onClick}
    >
      {name}
    </button>
  )
}

/** The one-line summary, in three parts: what, where from, and when. */
interface FactLine {
  kind: string | null
  place: string | null
  years: string
}

/**
 * The line under the name, with the place a way into its scene: "formed in
 * Manchester" opens Manchester, the stars that come from there.
 */
function Facts({ facts, scene, onScene }: { facts: FactLine; scene: number | null; onScene: (qid: number) => void }) {
  const parts: ReactNode[] = []
  if (facts.kind) parts.push(facts.kind)
  if (facts.place) {
    parts.push(scene === null ? facts.place : <NameButton key="place" name={facts.place} onClick={() => onScene(scene)} inline />)
  }
  if (facts.years) parts.push(facts.years)
  if (parts.length === 0) return null
  return (
    <p className="m-0 text-xs text-dim">
      {parts.map((part, index) => (
        // The three parts have fixed places, so a place is their identity.
        <Fragment key={index}>
          {index > 0 && ' · '}
          {part}
        </Fragment>
      ))}
    </p>
  )
}

/**
 * The labels this star released on, each a station of its own: the name
 * opens the label's dossier, and the years say when this star was on it.
 */
function Labels({ labels, onOpen }: { labels: OnLabel[]; onOpen: (id: number) => void }) {
  const { t } = useTranslation()
  if (labels.length === 0) return null
  return (
    <>
      <SectionLabel>{t('card.labels')}</SectionLabel>
      <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
        {labels.map(label => (
          <li key={label.id} className="flex items-baseline justify-between gap-2 text-xs">
            <NameButton name={label.name} onClick={() => onOpen(label.id)} />
            <span className="shrink-0 text-faint">{labelYears(label)}</span>
          </li>
        ))}
      </ul>
    </>
  )
}

function labelYears({ first_year: first, last_year: last }: OnLabel): string {
  if (first === null) return ''
  return last === null || last === first ? String(first) : `${String(first)}–${String(last)}`
}

/**
 * The one-line summary under the name.
 *
 * Origin comes from Wikidata and area from MusicBrainz, and they answer
 * different questions — a city against a country — so the more specific one
 * wins when both are known rather than both being printed.
 *
 * Takes `t` rather than calling `useTranslation` itself: the place and the
 * years are sentences now, not fragments joined by hand — "born in Seattle"
 * and "formed in Seattle" are different claims in every language, and which
 * preposition goes where is the translator's business rather than this file's.
 */
function factsOf(artist: Artist, t: Translate): FactLine {
  return { kind: artist.kind, place: place(artist, t), years: years(artist, t) }
}

type Translate = ReturnType<typeof useTranslation>['t']

function place(artist: Artist, t: Translate): string | null {
  const origin = artist.origin
  if (!origin?.place) return artist.area
  // "Formed in Seattle" and "born in Seattle" are different claims, and the
  // card says which one it is showing rather than flattening both to "from".
  return origin.is_birth ? t('card.bornIn', { place: origin.place }) : t('card.formedIn', { place: origin.place })
}

function years(artist: Artist, t: Translate): string {
  // MusicBrainz is curated and wins over Wikidata's inception year; the
  // crowdsourced value only fills a gap rather than overwriting a fact.
  const begin = artist.begin_year ?? artist.origin?.inception_year
  if (!begin) return ''
  return artist.end_year ? t('card.years', { begin, end: artist.end_year }) : t('card.since', { year: begin })
}
