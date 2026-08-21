---
title: Getting started
description: Run lyrid locally — the API against a development database, the SPA, and this documentation site.
---

lyrid is a web service: a Rust API over PostgreSQL, a React SPA, and this documentation site. At the foundation stage there is no deployment yet, so "getting started" means running the three pieces on your own machine.

## What you need

- **Rust** — at least the version in `rust-version` in `Cargo.toml`. `rustup update stable` is enough.
- **Node LTS with pnpm** — `corepack enable` provides pnpm.
- **Docker** — only for the development database; nothing else runs in a container.

## The database

The compose file brings up PostgreSQL alone, configured to match the example environment:

```sh
docker compose up -d db
```

Copy the example environment; its `DATABASE_URL` already points at that container:

```sh
cp .env.example .env
```

## The API

```sh
cargo run -- serve
```

`serve` is also what running the binary with no arguments does, which is what a container image or a service unit expects.

The server binds `0.0.0.0:8080` (override with `LYRID_ADDR`), applies any pending migrations on start, and serves `/health`:

```sh
curl http://127.0.0.1:8080/health
```

```json
{ "status": "ok", "version": "0.1.0", "database": "ok" }
```

If the database is unreachable the same endpoint answers `503` with `"status": "degraded"` — the process is alive but cannot do its job, and the two cases are worth telling apart.

## The universe

An empty database is a sky with no stars. Filling it takes five imports, in this order:

```sh
cargo run -- import musicbrainz  --dump ./mbdump.tar.bz2                   # the stars
cargo run -- import listenbrainz --dump ./artist-credit-relations.tar.bz2  # the routes between them
cargo run -- import discogs      --masters ./discogs_masters.xml.gz \
                                 --labels  ./discogs_labels.xml.gz         # what each star is made of
cargo run -- import wikidata                                               # where it came from, and who it followed
cargo run -- import wikipedia --dump ./enwiki-multistream.xml.bz2 \n                              --index ./enwiki-index.txt.bz2               # the words on the card
```

MusicBrainz comes first in every case: every later import resolves against the canon it builds.

The last one streams a 100 GB dump straight from the network without ever storing it — about ten hours, so run it in the background. What it extracts is a few hundred megabytes.

Once the canon is filled, the sky is built from it:

```sh
cargo run --release -- layout --tiles ./tiles   # coordinates, then the tile pyramid
```

See [Importing MusicBrainz](/lyrid/guides/importing-musicbrainz/), [Importing similarity](/lyrid/guides/importing-similarity/), [Importing genres and labels](/lyrid/guides/importing-genres/) [Importing facts and influence](/lyrid/guides/importing-facts/) [Importing prose](/lyrid/guides/importing-prose/) and [Building the sky](/lyrid/guides/building-the-sky/). Everything else runs fine without them; there is simply nothing to look at yet.

:::note[Adding a migration]
Migrations are embedded into the binary at compile time, so a newly added `.sql` file needs a rebuild before it will apply — `cargo build` after adding one, or the server will happily run the old set.
:::

## The sky

```sh
cd web
pnpm install
pnpm dev
```

The map needs tiles. Point the layout at the folder the dev server publishes:

```sh
cargo run --release -- layout --tiles web/public/tiles
```

Then `http://localhost:5173` shows the sky itself — drag to pan, wheel to
zoom, click a star for its card, and search by name. The pyramid is generated
data, not source, so it is not committed; rebuild it whenever the canon
changes.

### The renderer prototype

`web/prototype/sky.html` is the standalone page the renderer was measured in,
kept because re-running it costs twenty seconds and answers "did that change
cost us frames?".

```sh
cd web && pnpm prototype                        # open it
# ?benchmark  runs a fixed sweep over every level and reports frame times
# ?stress=8   draws the scene eight times per frame, to find the real ceiling
```

The numbers it produced are in
[ADR 0009](https://github.com/lacodda/lyrid/blob/main/docs/adr/0009-renderer-measured.md).

Vite serves the SPA on `http://127.0.0.1:5173` and proxies `/health` and `/api` to the API, so the browser only ever talks to one origin and no CORS configuration is needed.

## This site

```sh
cd docs/site
pnpm install
pnpm dev
```

## Checks before a commit

The same gate CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd web && pnpm lint && pnpm build
cd docs/site && pnpm build
```
