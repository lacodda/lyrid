---
title: Canon schema
description: The tables holding the shared universe, and what each column means.
---

The canon is the half of the database everyone shares: a projection of open
dumps, rebuilt by importers and never edited by users. Per-user data — fog of
war, journals, progress — lives in its own tables and is not touched when the
canon is replaced.

## `dump_import`

One row per import, so "which universe is this" is answerable from the
database rather than from memory.

| Column | Type | Meaning |
| --- | --- | --- |
| `source` | `text` | Which dump: `musicbrainz`, later `listenbrainz`, `discogs`… |
| `version` | `text` | The upstream version string, verbatim |
| `started_at` / `finished_at` | `timestamptz` | A row with no `finished_at` is an import that did not complete |
| `rows_imported` | `bigint` | Total rows written |

`(source, version)` is unique: importing the same export twice updates the
record instead of adding a second one.

## `artist`

Artists are the stars.

| Column | Type | Meaning |
| --- | --- | --- |
| `id` | `integer` | MusicBrainz's own id — the primary key, so relationship tables load without a lookup pass |
| `mbid` | `uuid` | The MBID: stable across merges, and the only id safe to expose or to trace back to musicbrainz.org |
| `name`, `sort_name` | `text` | Display name and the name to sort by |
| `kind` | `text` | `Person`, `Group`, `Orchestra`… flattened from `artist_type` |
| `area` | `text` | Country or region, flattened from `area` |
| `area_code` | `text` | ISO 3166-1 alpha-2, from `iso_3166_1`. `NULL` where the area is not a country — an artist from Glasgow has an area and no code, rather than a guessed one |
| `begin_year`, `end_year` | `smallint` | Formed and disbanded; born and died |
| `ended` | `boolean` | Whether the artist is over |
| `comment` | `text` | MusicBrainz's disambiguation comment, shown wherever two artists share a name |

## `release_group`

Release groups are the planets of an artist's system: an album, not each of
its pressings.

| Column | Type | Meaning |
| --- | --- | --- |
| `id`, `mbid` | `integer`, `uuid` | As above |
| `name` | `text` | Album title |
| `primary_type` | `text` | `Album`, `Single`, `EP`, `Compilation`… |
| `artist_id` | `integer` | The first credited artist. A release group crediting several artists belongs to the system of the first |
| `year` | `smallint` | The year of the earliest release in the group, so a reissue never dates the album. `NULL` when no release carries a date |

## `artist_url`

The addresses an artist card links out to, keyed by `(artist_id, kind, url)`.

| Column | Type | Meaning |
| --- | --- | --- |
| `artist_id` | `integer` | Owner of the link |
| `kind` | `text` | MusicBrainz's link type, verbatim: `youtube`, `official homepage`, `bandcamp`, `wikidata`… |
| `url` | `text` | The address |

This table is why playback needs no YouTube Data API: channel and video ids
come from these relationships, so there is no API key and no quota in the
product at all.

## `artist_credit`

MusicBrainz's credit ids, mapped to the artist credited first. Kept because
other datasets key on credits rather than artists — ListenBrainz's similarity
dump among them — and resolving them happens long after the MusicBrainz import
has finished.

## `similarity_metric`

Similarity is not one fact but a family of them: co-listening from one corpus,
co-listening from another, and later a deliberately different notion of
closeness for the "prestige" lens. Each set of edges names its metric.

| Column | Meaning |
| --- | --- |
| `key` | Stable identifier, e.g. `listenbrainz-2020` |
| `description` | Corpus, method and vintage — what a reader needs to judge the numbers |

Scores are comparable **within** a metric and never across metrics.

## `artist_similarity`

The edges themselves: `(metric_id, source_id, target_id, score)`.

Each unordered pair is stored **once**, with `source_id < target_id` enforced
by a check constraint. The relation is symmetric, and storing both directions
would double a table of millions of rows while inviting the two copies to
disagree. Both columns are indexed, so neighbours can be found from either end.

## `artist_prominence`

How brightly a star is drawn, per metric.

| Column | Meaning |
| --- | --- |
| `degree` | How many edges the artist has |
| `weight` | Sum of those edges' scores |

Derived from the graph rather than from listen counts, because no per-artist
listen counts are published as a dump. See
[Importing similarity](/lyrid/guides/importing-similarity/).

## `genre`

