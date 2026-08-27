---
title: Filling a stand
description: Moving a slice of the canon and the tile pyramid onto a deployed stand.
---

A freshly deployed stand answers `/health` and serves the SPA, and the SPA
tells you honestly that the sky has not been built yet. Deploying ships code;
the canon is data, and it arrives separately.

## Why the canon is not built on the stand

The importers read dumps measured in tens and hundreds of gigabytes — a
MusicBrainz export is 7 GB compressed, the Wikidata dump 102.7 GB. Building the
canon takes hours on a desktop, and the result is 4251 MB before it is cut down.
A small machine cannot do that work and does not need to: the canon is built
once where the dumps are, cut to a slice, and the slice is moved.

So the split is:

- **Code** moves on every release, with the deploy.
- **Data** moves when the canon is imported again — a matter of hours of work
  that happens every few stages at most.

Tying the two together would push the whole slice across the network on every
tag to rewrite a database with what it already holds.

## Cutting the slice

On the machine that holds the canon:

```sh
lyrid layout --tiles tiles      # positions and the tile pyramid
lyrid slice --keep 100000       # the brightest 100,000 artists
```

Then dump what is left. Neither this machine nor the stand has a PostgreSQL
client installed — `pg_dump` lives inside the database container:

```sh
docker compose exec -T db pg_dump -U lyrid -d lyrid -Fc -Z6 > .local/lyrid-slice-100k.dump
```

Measured on the real canon: **119 MB compressed**, from an 897 MB database
(4251 MB before `VACUUM FULL` — see
[Deploying a stand](/lyrid/guides/deploying-a-stand/)). The tile pyramid is a
further 6.3 MB.

Keep the dump out of the repository; `.local/` is ignored for this.

## Moving it

```sh
tools/stage-seed.sh
```

The script restores the dump into the stand's database and unpacks the tiles
into the volume the server reads. Both halves go through
`docker compose exec` on each side, because that is where the PostgreSQL client
and the tile volume actually are; the dump is piped over ssh as a stream rather
than staged on the stand's disk first.

| Flag | Effect |
| --- | --- |
| `--stand pi@pi` | ssh destination |
| `--dir /home/pi/lyrid` | where the compose file lives on the stand |
| `--dump <path>` | the dump to restore |
| `--tiles <dir>` | the tile directory to copy |
| `--skip-db` / `--skip-tiles` | move only one half |
| `--force` | replace a stand that already holds artists |

Without `--force` the script refuses to restore over a stand whose `artist`
table is not empty. Today the only thing there is the canon, which can always
be rebuilt; once accounts exist, a silent overwrite would be data loss rather
than a convenience.

The credentials are read from the stand's own `.env` on the stand — they are
never passed on a command line and never copied to the machine holding the
canon.

## Checking it arrived

The script prints what the stand ended up holding — how many artists, how many
of them placed, how many tile files. Two checks confirm the same from outside:

```sh
curl -fsS "http://<stand>:8083/api/search?q=beatles"   # the canon answers
curl -fsSI http://<stand>:8083/tiles/0/0/0.bin         # a tile is there
```

Then open the stand in a browser: the stars should draw, zooming should swap
tile levels, and search should find an artist by name.
