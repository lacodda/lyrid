---
title: Importing genres, labels and rosters
description: Add genres, styles, record labels and who released on them to the canon from the Discogs monthly dumps.
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

The dumps live behind an index at [data.discogs.com](https://data.discogs.com/),
and the host is particular: it ignores `Range`, so a cut transfer cannot be
resumed, and it cut plain HTTP/1.1 transfers of the releases file after six to
eight minutes, every time. Over HTTP/2 the same file arrives whole. The
repository carries a downloader that does it that way, one file after another,
and checks each against the dump's own `CHECKSUM.txt`:

```sh
node tools/fetch-discogs.mjs 20260801 .local/discogs
```

Four files, about 12.4 GB together:

| File | Size | What it gives |
| --- | --- | --- |
| `labels.xml.gz` | 86 MB | Labels, their descriptions and ownership |
| `artists.xml.gz` | 472 MB | Verifies credited ids exist; names the artists label descriptions point at |
| `masters.xml.gz` | 593 MB | The genres and styles |
| `releases.xml.gz` | 10.4 GB | Who released on which label, and when: the label rosters |

Run it again if a file fails: files already whole are skipped. Do not loop it —
the host answers a burst of requests with `429` for about an hour, and each
retry extends the wait.

## Run the import

```sh
lyrid import discogs   --masters  .local/discogs/discogs_20260801_masters.xml.gz   --labels   .local/discogs/discogs_20260801_labels.xml.gz   --artists  .local/discogs/discogs_20260801_artists.xml.gz   --releases .local/discogs/discogs_20260801_releases.xml.gz
```

Each file is read in one streaming pass — nothing is held in memory whole but
the counts — and everything is written in one transaction, so an interrupted
run leaves the previous genres, labels and rosters standing.

On the full 20260801 dumps the import took about fifty minutes: twenty-four
reading — twenty-one of them in the releases file — and twenty-seven writing.

```
INFO lyrid::import::discogs: resolving Discogs data against the canon linked=1286568
INFO lyrid::import::discogs: labels file read records=2405196 kept=2405195
INFO lyrid::import::discogs: artists file read records=10163318 linked_found=1271875 named=104750
INFO lyrid::import::discogs: masters file read records=2579897 artists_with_genres=427042 credits_counted=2429559 titles=520
INFO lyrid::import::discogs_releases: releases file read releases=19341287 unofficial=595691 not_accepted=0 without_label=1472106 roster_pairs=3986904 label_years=2510611
INFO lyrid::import::discogs: label descriptions named left_unnamed=1013
INFO lyrid::import::discogs: labels written rows=2405195 parents=139304
INFO lyrid::import::discogs: label rosters written rows=3983681 stations=532737
INFO lyrid::import::discogs: label chronologies written rows=1953114
```

`left_unnamed` counts references in label descriptions that point at an id no
file of the dump holds; they stay as they are, and the page leaves them out.

`--masters` alone imports genres and leaves labels and rosters as they were.
`--labels` needs `--releases` and `--artists` beside it: a label without its
roster is half a station, and a description that points at artists by bare id
names nobody without the artists file. `--releases` needs `--labels`, the
labels its rosters hang from.

The version defaults to the date in the masters filename
(`discogs_20260801_masters.xml.gz` → `20260801`). Pass `--dump-version` if your
files are named differently.

MusicBrainz's special purpose artists — "Various Artists", "[unknown]",
"[traditional]" and the rest of its documented set — are not joined to Discogs.
MusicBrainz links "Various Artists" to Discogs's "Various", and following that
link would give one star the genres and the labels of every compilation there
is.

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

## Labels and their rosters

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

A roster is who released on a label, from the releases file:

```sql
SELECT l.name, la.releases, la.first_year, la.last_year
FROM label_artist la
JOIN label l ON l.id = la.label_id
JOIN artist a ON a.id = la.artist_id
WHERE a.name = 'Nirvana' AND a.comment LIKE '%grunge%'
ORDER BY la.releases DESC
LIMIT 3;
```

```
      name      | releases | first_year | last_year
----------------+----------+------------+-----------
 Geffen Records |     1269 |       1991 |      2025
 DGC            |     1207 |       1991 |      2026
 Sub Pop        |      820 |       1988 |      2026
```

Official releases only — a release whose format says "Unofficial Release" names
a label that never agreed to it — and never "Not On Label", which is Discogs
saying there was none. A label printed twice on one release counts once;
pressings count, the way Discogs's own label pages count them. `label_year`
holds each label's output year by year over all its official releases, not only
the canon's. See [the schema](/lyrid/reference/canon-schema/#label_artist) and
[ADR 0017](https://github.com/lacodda/lyrid/blob/main/docs/adr/0017-stations-and-their-dossiers.md).

Descriptions point at artists and labels by id — `[a674]` — and the import
names them while every file is open, storing `[a674=Stephan Grieder]`.

## Re-importing

Importing again replaces genres, links, labels and rosters wholesale and updates the
`dump_import` record rather than adding a second one. Verified against the full
dumps: a second run produced identical counts with no duplicates.
