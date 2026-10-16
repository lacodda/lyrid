# 22. Living data from the daily dumps

Date: 2026-10-16

## Status

Accepted.

## Context

The canon is rebuilt every few stages, and between rebuilds the sky holds
still. Pulsars, supernovae and the antenna in the plan all need something that
does not: who is being listened to this week, and what came out. ADR 0002 says
that kind of data arrives as dumps and datasets, never as a rate-limited API in
the critical path; ADR 0004 found that ListenBrainz publishes no listen counts
as a dump.

What ListenBrainz does publish, measured on 2026-10-16:

- **A daily incremental dump of every listen it received**, CC0, kept on
  `data.metabrainz.org` for about eighteen days. Twice: as JSON (175 MB a day)
  and as Parquet for its own statistics cluster (~190 MB a day). In the JSON
  copy almost no listen says which artist it is by (965 of 3.76 million); the
  Parquet copy carries ListenBrainz's own match, `artist_credit_mbids`, for
  three live listens in four.
- Of the 3.76 million listens in the dump of 2026-10-15, 1.41 million were
  played that day by 26,000 people; 2.35 million were histories imported by
  660 people. One account sent 192,000 listens of one artist.
- The dumps are cut by when ListenBrainz received a listen, and each begins
  where the last ended -- except when one is not published at all: 3, 4 and
  8 October 2026 have none.
- **A list of releases out around today**, built from MusicBrainz for everyone:
  one request returns some ten thousand releases within thirty days either side,
  with their dates, release groups and credited artists.

## Decision

**Listening comes from the Parquet copy of the daily dumps, read by byte
ranges.** The server reads the tar's headers, then each file's footer, then
only the four columns it counts -- about a third of the file, and nothing on
disk. A server that answers a range request with the whole file is refused
rather than read.

**What is counted is people.** A day is a UTC day by arrival. A listen counts
when it was played at most a day before it arrived and at most an hour after
(every live listen of the measured dump was); its credited artists, if they are
stars of this canon, each gain that listener for that day. The pairs wait in
`listener_seen` until the day is closed -- when a dump ending after it has been
counted -- and are then counted into `artist_day` and deleted. A star one
person heard in a day is not kept. Each day records how much of it the dumps
covered, so a missing day reads as missing.

**Rising** is listeners a day over the last closed week against the four weeks
before, each divided by how much of its days the dumps covered, with three
listeners added to both sides so that two people are not a thousandfold rise.
A star needs five listeners a day and half again as many as before; nothing is
called rising until the week and the month are covered for three and seven days.
The risers are worked out when a day closes, not per request.

**Releases come from the fresh-releases list**, read once a UTC day and kept by
release group. Reading the whole list in one request is reading a dataset, not
asking about an artist, and nothing waits on it. Its tags are not kept --
MusicBrainz tags are CC BY-NC-SA (ADR 0005) -- nor its cover art.

**The server reads both feeds itself**, every half hour, behind an advisory
lock per feed, and records every pass in `feed_run`: the admin page reads that
as the health of the imports, and a dump's successful run is what marks it
counted, once, by a unique index. Dumps are counted in order; a failure stops
the pass so the next one starts from it. `lyrid feed listening` and `lyrid feed
releases` run one pass by hand.

## Consequences

- The weather is keyed by MusicBrainz id, without foreign keys, so a canon
  rebuild or a slice does not erase it. Days are kept thirteen weeks; the full
  dumps are the source for anything older (v0.20).
- A stand holding a slice counts only the stars of its slice.
- The charter says where the weather comes from: everyone's listening on
  ListenBrainz, as it publishes it, and not the listening lyrid keeps for its
  own accounts, which still serves only its owner's sky.
- First run on an empty database reads every dump the server still offers --
  about eighteen days, a few hundred megabytes of ranges -- so rising can be
  shown on the first day.
