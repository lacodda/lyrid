---
title: Importing facts and influence
description: Add biography and influence links to the canon from the Wikidata dump, without storing 100 GB.
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

## The 100 GB is read, not stored

The full dump is 102.7 GB compressed, and there is no smaller music-only
edition. It never touches your disk: the importer decompresses straight from
the HTTP response and keeps only what it extracts.

```sh
lyrid import wikidata
```

That is the whole command — the official dump URL is the default. Measured
against the live dump, the importer reads about 700 entities a second, which
puts a full pass at roughly **ten hours** — so run it as a background job:

```
INFO lyrid::import::wikidata: streaming the Wikidata dump; nothing is written to disk url=…
INFO lyrid::import::wikidata: still reading entities=1000000 matched=… labels=…
INFO lyrid::import::wikidata: dump read; writing to PostgreSQL entities=… with_mbid=… matched=…
INFO lyrid::import::wikidata: influence edges written rows=… dropped_outside_canon=…
```

To try the pipeline against the real dump without spending the night on it,
stop early:

```sh
lyrid import wikidata --limit 400000
```

A downloaded copy works too, if you would rather keep one:

```sh
lyrid import wikidata --dump ./latest-all.json.bz2
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

### Facts are references, and the names come free

A Wikidata fact points at an item, not a word: place of birth is `Q24826`, not
"Liverpool". Resolving those afterwards would mean a second ten-hour pass, so
every English label is captured during the same read and the referenced ones
are written to `wikidata_item`. A label may be `NULL` — the item exists and has
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