Discogs's vocabulary, at both of its depths.

| Column | Meaning |
| --- | --- |
| `name` | `Electronic`, `Techno`, `Grunge`… |
| `is_style` | `false` for a Discogs genre, `true` for a style |

One table rather than two: a genre and a style are the same kind of fact at
different depths, and separating them would double every query the map makes.
`(name, is_style)` is unique, so "Rock" the genre and "Rock" the style are
distinct rows.

Genres come from Discogs and not from MusicBrainz because MusicBrainz's tag
tables are licensed CC BY-NC-SA, which would travel to the layout, the tiles
and everything drawn from them — see
[ADR 0005](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md).

## `artist_genre`

What kind of music a star makes: `(artist_id, genre_id, releases)`.

Discogs attaches genres to releases, never to artists, so this is an aggregate
over the artist's discography. `releases` is **how many of their releases carry
this genre** — the weight that makes the aggregate honest, since a discography's
tail of remixes and interviews would otherwise count as much as its main body.
Consumers should threshold on `releases` rather than treat presence as
membership.

**An artist's main genre and main style** — the one carried by most of its
releases, ties to the name that sorts first — come from one function,
`main_genres(artists integer[])`. The names on the sky, the compass and the radio
all ask it, so a name on the map and the radio of that name mean the same stars.
It takes the artists to rank rather than being a view over everyone, because
every caller already holds a small set and ranking is cheap once they are chosen
([ADR 0016](https://github.com/lacodda/lyrid/blob/main/docs/adr/0016-listening-to-the-sky.md)).

## `artist_discogs`

Which Discogs artist a canonical artist is: `(artist_id, discogs_id)`.

Derived from MusicBrainz's `discogs` URL relationship and stored so later
pipelines can follow the join without re-parsing URLs. Two canonical artists
may point at one Discogs id, where MusicBrainz splits an act that Discogs keeps
whole, so `discogs_id` is indexed rather than unique.

## `label`

The stations of the map: imprints, and which imprint owns which.

| Column | Meaning |
| --- | --- |
| `id` | Discogs's own label id |
| `name` | Label name, verbatim — with the number Discogs adds to tell namesakes apart ("Antidote (4)"), which the API drops on the way out |
| `profile` | Discogs's description, for the station page, with its references named (below) |
| `parent_label_id` | The owning imprint, when Discogs records one |

Contact information is deliberately not imported: those blocks carry postal
addresses and personal e-mail of small-label owners, which this product has no
use for.

**References in `profile` carry their names.** Discogs writes `[a674]` for an
artist and `[l123]` for a label, and an id alone cannot be shown. The import
picks up the names while every dump file is open and stores `[a674=Stephan
Grieder]` — an id and a name together, this project's own form. Everything else
is Discogs's markup as written: `[a=Name]`, `[l=Name]`, `[url=…]…[/url]`, `[b]`,
`[i]`. Line breaks are stored as `
` only; a blank line separates paragraphs.

A slice keeps a label when it has a roster among the kept artists, or owns one
that does, however far up.

## `label_artist`

A label's roster: who released on it, how much, and when.

| Column | Meaning |
| --- | --- |
| `label_id` | The label |
| `artist_id` | A canonical artist with releases on it |
| `releases` | How many of the artist's official releases carry this label, pressings included |
| `first_year`, `last_year` | The years of the first and the last of them, where Discogs dates them — the artist's span on the label |

From the Discogs releases file, through `artist_discogs`. Unofficial releases
and "Not On Label" are not counted; a label printed twice on one release counts
once. See ADR 0017.

## `label_year`

A label's output year by year: `(label_id, year, releases)`, over **all** its
official releases — not only those by artists the canon holds, because this is
the label's history, not the canon's. Kept for labels with a roster only. A
label's first year, last year and total come from here rather than being stored
twice.

## `artist_wikidata`

Which Wikidata item an artist is: `(artist_id, qid, enwiki_title)`.

The link comes from MusicBrainz's `wikidata` URL relationship and is confirmed
from the other side — the item's `P434` must hold that artist's MBID. Two
independent statements agreeing is what makes this join safe enough to hang a
biography on.

`enwiki_title` is the English Wikipedia article title, kept verbatim
("Nirvana (band)"). It is captured during the same pass because the prose
import needs it and it exists nowhere else in the canon.

## `artist_fact`

One row per artist, holding the handful of biographical fields a card reads
together.

| Column | Meaning |
| --- | --- |
| `origin_qid` | Where the act comes from: location of formation for groups, place of birth for people |
| `origin_is_birth` | **Which** question was answered. "Formed in Seattle" and "born in Seattle" are different claims |
| `inception_year` | From Wikidata. May disagree with `artist.begin_year`; both are kept rather than one silently overwriting the other |
| `country_qid` | Country of origin |

`origin_qid` is also a **scene**: the place a dossier gathers everyone from.
It is indexed for that question, which reads this table by place rather than by
artist.

## `wikidata_item`

Names for the items facts point at: `(qid, label)`.

A Wikidata fact is a reference, not a word — place of birth is `Q24826`, not
"Liverpool". Resolving them afterwards would mean a second pass over 100 GB, so
labels are captured during the same read. `label` can be `NULL` where the item
has no English name, so queries joining this table should use `LEFT JOIN`.

## `artist_influence`

The currents of the map: `(artist_id, influence_id)`, meaning **artist_id was
influenced by influence_id**.

Directed, unlike similarity: the arrow is the whole point of the fact, so the
reverse is not stored. Both ends must be artists in the canon — an influence
pointing at a painter is true and undrawable. A check constraint rejects
self-influence, which Wikidata does contain.

## `artist_wikidata_genre`, `artist_wikidata_label`

Genres and record labels as Wikidata claims them, kept apart from the Discogs
vocabulary in `artist_genre` and `label_artist`. Discogs terms carry a release
count behind them; these are editorial claims with no weight. Mixing two
vocabularies in one table is the mistake `similarity_metric` exists to prevent.
Nothing serves them: the card's labels are the Discogs stations.

## `artist_prose`

The words on an artist card: Wikipedia lead paragraphs under CC BY-SA 4.0.

| Column | Meaning |
| --- | --- |
| `extract` | The lead, as plain text: templates resolved or removed, links flattened, references gone |
| `source_title` | The article title, verbatim ("Nirvana (band)") |
| `source_url` | The article address, which the licence requires be shown |
| `licence` | `CC BY-SA 4.0` |
| `dump_version` | Which dump the words came from |
| `revision_id` | The article revision, so a claim can be dated exactly |
| `source_chars`, `extract_chars` | How much of the original survived parsing |

**The attribution is in the same row as the text, deliberately.** Unlike every
other source in the canon, this one carries an obligation that travels with
each snippet. Storing the credit beside the words rather than attaching it at
render time means no query can select the prose and forget the licence — they
are one row.

`source_chars` and `extract_chars` exist to find leads the parser mangled: a
ratio far from the usual one is the signature of a template it did not know.
See [Importing prose](/lyrid/guides/importing-prose/) and
[ADR 0007](https://github.com/lacodda/lyrid/blob/main/docs/adr/0007-prose-parsed-from-wikitext.md).

## `sky_layout`

One row per layout run: where the stars were put, and by what.

| Column | Meaning |
| --- | --- |
| `key` | Stable name, e.g. `listenbrainz-2020-fd300-s24301` |
| `metric_id` | Which similarity graph was projected — a layout of another metric is another sky |
| `description` | Algorithm and parameters, in enough detail to run it again |
| `seed` | The random seed the run used |
| `stars` | How many were placed, so a truncated run shows as a number rather than a suspicion |

Versioned for the same reason similarity is: a layout is not a fact but a
rendering of one, and recomputing it moves every star. Coordinates are
comparable **within** a layout and never across two.

## `artist_position`

Where each star sits: `(layout_id, artist_id, x, y)`.

Single precision on purpose — the layout is a picture, not a measurement, and
`real` is finer than any zoom level shows. Indexed by `(layout_id, x, y)` so
the tile builder can walk a layout in spatial order instead of sorting
millions of rows per level.

Artists with no similarity edges have no position: there is nothing to place
them near. See [Building the sky](/lyrid/guides/building-the-sky/) and
[ADR 0008](https://github.com/lacodda/lyrid/blob/main/docs/adr/0008-layout-written-here.md).

## Replacing the canon

An importer truncates the tables it owns and writes the new contents in one
transaction. That makes re-import idempotent — the same dump yields the same
tables — and means an interrupted run leaves the previous universe standing.
