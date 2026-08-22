---
title: Configuration
description: Environment variables the server reads, their defaults, and what happens when they are wrong.
---

lyrid is configured entirely through the environment. There is no configuration file, and secrets never live in one.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `DATABASE_URL` | yes | — | PostgreSQL connection string, e.g. `postgres://lyrid:lyrid@localhost:5432/lyrid` |
| `LYRID_ADDR` | no | `0.0.0.0:8080` | Socket address the HTTP server binds to |
| `LYRID_STATIC` | no | — | Directory holding the built SPA and the tile pyramid. Unset means API only. |
| `RUST_LOG` | no | `lyrid=info,tower_http=info` | Log filter, in `tracing-subscriber` `EnvFilter` syntax |

## How it is read

The server reads the environment once at startup and fails immediately if it cannot build a valid configuration:

- **`DATABASE_URL` missing** — startup aborts with a message naming the variable and showing the expected shape.
- **`LYRID_ADDR` malformed** — startup aborts naming the variable and echoing the value it could not parse.

Failing at startup is deliberate. A server that boots with a broken configuration and only discovers it on the first request has turned a deployment error into an outage.

Note that a database that is *unreachable* is a different case from one that is *not configured*: the first is reported by [`/health`](/lyrid/guides/running-a-development-database/) as `degraded` while the server keeps running, the second stops it from starting at all.

## Serving the SPA and the tiles

`LYRID_STATIC` is what separates development from a deployed stand, and the
two arrangements differ enough to be worth stating.

In development it stays unset: Vite serves the SPA and the tiles and proxies
`/api` here, so this process answers nothing but the API. On a stand it points
at the directory holding `index.html` and a `tiles/` subdirectory, and this
process is the only thing listening.

Two rules hold when it is set:

- **`/tiles/**` never falls back to the SPA.** A missing tile answers `404`, so
  the renderer can read it as "no stars here". Handing it `index.html` instead
  would give it an HTML page where it expects a binary header — which is
  exactly what a dev server does, and exactly what broke the first zoom.
- **Every other unknown path answers `200` with the SPA.** A deep link like
  `/star/54` is a client-side route, not a missing file. The status matters
  beyond the browser: a `404` carrying a working page still tells crawlers and
  uptime monitors that the site is broken.

## Local development

Copy `.env.example` to `.env` and edit it. `.env` is in `.gitignore` and must stay there — credentials, tokens, and connection strings never enter the repository, the docs, the tests, or an example.

```sh
cp .env.example .env
```
