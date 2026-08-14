---
title: Importing MusicBrainz
description: Download a MusicBrainz full export and build the canonical universe from it.
---

The universe starts as artists. This guide takes a MusicBrainz full export and
turns it into the `artist`, `release_group` and `artist_url` tables that every
later stage builds on.

Nothing here touches a rate-limited API: the import reads a dump you already
have on disk, so its speed is bounded by your machine and nobody's quota.

## Get the dump

MusicBrainz publishes full exports at
[data.metabrainz.org](https://data.metabrainz.org/pub/musicbrainz/data/fullexport/).
Each export lives in a timestamped directory, and the `LATEST` file names the
current one:

```sh
base=https://data.metabrainz.org/pub/musicbrainz/data/fullexport
version=$(curl -s $base/LATEST)
curl -O "$base/$version/mbdump.tar.bz2"
```

`mbdump.tar.bz2` is the core archive — roughly 7 GB compressed. The other
archives (edit history, statistics, cover art) are not needed: lyrid reads
twelve tables and skips the rest without decompressing them into memory.

:::caution[Be a good citizen]
The dumps are served for free by a non-profit. Download the archive once and
keep it; do not re-fetch it on a schedule.
:::

## Run the import

```sh
lyrid import musicbrainz --dump ./mbdump.tar.bz2
```

The archive is read in a single pass — bzip2 has no random access, so a second
pass would mean decompressing gigabytes again — and everything is written in
one transaction. An interrupted import therefore leaves the previous universe
intact rather than a half-replaced one.

Output looks like this:

```
INFO lyrid::import::musicbrainz: reading the MusicBrainz export dump=./mbdump.tar.bz2 size_mb=7042
INFO lyrid::import::musicbrainz: archive read; writing to PostgreSQL version=20260813-220122 artists=2958586 release_groups=4459412
INFO lyrid::import::musicbrainz: artists written rows=2958586
INFO lyrid::import::musicbrainz: release groups written rows=4402331 dropped=57081
INFO lyrid::import::musicbrainz: import complete version=20260813-220122
```

`dropped` counts release groups whose credited artist is not in the dump;
importing them would leave albums floating outside any system.

## Which version is loaded

The export version is taken from the `TIMESTAMP` file inside the archive and
recorded in the database, so the universe can always say where it came from:

```sql
SELECT source, version, finished_at, rows_imported FROM dump_import;
```

```
   source    |     version     |          finished_at          | rows_imported
-------------+-----------------+-------------------------------+---------------
 musicbrainz | 20260813-220122 | 2026-08-14 12:13:32.296058+00 |       7360917
```

Pass `--dump-version` to override it, for an archive repackaged without its
timestamp.

## Running it again

Re-importing replaces the canon wholesale rather than merging into it: the
same dump imported twice yields exactly the same tables, and a newer dump
simply supersedes the older universe. Nothing user-owned lives in these
tables, so there is no progress to lose.

## What gets imported

| Table | From | Notes |
| --- | --- | --- |
| `artist` | `artist`, `artist_type`, `area` | Type and country are flattened onto the row |
| `release_group` | `release_group`, `artist_credit_name` | Credited to the first artist in the credit |
| `artist_url` | `l_artist_url`, `url`, `link`, `link_type` | Only artist→URL relationships |

The `artist_url` table is what makes YouTube playback possible without the
YouTube Data API: video and channel ids come from MusicBrainz relationships,
so no API key and no quota are involved anywhere in the product.
