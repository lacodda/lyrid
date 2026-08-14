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
| `area`, `area_code` | `text` | Country or region, flattened from `area` |
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
| `year` | `smallint` | Reserved for the first release date, which arrives with the release tables |

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

## Replacing the canon

An importer truncates the tables it owns and writes the new contents in one
transaction. That makes re-import idempotent — the same dump yields the same
tables — and means an interrupted run leaves the previous universe standing.
