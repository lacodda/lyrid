---
title: lyrid serve
description: The server - what it binds, what it serves, and what it keeps doing in the background while it runs.
---

```sh
lyrid serve
```

`lyrid serve` runs the HTTP server. It is also what the binary does with no
arguments at all, which is what a container image or a service unit expects.
It takes no options: everything it needs comes from the environment, listed
in [configuration](/lyrid/reference/configuration/).

## On start

1. **The mail transport is built first.** A malformed `LYRID_SMTP_URL` stops
   the server here, in front of whoever deployed it, rather than on the first
   registration. Without one, letters are written to the log instead of sent,
   and the log says so.
2. **The database is connected and migrated.** Pending migrations are applied
   before anything is served.
3. **The address is bound** - `LYRID_ADDR`, `0.0.0.0:8080` by default - and the
   log line `lyrid listening` names it with the version.

## What it serves

- **The API** under `/api` and [`/health`](/lyrid/reference/health/), always.
- **The interface and the sky**, when `LYRID_STATIC` names a directory: the
  built SPA, with `index.html` for any address that is not a file, and the
  [tile pyramid](/lyrid/reference/tile-format/) under `/tiles`. A tile inside
  the pyramid that has no stars answers `204`; an address the pyramid does not
  have answers `404`, never the SPA. In development Vite serves both and
  proxies the API here.

## What it keeps doing

None of these hold up a request, and a failure in one is logged rather than
stopping the server.

- **Reading the playable stars** once at start, so the first radio or signal
  does not wait for them.
- **Carrying people's progress** into the newest
  [edition](/lyrid/concepts/what-changed-in-the-sky/) of the canon, for anyone
  still behind it - a stand filled by hand catches up on its next start. See
  [the fog](/lyrid/concepts/the-fog/#when-the-canon-changes).
- **Reading linked ListenBrainz accounts** for as long as it runs. See
  [scrobbling](/lyrid/concepts/scrobbling/).
- **Reading what ListenBrainz publishes every day**: everyone's listening and
  the releases around today. See [watching a stand](/lyrid/guides/watching-a-stand/).
- **Sending the weekly letters** within an hour of a week's end, to those who
  asked for them.

## Stopping

`Ctrl+C` stops it gracefully: the requests in flight are answered first.
