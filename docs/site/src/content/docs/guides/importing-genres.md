---
title: Importing genres and labels
description: Add genres, styles and record labels to the canon from the Discogs monthly dumps.
---

Stars have positions and routes between them. This import gives them a
character: what kind of music each artist makes, and which imprints pressed it.

Run the [MusicBrainz import](/lyrid/guides/importing-musicbrainz/) first.
Artists are joined to their Discogs entries through MusicBrainz's own `discogs`
URL relationship, so without the canon there is nothing to attach genres to.

## Why Discogs and not MusicBrainz

MusicBrainz does carry genres — as folksonomy tags, in
`mbdump-derived.tar.bz2`, which is licensed **CC BY-NC-SA 3.0**. That
restriction travels to everything computed from the data, and genres feed the
sky layout, the tiles and everything drawn from them. The Discogs dumps are
CC0, so the canon stays public domain end to end. The full reasoning is in
[ADR 0005](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md).

## Get the dumps

The dumps live behind an index at [data.discogs.com](https://data.discogs.com/).
Object URLs take a `?download=` parameter — a plain path returns the HTML index
with a `200`, so check what you actually downloaded:

```sh
base='https://data.discogs.com/?download=data%2F2026%2F'
for file in CHECKSUM.txt masters.xml.gz labels.xml.gz artists.xml.gz; do
  curl -sL -o "discogs_20260801_$file" "${base}discogs_20260801_$file"
done
sha256sum -c discogs_20260801_CHECKSUM.txt
```

Three files are needed, about 1.15 GB together:

| File | Size | What it gives |
| --- | --- | --- |
| `masters.xml.gz` | 593 MB | The genres and styles themselves |
| `labels.xml.gz` | 86 MB | Labels, their descriptions and ownership |
| `artists.xml.gz` | 472 MB | Optional: verifies that credited ids exist |

**Do not download `releases.xml.gz`.** It is 10.4 GB and adds nothing: a master
release already carries the genres of all its pressings.

## Run the import

```sh
lyrid import discogs \
  --masters ./discogs_20260801_masters.xml.gz \
  --labels  ./discogs_20260801_labels.xml.gz \
  --artists ./discogs_20260801_artists.xml.gz
```

Each file is read in one streaming pass — nothing is held in memory whole — and
everything is written in one transaction, so an interrupted run leaves the
previous genres standing. On a full 2026-08 dump set the whole import took
about two and a half minutes.

```
INFO lyrid::import::discogs: resolving Discogs data against the canon linked=…
INFO lyrid::import::discogs: artists file read records=10163318 linked_found=…
INFO lyrid::import::discogs: masters file read records=2579897 artists_with_genres=… credits_counted=…
INFO lyrid::import::discogs: labels file read records=2405196 kept=2405195
INFO lyrid::import::discogs: genre vocabulary written rows=…
INFO lyrid::import::discogs: artist genres written rows=…
INFO lyrid::import::discogs: labels written rows=2405195 parents=139304
```

`--labels` and `--artists` are both optional. Without `--labels` the labels
table is left as it was; without `--artists` the import skips checking that the
ids credited on a master actually exist in the artists file.

The version defaults to the date in the masters filename
(`discogs_20260801_masters.xml.gz` → `20260801`). Pass `--dump-version` if your
files are named differently.

## Genres are weighted, not boolean

Discogs attaches genres to releases, never to artists, so an artist's genres
are aggregated over their discography — and the number of releases behind each
one is kept:

```sql
SELECT g.name, g.is_style, ag.releases
FROM artist_genre ag
JOIN genre g ON g.id = ag.genre_id
JOIN artist a ON a.id = ag.artist_id
WHERE a.name = 'Nirvana'
ORDER BY ag.releases DESC
LIMIT 5;
```

```
      name       | is_style | releases
-----------------+----------+----------
 Rock            | f        |      314
 Grunge          | t        |      303
 Alternative Rock| t        |      113
 Acoustic        | t        |       14
 Punk            | t        |       13
```

The weight is what makes this usable. A discography's tail is full of remixes,
interviews and compilation appearances that would each count as much as the
main body if genres were a yes-or-no fact. Threshold on `releases` rather than
treating presence as membership.

`is_style` separates Discogs's two depths: `false` for a genre
("Electronic"), `true` for a style ("Techno").

## How an artist is matched

Through MusicBrainz's `discogs` artist-URL relationship, never by name:

```sql
SELECT a.name, ad.discogs_id
FROM artist_discogs ad
JOIN artist a ON a.id = ad.artist_id
LIMIT 5;
```

Names collide constantly — Discogs disambiguates with numeric suffixes like
"Jack Jones (4)" precisely because they do — and a name-based join would put
one artist's genres on another. URLs under the same relationship kind that
point at label or release pages are ignored.

An artist with no Discogs link gets no genres. That is expected: it is the same
dark matter at the map's margins that missing similarity produces.

## Labels

Labels are the stations of the map, and they nest:

```sql
SELECT l.name, p.name AS parent
FROM label l JOIN label p ON p.id = l.parent_label_id
WHERE l.name = 'Svek';
```

```
 name |     parent
------+----------------
 Svek | Goldhead Music
```

Contact blocks are deliberately not imported. They carry postal addresses and
personal e-mail of small-label owners, and this product has no use for them.

## Re-importing

Importing again replaces genres, links and labels wholesale and updates the
`dump_import` record rather than adding a second one. Verified against the full
dumps: a second run produced identical counts with no duplicates.
