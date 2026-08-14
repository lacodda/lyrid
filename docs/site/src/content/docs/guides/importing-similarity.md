---
title: Importing similarity
description: Build the artist similarity graph from the ListenBrainz relations dataset.
---

Stars alone are a list. What makes the sky a map is similarity: who is
listened to alongside whom. This guide imports that graph.

Run the [MusicBrainz import](/lyrid/guides/importing-musicbrainz/) first — the
dataset is keyed by MusicBrainz artist credits, and resolving it needs the
canon already in place.

## Get the dataset

```sh
base=https://data.metabrainz.org/pub/musicbrainz/listenbrainz/labs/artist-credit-artist-credit-relations
dir=artist-credit-artist-credit-relations-02-20200813-178494
curl -O "$base/$dir/$dir.tar.bz2"
```

About 117 MB, roughly 7 million edges, CC0. Each line is one relationship:

```json
{"id_0":1021,"name_0":"Ludwig van Beethoven","id_1":11285,"name_1":"Wolfgang Amadeus Mozart","score":1.0}
```

## Run the import

```sh
lyrid import listenbrainz --dump ./artist-credit-artist-credit-relations-02-20200813-178494.tar.bz2
```

The whole dataset is read in one pass — about 7 million lines — and written in
one transaction, so an interrupted run leaves the previous graph standing.

The import reports what it dropped rather than leaving the gaps invisible:

```
INFO lyrid::import::listenbrainz: resolving similarity against the canon credits=…
INFO lyrid::import::listenbrainz: dataset read; writing to PostgreSQL version=2020-08-13T12:41:07.693781+00:00 edges=… skipped_unknown=… skipped_weak=0 skipped_self=…
INFO lyrid::import::listenbrainz: similarity edges written rows=…
INFO lyrid::import::listenbrainz: artist prominence computed rows=…
```

- **`skipped_unknown`** — credits absent from your canon. A newer MusicBrainz
  dump than the similarity dataset will always produce some.
- **`skipped_self`** — both credits resolved to the same artist ("Nirvana" and
  "Nirvana feat. …"). An edge from a star to itself is not a route anywhere.
- **`skipped_weak`** — below `--min-score`, if you passed one.

## Trimming the long tail

Most edges in the dataset are very faint — in a sample of half a million, only
14 scored above 0.5. Faint edges are not noise exactly, but they dominate the
count without carrying much meaning:

```sh
lyrid import listenbrainz --dump ./relations.tar.bz2 --min-score 0.01
```

The threshold is recorded in the metric's description, so a graph can always
say how it was filtered.

## Metrics

Similarity is not one fact but a family of them, so edges are grouped by
metric:

```sql
SELECT key, description FROM similarity_metric;
```

Scores are comparable **within** a metric and never across metrics. Importing
one metric replaces only its own edges, leaving the others standing. This is
what later lets the same music be arranged by a deliberately different notion
of closeness without discarding the first.

## Brightness without listen counts

ListenBrainz publishes no per-artist listen counts as a dump — only a
top-1000 API and a corpus licensed for non-commercial use only. So brightness
is derived from the graph itself, in `artist_prominence`:

| Column | Meaning |
| --- | --- |
| `degree` | How many edges the artist has |
| `weight` | Sum of those edges' scores |

`weight` is the more honest measure: one strong tie counts for more than many
faint ones, which keeps a well-connected obscurity from outshining a genuinely
central artist. The columns are named for what they measure, so nothing here
can be mistaken for play counts.

## Why a dataset from 2020

Current similarity exists at ListenBrainz only behind a live, rate-limited
API. Covering three million artists would mean three million requests —
exactly the failure that
[ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md)
was written to prevent, after a predecessor spent a month of hydration to
collect 1,516 edges.

This dataset yields about four thousand times that, immediately. Musical
similarity also ages slowly: Beethoven and Mozart have not drifted apart since
2020. What the vintage does cost is artists who rose afterwards, who arrive
with no edges at all — which the design already has a place for, as the "dark
matter" every map has at its margins.

Computing a fresh graph from the CC0 listen dumps is its own stage, and the
schema is ready for it: it will land as a second metric beside this one.
