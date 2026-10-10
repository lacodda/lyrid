# 19. Real listening, read from ListenBrainz into a ledger

Date: 2026-10-10

## Status

Accepted.

## Context

Since the founding, the Vision has named three kinds of listening: a preview
is a scan, an embedded video is a landing, and scrobbles from ListenBrainz are
the real journey -- the one that pays in light. v0.17 builds the third. A person
links their ListenBrainz account; what they listen to anywhere, from then on,
opens the stars it names and gathers light, and something new pays far more
than something already familiar (bank 35).

This is the first personal data lyrid did not get from the person typing it.
It also brings the first outbound call to a third-party API on behalf of a
user, which [ADR 0002](0002-universe-from-open-dumps.md) keeps out of the
critical path.

What ListenBrainz offers, measured against the live service on 2026-10-10
rather than read from its documentation:

- Every user's listens are public and read by name at
  `/1/user/{name}/listens`, without a token.
- A user token proves who someone is: `/1/validate-token` answers with the
  name it belongs to. The same token also lets its holder *submit* listens.
- Asked for listens after `min_ts`, the service returns the **oldest** `count`
  of them, newest first within the page. With `max_ts` as well, the same.
- Asked with `min_ts` alone, it scans forward until the page is full or it
  reaches the present. For someone who has not listened for a while, the last
  page scans months of nothing and is cut off by the service's edge at about
  forty seconds -- even for a page of ten. The same request bounded by
  `max_ts` to a week answered in eight seconds.
- A listen is matched to MusicBrainz after it arrives. The mapping may name
  the recording and no artists, while `additional_info.artist_mbids`, written
  by the player, does. A third of a sample of 225 listens credited more than
  one artist.
- Players that lose their connection submit listens later, carrying the time
  they were played.

## Decision

**Link by proof, read by name, keep a ledger.**

- **Linking.** The person pastes their ListenBrainz token. The server asks
  ListenBrainz whose it is, stores the name, and drops the token: it is never
  written to the database or the log. A stored token would be the power to
  submit listens in someone's name, which lyrid has no use for. One
  ListenBrainz account feeds one lyrid account (a unique index on the
  case-folded name), or one person's music would pay twice.
- **The ledger.** Each listen is a row keyed by ListenBrainz's own identity of
  a listen -- `(user, listened_at, recording_msid)` -- holding the credited
  artists' MBIDs, the length when it was reported, and the light it paid. No
  track or album names. Paying a listen twice is a key violation rather than
  something a watermark has to prevent, so the reading window can overlap
  itself freely. Openings are not stored: a star is open from the first listen
  that names it, and the ledger answers that.
- **Reading.** In the background every fifteen minutes, and at once when the
  person asks (at most once a minute). Each read covers from two days before
  the last one -- for late submissions -- and never before the link. It goes
  in windows of three days, each bounded on both sides, paged forwards. A
  listen not yet matched to an artist and less than a day old is left for a
  later read. A read that fails partway keeps what it got and moves its cursor
  that far. The request is made outside any transaction; the write happens
  under a lock on the link.
- **Failures** are stored on the link as a code (`unreachable`,
  `unknown_user`, `throttled`, `malformed`) that the interface puts into the
  reader's language, and the cause is logged for whoever runs the stand.
- **The rule.** One light per minute; five per minute for a star heard fewer
  than ten times before. Minutes are the length rounded, at least one, at most
  fifteen; an unknown length counts as three. A listen with several artists
  opens each and pays once at the rate of the newest. A listen nobody could
  name pays at the familiar rate and opens nothing. A row pays once, under the
  rule in force when it arrived.
- **Unlinking** stops the reading. What was gathered stays with the account
  and leaves with it -- in the export, and in the deletion.

## Consequences

- **Nothing on the way to the sky waits for ListenBrainz.** The card asks what
  you have heard of its star separately, after it has opened, and only from a
  person with listening to ask about. A ListenBrainz outage is a code on a link
  and a line in the log.
- **The charter grew one entry** naming exactly what is kept, and its line
  about not profiling taste was rewritten to what is true: the listening is
  used for nothing but the person's own sky.
- **Light accrues before the game exists.** Nobody has chosen a mode yet (that
  arrives with the fog, v0.22), and the ledger records listening whatever the
  interface later does with it.
- **Listening before the link is not read.** The history import is v0.18, and
  it inherits the windowed reading: a year of someone's history is about a
  hundred and twenty bounded requests, resumable from wherever the last one
  stopped.
- **A forged scrobble pays.** Anyone with a ListenBrainz token can submit
  listens that never happened. The ceiling on minutes per listen bounds what
  one claim is worth; nothing more is defended until light can be spent
  (v0.25).
