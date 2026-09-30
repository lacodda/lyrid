import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import {
  fetchLabel,
  fetchScene,
  type LabelDossier,
  type LabelRef,
  type SceneDossier,
  type SceneRef,
  type Segment,
  type Sound,
  type YearCount,
} from '@/api'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogActions,
  DialogBody,
  DialogClose,
  DialogDescription,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { SectionLabel } from '@/components/ui/panel'
import { Spinner } from '@/components/ui/spinner'
import { useLanguage } from '@/lib/language'
import { memberAt, peak, yearDomain, yearTicks, yearX, type Member, type Span } from './roster'
import { useSkyField } from './field'
import type { Dossier as Target } from './location'
import { toMap } from './minimap'
import type { Overview } from './Sky'

interface Props {
  target: Target
  /** The whole sky small, once it has loaded: the map's backdrop. */
  overview: Overview | null
  onClose: () => void
  /** Flies to a star named in the dossier. */
  onOpenStar: (id: number) => void
  /** Opens another station's dossier in place of this one. */
  onOpenDossier: (target: Target) => void
  /** Marks the roster on the big sky and frames it, under the station's name. */
  onShowOnSky: (members: Member[], name: string) => void
}

/** A roster row: whoever it is, with the years it spans. */
interface Row {
  id: number
  name: string
  span: Span
  /** What sits on the right of the row: releases on a label, years in a scene. */
  aside: string
  /** Said when the name is pointed at: born here, or formed here. */
  note?: string
  /** A span with no end drawn open, as still going or not known to end. */
  open: boolean
}

/** Everything the two kinds of dossier share, once each is read. */
interface Shape {
  title: string
  lede: string
  profile: Segment[][]
  source: { url: string; name: string }
  chronology: YearCount[]
  chronologyLabel: string
  rows: Row[]
  size: number
  map: Member[]
  sound: Sound[]
  pointers: ReactNode
}

/**
 * A station opened into a page: a label, or a scene.
 *
 * The same three questions for both — who is here, when, and where on the
 * sky — then where to go next. It opens over the sky rather than instead of
 * it, like the spectrograph: the map is still there when it closes, and a
 * star named here is flown to on the map rather than described again here.
 */
