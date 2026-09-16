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
| `LYRID_SECURE_COOKIE` | no | `false` | Marks the session cookie `Secure`. Set to `true` when the server is reached over HTTPS. |
| `LYRID_PUBLIC_URL` | no | `http://$LYRID_ADDR` | Where this service is reached from outside. Used for the links in letters and for the preview image. |
| `LYRID_SMTP_URL` | no | — | SMTP server, credentials included. Unset means letters are written to the log instead of sent. |
| `LYRID_MAIL_FROM` | no | `lyrid <no-reply@localhost>` | Sender address for the two letters lyrid sends. |
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

## The session cookie

`LYRID_SECURE_COOKIE` decides one attribute of one cookie, and getting it
wrong fails in opposite directions:

- **Off over HTTPS** — the session cookie travels without `Secure`, so a
  downgrade to plain HTTP would send the token in the clear.
- **On over plain HTTP** — the browser accepts the cookie and then never sends
  it back. Every request is anonymous, sign-in appears to succeed and nothing
  is remembered, and there is no error anywhere to read.

It defaults to off because the stand runs plain HTTP on a home network, where
the second failure is the one that would actually happen. Production over
HTTPS sets it to `true`.

Only the exact word `true` (in any case, with surrounding spaces ignored)
turns it on. A typo, an empty value or `yes` leaves it off rather than being
guessed at, because the failure it would cause is silent.

The rest of the cookie is not configurable: it is always `HttpOnly` (no script
can read the token, so an XSS hole cannot leak it), always `SameSite=Lax` (a
link from elsewhere arrives signed in; a cross-site form post does not carry
the session), and always expires 30 days after it is issued.

## Mail, and the stand that has none

lyrid sends exactly two letters: one confirming an address, one carrying a way
back into an account. Both are plain text and both carry one link.

`LYRID_SMTP_URL` is the whole configuration, credentials included — for example
`smtps://lyrid:password@smtp.example.com:465`. One variable rather than five
that can disagree with each other. A malformed URL stops the server at startup
rather than on the first registration, and the URL itself never appears in the
error: it holds the password.

**Leaving it unset is a supported configuration, not a fallback.** The home
stand has no route to the internet, and a mailer that failed there would make
every registration on it look broken. With no SMTP server the letter is written
to the log — address, subject and body — where whoever runs the stand can read
the link out of it and click it. The server says so once at startup:

```
WARN lyrid: no LYRID_SMTP_URL: confirmation and password-reset letters will be
written to this log instead of sent
```

That also means the log is a place one-time secrets appear, so it is only
appropriate where nobody else reads the log. A public deployment sets the
variable.

## The public URL

`LYRID_PUBLIC_URL` is how the server addresses itself to things that are not
the browser in front of it:

- **Links in letters.** A letter is read in a mail client, where there is no
  page for a relative link to be relative to.
- **The preview image.** A chat or a timeline fetches `og:image` from its own
  servers, so `/social-preview.png` must leave here as an absolute URL or the
  link unfurls into nothing.

It cannot be inferred from `LYRID_ADDR`: a stand binds `0.0.0.0` and is reached
by a name, and behind a reverse proxy the two share nothing at all. It defaults
to `http://` plus the bind address, which is right in development and wrong
everywhere else — so a deployment sets it.

A trailing slash is trimmed, because a doubled slash is a different path to
most routers and a 404 inside a letter cannot be fixed by whoever received it.

## Caching the tiles

When `LYRID_STATIC` is set, the server puts cache headers on what it serves
from it, and the two files under `/tiles` get opposite ones:

- **A tile** (`/tiles/3/2/1.bin`) — `public, max-age=86400`. A tile never
  changes: rebuilding the sky writes a new layout and a whole new pyramid, so
  there is no such thing as an edited tile, only a replaced sky. A day rather
  than `immutable`, because these URLs are reused by the next pyramid.
- **`/tiles/sky.json`** — `no-cache`. It is the one file that says which
  pyramid this is and how deep it goes, so a held copy points a fresh browser
  at levels that no longer exist: the whole sky broken by one cached file, with
  nothing on screen to explain it.

## Local development

Copy `.env.example` to `.env` and edit it. `.env` is in `.gitignore` and must stay there — credentials, tokens, and connection strings never enter the repository, the docs, the tests, or an example.

```sh
cp .env.example .env
```
