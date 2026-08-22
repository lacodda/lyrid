---
title: Deploying a stand
description: Running lyrid on a small machine — the image, the slice, and the tiles.
---

A stand runs the whole product from one process: the server, the built SPA and
the tile pyramid behind a single origin. That is deliberately unlike
development, where Vite serves the front end and proxies the API — and the
difference is most of what a stand is for. Same-origin requests, static files
served by the server, migrations against a database that already has data, and
a different processor architecture are all invisible until something is
actually deployed.

## What the stand needs

- Docker with Compose.
- A `.env` beside `docker-compose.prod.yml`, holding at least `POSTGRES_USER`
  and `POSTGRES_PASSWORD`. Compose refuses to start without them rather than
  falling back to a default credential.
- Enough disk for a slice, not for the canon — see below.

```sh
docker compose -f docker-compose.prod.yml up -d --build
```

The image builds the SPA and the server in one pass and runs as an
unprivileged user. It is built on the stand itself, so the architecture is
whatever the stand actually is; nothing is cross-compiled and no `aarch64`
binary is produced blind on a developer's machine.

## Why a slice, not the canon

A full import measures **4251 MB**, and most of that is data the sky never
reads: URL relationships, release groups and labels alone account for more than
half. Meanwhile only 206,636 artists have a position at all — a star with no
edges has nowhere to be placed.

So a stand carries a slice:

```sh
lyrid slice --keep 100000
```

This keeps the brightest 100,000 artists by graph weight and lets the schema
remove everything hanging off the rest. Measured on the real graph, that
retains **5,462,464 of 5,955,657 similarity edges — 92% of the graph from 3.4%
of the artists**. Connectivity is concentrated enough that the result reads as
the sky, not a thinned copy of it.

Three things worth knowing:

- **Run `lyrid layout` first.** Brightness comes from the layout, so the
  command refuses to run before one exists.
- **It is not quick, and it is not stuck.** Removing 2.86M artists means
  clearing fourteen child tables first — see
  [ADR 0010](https://github.com/lacodda/lyrid/blob/main/docs/adr/0010-a-slice-for-a-small-stand.md)
  for why that order matters. Measured end to end on the real canon it took
  **about an hour** on a desktop, and a small machine will be slower. Each
  table is logged as it finishes, so progress is visible.
- **Space returns only after `VACUUM FULL`.** Postgres marks the deleted rows
  dead but keeps the files, so the database still measures its old size until
  the tables are rewritten. Measured on the real canon: **4251 MB before, 897
  MB after** — on a machine chosen for being small, this is the step that
  actually frees the disk, not an optional tidy-up.

Use `--dry-run` to see what would go without changing anything.

## Tiles

Tiles are data, not part of the image: they are cut from the layout and would
otherwise force a rebuild every time the sky changed. They live in a named
volume mounted at `/app/static/tiles`, and the server serves them directly.

```sh
lyrid layout --tiles /app/static/tiles
```

One detail the volume depends on: the image creates that directory and gives it
to the runtime user. Docker copies ownership onto a fresh named volume from
whatever the image has at the mount point, so an absent directory would leave
the volume owned by root and the layout unable to write a single tile.

## How the server answers

With `LYRID_STATIC` set, two rules hold — see
[Configuration](/lyrid/reference/configuration/) for the detail:

- A missing tile answers `404`, never the SPA. The renderer reads that as "no
  stars here"; an HTML page where it expects a binary header is what broke the
  first zoom in development.
- Any other unknown path answers `200` with the SPA, because a deep link like
  `/star/54` is a client route rather than a missing file.

## Checking it came up

```sh
curl -fsS http://<stand>:8083/health
```

`/health` reports `degraded` with a `503` when the database is unreachable and
`ok` otherwise, so it separates "alive but cannot work" from "not listening".
The container's healthcheck uses the same endpoint.
