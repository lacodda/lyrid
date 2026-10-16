---
title: Watching a stand
description: The two feeds a running server reads every day, how to run them by hand, and the admin page that shows a stand's canon, feeds and accounts.
---

A stand does not only serve the canon it was filled with. While it runs, it reads two feeds from ListenBrainz, and it has a page that says how that is going.

## The feeds

| Feed | What it reads | How often |
| --- | --- | --- |
| **listening** | The daily incremental dumps of every listen ListenBrainz received, the Parquet copy | Every dump not yet counted, in order |
| **releases** | The list of releases out within thirty days of today | Once a UTC day |

The server looks every half hour, starting a minute after it boots. Each pass over a dump or the list is a row of `feed_run`: when it started and finished, how many rows it read and kept, and why it failed if it did. A failure is retried on the next look; the listening feed stops at a failed dump rather than skipping past it, so the days it covers are not closed without it.

A dump is read by byte ranges — its footer and four columns, about a third of the file — and nothing is written to disk. The first run on an empty database reads every dump the server still offers, about eighteen days; on a Raspberry Pi 4 that is the better part of an hour, in the background.

What is kept, and for how long:

- **`artist_day`** — how many people listened to each star of the canon on each day, thirteen weeks of it;
- **`listening_day`** — how much of each day the dumps covered, and how many people were heard;
- **`star_rising`** — this week's risers, worked out whenever a day closes;
- **`new_release`** — every release the list has named, kept by release group.

None of these reference the canon's tables, so cutting a slice or rebuilding the canon leaves them standing. A stand counts only the stars of its own slice.

### Running a feed by hand

```sh
lyrid feed listening     # count every dump not yet counted
lyrid feed releases      # read the list, unless it was read today
```

Both take the same lock the server does, so running one while the server is reading the same feed does nothing and says so. On a stand: `docker compose -f docker-compose.prod.yml exec server lyrid feed listening`.

`LYRID_DUMPS_URL` points the listening feed somewhere other than `data.metabrainz.org`; the releases feed uses `LYRID_LISTENBRAINZ_URL`, like the scrobbling. See [Configuration](/lyrid/reference/configuration/).

## The admin page

`/admin` shows the stand's own state to an admin, and only to one:

- **the canon** — artists and stars, the layout, the edition and whether this database holds a slice of it, every edition, and every import the database has recorded;
- **listening** — whether the feed is working, a strip of the last five weeks with a bar a day as tall as the share of it the dumps covered, the risers, and the latest runs with their errors;
- **releases** — the same health, and how many releases are known;
- **accounts** — counts only: how many, how many confirmed, joined this week and month, linked to ListenBrainz and how those links are reading, letters asked for;
- **what gets used** — the usage counters of the last fourteen days.

Nobody is named on it. The server sends counts, the same aggregates anyone running the stand could reach with SQL, because the [privacy charter](/lyrid/reference/accounts/#the-privacy-charter-export-and-delete) does not stop at an admin.

### Making an admin

There is no route that makes one. An admin is granted from the server's command line, so holding a shell on the stand is what it takes:

```sh
docker compose -f docker-compose.prod.yml exec server lyrid admin grant someone@example.com
docker compose -f docker-compose.prod.yml exec server lyrid admin revoke someone@example.com
```

The account must already exist. Anyone else opening `/admin` is told to sign in, or that the account does not run this stand.