export function Dossier({ target, overview, onClose, onOpenStar, onOpenDossier, onShowOnSky }: Props) {
  const { t } = useTranslation()
  const { resolved } = useLanguage()
  const [label, setLabel] = useState<LabelDossier | null>(null)
  const [scene, setScene] = useState<SceneDossier | null>(null)
  const [error, setError] = useState<string | null>(null)

  // Keyed by the caller on the station, so a new one mounts fresh rather than
  // showing the last one's roster while the next loads.
  useEffect(() => {
    const abort = new AbortController()
    const failed = (cause: unknown) => {
      if (abort.signal.aborted) return
      const missing = cause instanceof Error && cause.message.startsWith('no such')
      setError(missing ? t('dossier.missing') : t('dossier.failed'))
    }
    if (target.kind === 'label') fetchLabel(target.id, abort.signal).then(setLabel, failed)
    else fetchScene(target.qid, abort.signal).then(setScene, failed)
    return () => abort.abort()
  }, [target, t])

  const number = (value: number) => value.toLocaleString(resolved)
  const openLabel = (id: number) => onOpenDossier({ kind: 'label', id })
  const openScene = (qid: number) => onOpenDossier({ kind: 'scene', qid })

  let shape: Shape | null = null
  if (label) {
    const years = label.chronology
    const first = years[0]?.year
    const last = years.at(-1)?.year
    const releases = years.reduce((sum, entry) => sum + entry.count, 0)
    const parent = label.parent
    shape = {
      title: label.name,
      lede: [
        t('dossier.label'),
        first !== undefined && last !== undefined ? (first === last ? String(first) : t('dossier.years', { first, last })) : null,
        releases > 0 ? t('dossier.releases', { count: releases, number: number(releases) }) : null,
      ]
        .filter(Boolean)
        .join(' · '),
      profile: label.profile,
      source: { url: label.discogs_url, name: 'Discogs' },
      chronology: years,
      chronologyLabel: t('dossier.byYear'),
      rows: label.roster.listed.map(member => ({
        id: member.id,
        name: member.name,
        span: { first: member.first_year, last: member.last_year },
        aside: t('dossier.releases', { count: member.releases, number: number(member.releases) }),
        open: false,
      })),
      size: label.roster.size,
      map: label.roster.map,
      sound: label.sound,
      pointers: (
        <>
          {parent && (
            <Pointers heading={t('dossier.partOf')}>
              <Pointer name={parent.name} onClick={() => openLabel(parent.id)} />
            </Pointers>
          )}
          <LabelPointers heading={t('dossier.imprints')} labels={label.sublabels} onOpen={openLabel} number={number} />
          <ScenePointers heading={t('dossier.comesFrom')} scenes={label.scenes} onOpen={openScene} number={number} />
        </>
      ),
    }
  } else if (scene) {
    shape = {
      title: scene.name,
      lede: [t('dossier.scene'), t('dossier.formedHere', { number: number(scene.formed) }), t('dossier.bornHere', { number: number(scene.born) })].join(' · '),
      profile: [],
      source: { url: scene.wikidata_url, name: 'Wikidata' },
      chronology: scene.chronology,
      chronologyLabel: t('dossier.began'),
      rows: scene.roster.listed.map(member => ({
        id: member.id,
        name: member.name,
        span: { first: member.begin_year, last: member.end_year },
        aside: spanWords({ first: member.begin_year, last: member.end_year }, member.end_year === null, t),
        note: member.born ? t('dossier.born') : t('dossier.formed'),
        open: member.end_year === null,
      })),
      size: scene.roster.size,
      map: scene.roster.map,
      sound: scene.sound,
      pointers: <LabelPointers heading={t('dossier.releasedOn')} labels={scene.labels} onOpen={openLabel} number={number} />,
    }
  }

  return (
    <Dialog
      open
      onOpenChange={open => {
        if (!open) onClose()
      }}
    >
      <DialogPopup size="xl">
        <DialogHeader>
          <DialogTitle>{shape?.title ?? t('dossier.reading')}</DialogTitle>
          {shape && <DialogDescription>{shape.lede}</DialogDescription>}
        </DialogHeader>

        <DialogBody className="flex flex-col gap-3">
          {!shape && !error && (
            <p className="flex items-center gap-2 text-xs text-dim">
              <Spinner size="sm" label={t('dossier.reading')} />
              {t('dossier.reading')}
            </p>
          )}
          {error && (
            <p role="alert" className="m-0 text-xs text-bad">
              {error}
            </p>
          )}

          {shape && (
            <>
              {shape.profile.length > 0 && <Profile paragraphs={shape.profile} onOpenStar={onOpenStar} onOpenLabel={openLabel} />}

              <div className="grid gap-4 sm:grid-cols-[auto_1fr]">
                <RosterMap overview={overview} members={shape.map} rows={shape.rows} onOpenStar={onOpenStar} />
                <div className="flex min-w-0 flex-col gap-2">
                  <p className="m-0 text-sm text-text">{t('dossier.onTheSky', { count: shape.size, number: number(shape.size) })}</p>
                  {shape.map.length > 0 && (
                    <Button size="sm" className="self-start" onClick={() => onShowOnSky(shape.map, shape.title)}>
                      {t('dossier.showOnSky')}
                    </Button>
                  )}
                  {shape.sound.length > 0 && (
                    <>
                      <SectionLabel>{t('dossier.sound')}</SectionLabel>
                      <ul className="m-0 flex list-none flex-wrap gap-1 p-0">
                        {shape.sound.map(sound => (
                          <li key={`${sound.name}-${String(sound.is_style)}`}>
                            <Badge variant="soft" className="gap-1">
                              {sound.name}
                              <span className="text-faint">{number(sound.artists)}</span>
                            </Badge>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                  {shape.pointers}
                </div>
              </div>

              <History counts={shape.chronology} rows={shape.rows} size={shape.size} heading={shape.chronologyLabel} onOpenStar={onOpenStar} number={number} />
            </>
          )}
        </DialogBody>

        <DialogActions>
          {shape && (
            <a className="mr-auto self-center text-2xs text-dim underline underline-offset-2" href={shape.source.url} target="_blank" rel="noreferrer">
              {t('dossier.source', { name: shape.source.name })}
            </a>
          )}
          <DialogClose render={<Button size="sm">{t('dossier.close')}</Button>} />
        </DialogActions>
      </DialogPopup>
    </Dialog>
  )
}

/**
 * How much of a description shows before the rest is asked for, in
 * characters: enough to say what the label is. Discogs descriptions run from
 * one line to pages of catalogue-number notes, and the dossier's argument —
 * the map, the roster, the years — must not start a screen further down.
 */
const LEAD = 300

/**
 * The description, with what the canon can follow made followable: an artist
 * it links to a star opens that star, a label with a dossier opens it, and a
 * web address leaves for the web. Opened to its first paragraphs, as a card
 * opens a Wikipedia lead.
 */
function Profile({ paragraphs, onOpenStar, onOpenLabel }: { paragraphs: Segment[][]; onOpenStar: (id: number) => void; onOpenLabel: (id: number) => void }) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)
  const lead = leadLength(paragraphs)
  const shown = expanded ? paragraphs : paragraphs.slice(0, lead)
  const rest = paragraphs.length - lead

  return (
    <div className="flex flex-col gap-1.5">
      {/* Paragraphs and their runs never reorder, so a place is an identity. */}
      {shown.map((paragraph, index) => (
        <p key={index} className="m-0 whitespace-pre-line text-xs leading-relaxed text-text">
          {paragraph.map((segment, key) => {
            if (segment.star !== undefined) {
              const id = segment.star
              return <Inline key={key} text={segment.text} onClick={() => onOpenStar(id)} />
            }
            if (segment.label !== undefined) {
              const id = segment.label
              return <Inline key={key} text={segment.text} onClick={() => onOpenLabel(id)} />
            }
            if (segment.url !== undefined) {
              return (
                <a key={key} className="text-accent underline-offset-2 hover:underline" href={segment.url} target="_blank" rel="noreferrer">
                  {segment.text}
                </a>
              )
            }
            return <span key={key}>{segment.text}</span>
          })}
        </p>
      ))}
      {rest > 0 && (
        <Button size="sm" className="self-start" onClick={() => setExpanded(!expanded)}>
          {expanded ? t('card.less') : t('card.more', { count: rest })}
        </Button>
      )}
    </div>
  )
}

/** How many leading paragraphs make the lead: at least one, then up to `LEAD` characters. */
function leadLength(paragraphs: Segment[][]): number {
  let length = 0
  for (const [index, paragraph] of paragraphs.entries()) {
    length += paragraph.reduce((sum, segment) => sum + segment.text.length, 0)
    if (length >= LEAD) return index + 1
  }
  return paragraphs.length
}

/** A name inside running text that opens something: a link in look, a button in behaviour. */
function Inline({ text, onClick }: { text: string; onClick: () => void }) {
  return (
    <button type="button" className="cursor-pointer text-accent underline-offset-2 hover:underline" onClick={onClick}>
      {text}
    </button>
  )
}

/** The small map's side, in CSS pixels. */
const MAP_SIZE = 208

/** How far from a member a click on the map still means it, in pixels. */
const MAP_REACH = 8

/** The roster's colour on the map: the second hue of the spectrograph, which stands off the blue sky. */
const ROSTER_FILL = '#e0a040'

/**
 * Where the roster is: the whole sky small, every placed member marked on it.
 *
 * This is the dossier's argument, not an illustration — a label's roster or a
 * city's people either gather in one part of the sky or scatter over it, and
 * which one it is says whether the station is a sound. A click on a member
 * flies to it; the list below is the same people for a keyboard.
 */
function RosterMap({ overview, members, rows, onOpenStar }: { overview: Overview | null; members: Member[]; rows: Row[]; onOpenStar: (id: number) => void }) {
  const { t } = useTranslation()
  const names = useMemo(() => new Map(rows.map(row => [row.id, row.name])), [rows])
  const [hovered, setHovered] = useState<number | null>(null)

  if (!overview) {
    return <div className="rounded-inner bg-[#07080d]" style={{ width: MAP_SIZE, height: MAP_SIZE }} aria-hidden="true" />
  }
  return (
    <figure className="m-0 flex flex-col gap-1">
      <MapCanvas
        overview={overview}
        members={members}
        onHover={setHovered}
        onPick={onOpenStar}
        label={t('dossier.mapLabel', { count: members.length })}
      />
      <figcaption className="h-4 max-w-[13rem] truncate text-2xs text-dim" aria-live="polite">
        {hovered !== null ? (names.get(hovered) ?? t('dossier.mapStar')) : t('dossier.mapHint')}
      </figcaption>
    </figure>
  )
}

function MapCanvas({
  overview,
  members,
  onHover,
  onPick,
  label,
}: {
  overview: Overview
  members: Member[]
  onHover: (id: number | null) => void
  onPick: (id: number) => void
  label: string
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const field = useSkyField(overview, MAP_SIZE)
  const { sky } = overview

  useEffect(() => {
    const canvas = canvasRef.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    const ratio = window.devicePixelRatio || 1
    canvas.width = canvas.height = Math.round(MAP_SIZE * ratio)
    context.setTransform(1, 0, 0, 1, 0, 0)
    context.clearRect(0, 0, canvas.width, canvas.height)
    // The sky dimmed, so the roster is what reads.
    context.globalAlpha = 0.55
    context.drawImage(field, 0, 0)
    context.globalAlpha = 1
    context.scale(ratio, ratio)
    // Faintest first, so a bright member is never painted over.
    const ordered = [...members].sort((a, b) => a[3] - b[3])
    context.fillStyle = ROSTER_FILL
    for (const [, x, y, brightness] of ordered) {
      const at = toMap(sky, MAP_SIZE, x, y)
      context.globalAlpha = 0.55 + 0.45 * brightness
      context.beginPath()
      context.arc(at.x, at.y, 1.2 + 2.3 * brightness, 0, Math.PI * 2)
      context.fill()
    }
  }, [field, sky, members])

  const pointAt = (event: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    return memberAt(members, sky, MAP_SIZE, { x: event.clientX - rect.left, y: event.clientY - rect.top }, MAP_REACH)
  }

  return (
    <canvas
      ref={canvasRef}
      style={{ width: MAP_SIZE, height: MAP_SIZE }}
      className="block cursor-crosshair rounded-inner bg-[#07080d]"
      role="img"
      aria-label={label}
      onPointerMove={event => onHover(pointAt(event))}
      onPointerLeave={() => onHover(null)}
      onClick={event => {
        const id = pointAt(event)
        if (id !== null) onPick(id)
      }}
    />
  )
}

/** The chart's height, in CSS pixels. */
const CHART_HEIGHT = 72

/** How many rows show before the rest are asked for. */
const FIRST_ROWS = 15

/**
 * When, and who: the station year by year, then its people, each with the
 * years they span.
 *
 * One grid for both, and that is the point of it: the chart and every span
 * bar sit in the same column, so a bar lies under the years of the history it
 * is part of. Drawn as two things side by side, they had two widths and two
 * scales, and a span read a decade away from the bars above it.
 */
function History({
  counts,
  rows,
  size,
  heading,
  onOpenStar,
  number,
}: {
  counts: YearCount[]
  rows: Row[]
  size: number
  heading: string
  onOpenStar: (id: number) => void
  number: (value: number) => string
}) {
  const { t } = useTranslation()
  const [all, setAll] = useState(false)
  const domain = yearDomain(
    counts,
    rows.map(row => row.span)
  )
  const top = peak(counts)
  const shown = all ? rows : rows.slice(0, FIRST_ROWS)
  if (rows.length === 0 && (!domain || counts.length === 0)) return null

  return (
    <section className="grid grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-0.5 text-xs">
      {domain && counts.length > 0 && (
        <>
          <SectionLabel className="col-span-3">{heading}</SectionLabel>
          {/* Said in words for a reader, drawn for the eye. */}
          <p className="m-0 self-start text-2xs text-dim">{top && t('dossier.peak', { year: top.year, number: number(top.count) })}</p>
          <div className="flex flex-col gap-1">
            <Bars counts={counts} domain={domain} highest={top?.count ?? 1} busiest={top?.year ?? null} />
            <Axis domain={domain} />
          </div>
          <span />
        </>
      )}

      {rows.length > 0 && (
        <>
          <SectionLabel className="col-span-3 mt-2">
            {size > rows.length ? t('dossier.rosterFirst', { count: rows.length, number: number(size) }) : t('dossier.roster')}
          </SectionLabel>
          {shown.map(row => (
            <Fragment key={row.id}>
              <button
                type="button"
                title={row.note}
                className="cursor-pointer truncate text-left text-accent underline-offset-2 hover:underline"
                onClick={() => onOpenStar(row.id)}
              >
                {row.name}
              </button>
              <SpanBar domain={domain} row={row} />
              <span className="text-right text-2xs text-faint">{row.aside}</span>
            </Fragment>
          ))}
          {rows.length > FIRST_ROWS && (
            <Button size="sm" className="col-span-3 mt-1 justify-self-start" onClick={() => setAll(!all)}>
              {all ? t('dossier.fewer') : t('dossier.all', { count: rows.length })}
            </Button>
          )}
        </>
      )}
    </section>
  )
}

/** The year-by-year bars, stretched to their column. */
function Bars({ counts, domain, highest, busiest }: { counts: YearCount[]; domain: [number, number]; highest: number; busiest: number | null }) {
  const width = 1000
  const slot = yearX(domain, domain[0] + 1, width)
  return (
    <svg viewBox={`0 0 ${String(width)} ${String(CHART_HEIGHT)}`} className="block w-full" preserveAspectRatio="none" aria-hidden="true" style={{ height: CHART_HEIGHT }}>
      {counts.map(entry => {
        const h = Math.max(1, (entry.count / highest) * (CHART_HEIGHT - 1))
        return (
          <rect
            key={entry.year}
            x={yearX(domain, entry.year, width)}
            y={CHART_HEIGHT - 1 - h}
            width={Math.max(1, slot * 0.8)}
            height={h}
            className={entry.year === busiest ? 'fill-accent' : 'fill-accent/55'}
          />
        )
      })}
      <line x1={0} x2={width} y1={CHART_HEIGHT - 0.5} y2={CHART_HEIGHT - 0.5} className="stroke-line" />
    </svg>
  )
}

/** The years under a chart, as text a screen can size: an SVG's text would stretch with it. */
function Axis({ domain }: { domain: [number, number] }) {
  return (
    <div className="relative h-4 text-2xs text-faint" aria-hidden="true">
      {yearTicks(domain).map(year => (
        <span key={year} className="absolute -translate-x-1/2 font-mono" style={{ left: `${String((yearX(domain, year, 1000) / 1000) * 100)}%` }}>
          {year}
        </span>
      ))}
    </div>
  )
}

/** One member's years, as a bar on the shared axis and as words for a reader. */
function SpanBar({ domain, row }: { domain: [number, number] | null; row: Row }) {
  const { t } = useTranslation()
  const { first, last } = row.span
  const said = spanWords(row.span, row.open, t)
  if (!domain || first === null) {
    return <span className="text-2xs text-faint">{said}</span>
  }
  const end = last ?? (row.open ? domain[1] : first)
  const left = (yearX(domain, first, 1000) / 1000) * 100
  const right = (yearX(domain, end + 1, 1000) / 1000) * 100
  return (
    <span className="relative h-3" title={said}>
      <span className="sr-only">{said}</span>
      <span
        aria-hidden="true"
        className={`absolute top-1 h-1 rounded-full ${row.open && last === null ? 'bg-accent/40' : 'bg-accent/80'}`}
        style={{ left: `${String(left)}%`, width: `${String(Math.max(0.6, right - left))}%` }}
      />
    </span>
  )
}

/** A span in words: "1962–1971", "since 1962", or one year. */
function spanWords({ first, last }: Span, open: boolean, t: ReturnType<typeof useTranslation>['t']): string {
  if (first === null) return ''
  if (last === null || last === first) return open ? t('dossier.since', { year: first }) : String(first)
  return t('dossier.years', { first, last })
}

function Pointers({ heading, children }: { heading: string; children: ReactNode }) {
  return (
    <>
      <SectionLabel>{heading}</SectionLabel>
      <ul className="m-0 flex list-none flex-wrap gap-x-3 gap-y-1 p-0 text-xs">{children}</ul>
    </>
  )
}

function Pointer({ name, count, onClick }: { name: string; count?: string; onClick: () => void }) {
  return (
    <li>
      <button type="button" className="cursor-pointer text-accent underline-offset-2 hover:underline" onClick={onClick}>
        {name}
      </button>
      {count && <span className="ml-1 text-2xs text-faint">{count}</span>}
    </li>
  )
}

function LabelPointers({ heading, labels, onOpen, number }: { heading: string; labels: LabelRef[]; onOpen: (id: number) => void; number: (value: number) => string }) {
  if (labels.length === 0) return null
  return (
    <Pointers heading={heading}>
      {labels.map(label => (
        <Pointer key={label.id} name={label.name} count={label.artists > 0 ? number(label.artists) : undefined} onClick={() => onOpen(label.id)} />
      ))}
    </Pointers>
  )
}

function ScenePointers({ heading, scenes, onOpen, number }: { heading: string; scenes: SceneRef[]; onOpen: (qid: number) => void; number: (value: number) => string }) {
  if (scenes.length === 0) return null
  return (
    <Pointers heading={heading}>
      {scenes.map(scene => (
        <Pointer key={scene.qid} name={scene.name} count={number(scene.artists)} onClick={() => onOpen(scene.qid)} />
      ))}
    </Pointers>
  )
}
