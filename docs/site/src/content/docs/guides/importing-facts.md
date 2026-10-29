---
title: Importing facts and influence
description: Add biography and influence links to the canon from the Wikidata dump.
---

Stars have positions, routes and a sound. This import gives them a history:
where the act formed, when, on whose label — and who influenced whom.

Run the [MusicBrainz import](/lyrid/guides/importing-musicbrainz/) first.
Entities are matched to the canon by their MusicBrainz id, so without artists
there is nothing to attach a biography to.

## Why Wikidata

Two of these facts exist nowhere else. MusicBrainz has **no influence
relationship at all** — its artist-to-artist vocabulary covers membership,
collaboration and family, not lineage. And where MusicBrainz records a country,
Wikidata records a city: "formed in Aberdeen" rather than "United States".

Wikidata is CC0, so it raises none of the licensing questions that
[ADR 0005](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md)
had to settle for genres.

## Download the dump first

The full dump is about 104 GB compressed (103.6 GB for `20261026`), and there
is no smaller music-only edition. The importer can read it straight from the
network without ever writing it down — but for a full pass, download a
**dated** file first:

```sh
run=20261026
curl -fL -C - -O "https://dumps.wikimedia.org/wikidatawiki/entities/$run/wikidata-$run-all.json.bz2"

lyrid import wikidata --dump ./wikidata-$run-all.json.bz2 --dump-version $run
```

A dated file rather than `latest-all.json.bz2`: "latest" moves every week, and
resuming a download across a move splices two dumps into one file. The run's
directory publishes an md5 for each file; check it before importing.

**Why not stream the whole thing.** A full pass takes hours, and a connection
held open that long does not survive: measured on the real dump, the server
dropped the connection after 24 minutes and three million entities, and the
pass died with `Peer disconnected` — everything read up to that point thrown
away. `curl -C -` resumes from what is already on disk, so an interruption costs
seconds instead of the whole run. The cost is the dump's size on disk,
temporarily — and the download itself: dumps.wikimedia.org gives a connection a
few megabytes a second and limits how many one address may open, so expect
several hours. The mirrors listed on dumps.wikimedia.org carry the same files.

### How long the import takes

The dump is read twice, facts first and then the labels they point at (see
below), and each pass runs at the speed bzip2 decompresses on all your cores
([ADR 0025](https://github.com/lacodda/lyrid/blob/main/docs/adr/0025-bzip2-on-every-core.md)):
lines without a MusicBrainz id are let go unparsed, so parsing never holds the
reading up. Measured on a 16-thread laptop on the first three million entities
of `20261026` — 37.5 GB of JSON:

| Pass | One core (v0.20) | Every core |
| --- | --- | --- |
| Facts | 834 s | 232 s |
| Labels | 821 s | 252 s |

Both versions wrote the same rows. Run the full import as a background job:

```
INFO lyrid::import::wikidata: reading the Wikidata dump dump=./wikidata-20261026-all.json.bz2 size_gb=96
INFO lyrid::import::wikidata: still reading facts entities=1000000 matched=…
INFO lyrid::import::wikidata: facts read entities=… matched=…
INFO lyrid::import::wikidata: reading labels for the items the facts point at wanted=…
INFO lyrid::import::wikidata: labels read labels=… wanted=…
INFO lyrid::import::wikidata: dump read; writing to PostgreSQL entities=… with_mbid=… matched=…
INFO lyrid::import::wikidata: influence edges written rows=… dropped_outside_canon=…
```

Streaming still has its place for a short run against the live dump, where the
window is small enough that a drop is unlikely. The official dump URL is the
default, and nothing is written to disk:

```sh
lyrid import wikidata --limit 400000
```

```
INFO lyrid::import::wikidata: streaming the Wikidata dump; nothing is written to disk url=…
```

### Why not the smaller dump

The `latest-truthy.nt.bz2` edition is 43.3 GB — less than half — and it does
carry all eight properties this import reads. It was checked and rejected for
one reason: it has **no Wikipedia sitelinks**. Those article titles are the
only bridge to the prose import, and finding articles by artist name instead
would reintroduce name matching, which the genre import already refused for
good reason.

## Having a MusicBrainz id is not being a musician

The first entity with a `P434` in the dump is a 17th-century painter — there is
music written about him, so he has an id. The filter that matters is your own
canon: only entities whose MBID is already an artist here are kept. The log
reports both numbers, so the gap is visible rather than implied:

```
entities=… with_mbid=… matched=…
```

## What lands in the database

```sql
SELECT a.name, origin.label AS formed_in, f.inception_year, country.label
FROM artist a
JOIN artist_fact f ON f.artist_id = a.id
LEFT JOIN wikidata_item origin ON origin.qid = f.origin_qid
LEFT JOIN wikidata_item country ON country.qid = f.country_qid
ORDER BY a.name;
```

```
     name     | formed_in | inception_year |     label
--------------+-----------+----------------+----------------
 Led Zeppelin | London    |           1968 | England
 Nirvana      | Aberdeen  |           1987 | United States
 Pixies       | Boston    |           1986 | United States
 The Beatles  | Liverpool |           1960 | United Kingdom
```

`origin_is_birth` records **which** question was answered: a group's location
of formation (`P740`) when there is one, a person's place of birth (`P19`)
otherwise. "Formed in Seattle" and "born in Seattle" are different claims and
the column keeps them apart.

`inception_year` can disagree with `artist.begin_year` from MusicBrainz. Both
are kept deliberately — silently overwriting a curated value with a
crowdsourced one would be worse than showing the disagreement.

### Facts are references, and their names take a second read

A Wikidata fact points at an item, not a word: place of birth is `Q24826`, not
"Liverpool". Keeping every English label while reading would name them in one
pass, but 123 million labels do not fit in memory — measured at 3.1 GB a
quarter of the way through. So the dump is read twice: the first pass collects
the facts and the items they point at, tens of thousands of them; the second
reads the labels of exactly those, stopping as soon as all are found, and
writes them to `wikidata_item`. A label may be `NULL` — the item exists and has
no English name — which is why the queries above use `LEFT JOIN`.

## Influence

```sql
SELECT a.name AS artist, i.name AS influenced_by
FROM artist_influence e
JOIN artist a ON a.id = e.artist_id
JOIN artist i ON i.id = e.influence_id
ORDER BY a.name;
```

```
 artist  | influenced_by
---------+---------------
 Nirvana | Led Zeppelin
 Nirvana | Pixies
 Pixies  | The Beatles
```

Unlike similarity, influence is **directed**: "X was influenced by Y" says
nothing about the other direction, and the arrow is the whole point. Both ends
must be artists in the canon — an influence pointing at a painter or a novelist
is true and undrawable, so it is dropped and counted as
`dropped_outside_canon` rather than stored as a dangling id.

Expect the graph to be thin. An edge needs both ends in your canon *and* a
Wikidata editor who recorded the claim; this is a garnish on the map, not a
second similarity graph.

## Genres, kept apart

Wikidata genres land in `artist_wikidata_genre`, separate from the Discogs
genres in `artist_genre`. They are not the same kind of fact: Discogs terms
carry a release count behind them, Wikidata genres are editorial claims with no
weight. Mixing two vocabularies in one table is the mistake `similarity_metric`
exists to prevent.

## Re-importing

Importing again replaces every table this source owns and updates the
`dump_import` record rather than adding a second one. Verified: a second run
over the same input produces identical counts with no duplicates.
