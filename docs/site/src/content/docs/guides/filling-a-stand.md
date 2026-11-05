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
canon takes hours on a desktop, and the result is 5568 MB before it is cut down.
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
lyrid layout --metric listenbrainz-listeners  # positions
lyrid canon seal                              # the edition, on the whole canon
lyrid slice --keep 100000                     # the 100,000 most listened of the placed stars
lyrid tiles --out tiles         # the pyramid and the names, from the slice
```

The order matters. Tiles cut before the slice carry every star of the full
canon, and a stand holding only the slice draws those extra stars but opens no
card for them — which is how the stand ran until v0.13. Cutting after the slice
means every star on screen is one the stand's database can answer for.

Then dump what is left. Neither this machine nor the stand has a PostgreSQL
client installed — `pg_dump` lives inside the database container:

```sh
docker compose exec -T db pg_dump -U lyrid -d lyrid -Fc -Z6 > .local/lyrid-slice-100k.dump
```

Measured on the real canon, edition 3: **242 MB compressed**, from a 1484 MB
database (5568 MB before `VACUUM FULL` — see
[Deploying a stand](/lyrid/guides/deploying-a-stand/)). The tile pyramid is a
further 2.9 MB. From the layout already sealed, the slice, its vacuum, the tiles
and the dump took eight minutes on a desktop.

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
| `--tables a,b,c` | replace only the rows of these tables, leaving the rest |

Without `--force` the script refuses to restore over a stand whose `artist`
table is not empty. The canon can always be rebuilt; the accounts on a running
stand cannot, and a whole restore replaces them with whatever the dump holds.

## Adding tables to a stand that has accounts

When a release adds canon tables and nothing else changes — v0.15 added the
label rosters — the stand does not need its database replaced. Deploy the
release first, so its migrations create the empty tables, then move only their
rows:

```sh
tools/stage-seed.sh --skip-tiles --tables label,label_artist,label_year
```

The named tables are emptied in one `TRUNCATE` — without `CASCADE`, so a table
outside the list that points at one of them stops the script instead of being
emptied along with it — and their rows restored from the dump in the order the
dump holds them, which loads a table before the ones that point at it. Every
other table, the accounts above all, is left as it was.

The credentials are read from the stand's own `.env` on the stand — they are
never passed on a command line and never copied to the machine holding the
canon.

## Replacing the whole canon under accounts

A new sky — v0.20 laid it out from a different graph — changes every canon
table at once, on a stand that has people on it. Name every table of the canon,
and nothing of the accounts or the weather:

```sh
tools/stage-seed.sh --dump .local/lyrid-slice-100k.dump --tiles .local/tiles \
  --tables artist,artist_credit,artist_discogs,artist_fact,artist_genre,artist_influence,artist_listeners,artist_position,artist_prose,artist_redirect,artist_similarity,artist_spectrum,artist_url,artist_wikidata,artist_wikidata_genre,artist_wikidata_label,canon_change,canon_edition,canon_edition_source,canon_edition_star,dump_import,genre,label,label_artist,label_year,release_group,similarity_metric,sky_layout,spectrum_scale,wikidata_item
```

The accounts, their listening, their letters and the stand's own weather —
`app_user` and what hangs off it, `feed_run`, `listening_day`, `artist_day`,
`star_rising`, `new_release` — stay as they are. Listening and the
fog's openings are keyed by MusicBrainz id, not by the canon's own numbers, so
they find their stars again in the new canon.

One thing outside the canon does point into it: a profile remembers the layout
its camera was on. So the named tables are not simply truncated — Postgres
would refuse, since `user_profile` is not among them. The script truncates what
it can and deletes the rest children first, which lets the schema do what it
says: the camera forgets a layout that is gone, and the next visit opens on the
whole sky. A table outside the list that would lose rows with the canon, or
block it, stops the script before anything is emptied. Measured on a slice of
100,000 artists: one second.

Replacing all thirty tables of the canon under accounts, on a small single-board
stand over a home network, took eleven and a half minutes. For as long as the rows
are loading the old sky stays drawn from its tiles while cards and search find
nothing, because the tables are emptied first. Take a backup before it; `pg_dump`
runs in the same container.

## Carrying people's progress

Whenever the canon's tables are replaced under accounts, `tools/stage-seed.sh` finishes by running `lyrid canon carry-over` in the stand's server: an id MusicBrainz merged becomes the star it merged into, in each person's openings and in the credits of their listens, and the profile records the edition it has reached. It runs once per edition per person, so running it twice changes nothing, and a server does the same whenever it starts, which is what catches a stand filled by hand.

```sh
docker compose -f docker-compose.prod.yml exec server lyrid canon carry-over
```

The redirects live in the canon (`artist_redirect`), so name that table with the rest when replacing it. A redirect goes with the artist it points at, which means a merge whose target was cut from the slice is not followed on that stand: that opening stays under its old id until a canon holding the target arrives. Nothing is lost by this. See [ADR 0026](https://github.com/lacodda/lyrid/blob/main/docs/adr/0026-the-fog-and-progress-across-editions.md).

## Carrying the edition

The [edition](/lyrid/guides/building-the-sky/#sealing-the-edition) is sealed on the full canon, so a slice cut after sealing already carries it. A stand filled before an edition was sealed takes it on its own, as four tables:

```sh
pg_dump -Fc --data-only -t canon_edition -t canon_edition_source \
  -t canon_edition_star -t canon_change > .local/lyrid-edition.dump
tools/stage-seed.sh --skip-tiles --dump .local/lyrid-edition.dump \
  --tables canon_edition,canon_edition_source,canon_edition_star,canon_change
```

All four, because the other three point at `canon_edition` and the script's `TRUNCATE` refuses to empty a table something outside the list still points at. The stand then shows the edition in its status line, and its [admin page](/lyrid/guides/watching-a-stand/#the-admin-page) shows it holds a slice of it.

## Checking it arrived

The script prints what the stand ended up holding — how many artists, how many
of them placed, how many tile files. Two checks confirm the same from outside:

```sh
curl -fsS "http://<stand>:8083/api/search?q=beatles"   # the canon answers
curl -fsSI http://<stand>:8083/tiles/0/0/0.bin         # a tile is there
```

Then open the stand in a browser: the stars should draw, zooming should swap
tile levels, and search should find an artist by name.
