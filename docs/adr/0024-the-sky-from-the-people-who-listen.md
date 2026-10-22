# 24. The sky from the people who listen

Date: 2026-10-22

## Status

Accepted. Supersedes the brightness half of ADR 0004, and its finding that
ListenBrainz publishes no counts of listeners.

## Context

ADR 0004 found two things missing from every dump: current similarity, and
how much each artist is listened to. So the sky was laid out from the one bulk
similarity dataset there was, published in 2020, and a star's brightness was
its connectivity in that graph. The plan for v0.20 was to compute both from
the full listen dumps -- 233 GB of JSON, or 232 GB of Parquet -- read through
once.

What a full export actually holds, measured on 2026-10-22
(`listenbrainz-dump-2723-20261015-000002-full`):

- Beside the listens, a **statistics dump**: 23 GB, one zstd stream over a
  tar, CC0 by its own `COPYING`. Its first file, `artists_all_time.jsonl`, is
  one line per person -- 99,812 of them -- with the thousand artists they have
  played most over all time, each with a listen count and the MusicBrainz id
  ListenBrainz matched it to.
- 34.0 million person-artist pairs carry an id, 9.7 million do not (names
  ListenBrainz could not match). 700,376 distinct artists; 30,611 people reach
  the thousand, and their thousandth artist has 2 to 51 listens.
- Listeners per artist: median 3, 90th percentile 52, 99th 813, the most
  30,369. 283,488 artists have at least five.

Measured against the sky of the 2020 graph, which places 206,636 stars:

- Of the 100,000 artists most listened to now (30 listeners or more), 42,945
  have a place on it.
- Of the 100,000 stars a stand holds -- the most connected -- 57,834 have five
  listeners; 62,800 placed stars have none at all.

The sky was missing more than half of what people listen to, and drawing tens
of thousands of stars nobody does.

## Decision

**Who listens comes from the statistics dump**, not from the listen dumps.
`lyrid import listeners` reads the newest full export's statistics as a stream
and stops at the end of its first file: about 730 MB of the 23 GB, three
minutes, nothing on disk. It counts people, as the weather does (ADR 0022):
one account sending a hundred thousand listens is one listener, which the full
listens would have needed aggregating a billion and a half rows to say.

**A listener is a person with the artist among their thousand most played.**
`artist_listeners` keeps, per star, how many people that is and their listens
summed. The cap is named in the column's meaning rather than hidden: a heavy
listener's long tail is not in the dump, so the count is of the people who
keep coming back to an artist.

**Brightness is listeners**, by the same formula as before: the square root of
a star's share of the brightest. Measured, the two give nearly the same sky --
median 0.024 against 0.021, 99th percentile 0.246 against 0.266, the cut of
the widest tile level 0.282 against 0.270 -- so the sky changes which stars
burn, not how a sky of them looks. A logarithm (median 0.275) would have lit
the whole tail. `artist_prominence` is dropped: two brightnesses would be two
answers to one question.

**Similarity by the people two artists share** lands as a second metric,
`listenbrainz-listeners`: the cosine of their sets of listeners, for artists
with at least five, pairs with at least three people in common, thirty
neighbours kept per artist and the lists' union stored. Measured against the
alternatives on the same dump:

| Variant | Genre coherence of the top ten | Agreement with 2020 |
| --- | --- | --- |
| Plain sets (chosen) | 0.767 | 0.420 |
| Each person weighted by 1/√(artists listed) | 0.748 | 0.382 |
| Listen counts, log-scaled | 0.768 | 0.395 |
| Both | 0.766 | 0.393 |
| Plain sets, pairs played once dropped | 0.769 | 0.418 (30,000 fewer stars) |

The 2020 graph itself scores 0.728 on genre coherence. Weighting by person let
accounts listing three artists name a star's nearest neighbours.

**The sky is laid out on the new metric.** 283,060 stars have edges in it,
194,255 of them new to the sky. The 2020 metric stays in the canon -- a
different notion of closeness the plan already has uses for -- but the layout,
the brightness and the card's neighbours read the metric of the current
layout, by name rather than by whichever metric was imported last.

**The listen dumps are not read for this.** They would give exact counts and
session-level co-listening, at about eighty gigabytes of ranges and hours of
work per refresh that a stand cannot do; the statistics give people, are
refreshed with every full export, and are read in minutes.

## Consequences

- Edition 2 of the canon is this sky. Its "what changed" is mostly stars
  arriving and stars going dark, which is what an edition is for; there is no
  player progress yet to carry over (that is v0.22).
- The graph can be refreshed with every full export ListenBrainz publishes:
  import, lay out, seal. A newer import replaces the metric's edges and the
  listeners; the layout keeps the positions it drew until it is drawn again.
- A stand's slice keeps the most listened of the placed stars.
- The dark matter at the sky's margins is now artists fewer than five people
  return to, and artists whose listeners share no one else.
- The count of a star's listeners is a lower bound for artists that live in
  heavy listeners' long tails. It is read as "people who return to it", and
  the card says how many on ListenBrainz.
