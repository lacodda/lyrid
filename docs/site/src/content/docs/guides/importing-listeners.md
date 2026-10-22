---
title: Importing listeners
description: How many people listen to each star, and who is listened to by the same people, from ListenBrainz's statistics dump.
---

Two numbers make the sky what it is: how brightly each star burns, and which
stars sit near each other. Both come from one file ListenBrainz publishes with
every full export: for each person, the thousand artists they have played most.

Run the [MusicBrainz import](/lyrid/guides/importing-musicbrainz/) first — the
file names artists by their MusicBrainz ids, and resolving them needs the canon.

## Run it

```sh
lyrid import listeners
```

```
INFO lyrid::import::listeners: reading who listens to whom dump=listenbrainz-dump-2723-20261015-000002-full
INFO lyrid::import::listeners: reading people people=20000 artists=…
INFO lyrid::import::listeners: people read; working out who is listened to alongside whom people=99812 artists=… unmatched=… unknown=… unreadable=0
INFO lyrid::import::listeners: similarity worked out; writing to PostgreSQL edges=…
INFO lyrid::import::listeners: similarity edges written rows=…
INFO lyrid::import::listeners: listeners written rows=…
INFO lyrid::import::listeners: listeners import complete dump=… edges=… listeners=…
```

There is nothing to download first. The import reads the newest full export
straight from `data.metabrainz.org` — or from `LYRID_DUMPS_URL`, as the daily
feed does — and `--dump listenbrainz-dump-2722-20261015-000002-full` names an
older one; the server keeps the last two.

The statistics dump is 23 GB, compressed as one stream, so it cannot be read in
pieces. But the file of people is its first: the import reads the stream until
that file ends and closes the connection. About 730 MB crosses the network,
nothing is written to disk, and the whole run takes a few minutes.

What it reports dropping:

- **`unmatched`** — artists in someone's list that ListenBrainz could not match
  to a MusicBrainz id, which is about a fifth of all entries.
- **`unknown`** — ids your canon does not hold: artists added to MusicBrainz
  after your dump.
- **`unreadable`** — lines that are not a person. Normally none.

An id MusicBrainz has since merged into another artist counts as that artist,
through the redirects the MusicBrainz import keeps.

## What it writes

**`artist_listeners`** — per star, how many people listen to it and their
listens summed. A listener is a person with the artist among the thousand they
have played most over all time. That is a real limit: a heavy listener's long
tail — artists played a handful of times, past their thousandth — is not in the
dump. So the number reads as *people who keep coming back*, which is what a
star's brightness should mean anyway. It counts people, not plays: one account
sending a hundred thousand listens is one listener.

**The `listenbrainz-listeners` metric** — the similarity of two artists is the
cosine of their sets of listeners: the people who have both, over the geometric
mean of how many have each. An artist needs five listeners to have neighbours
at all and a pair needs three people in common; each artist keeps its thirty
strongest neighbours, and the graph is the union of those lists.

Importing again replaces both. Other metrics — the
[2020 relations dataset](/lyrid/guides/importing-similarity/) — are left as
they are.

## What it looks like

Measured on the export of 2026-10-15: 99,812 people and 34 million
person-artist pairs. 283,060 stars end up with neighbours, joined by 6.89
million edges — 194,255 of those stars had no place on the sky drawn from the
2020 graph. The ten nearest neighbours of a star share its main Discogs genre
77% of the time (the 2020 graph: 73%), and nothing in the computation knows
what a genre is.

The nearest of a few, to judge by:

| Star | Nearest by listeners |
| --- | --- |
| Boards of Canada | Aphex Twin, Burial, Brian Eno, Autechre, Four Tet |
| Death Valley Girls | L.A. Witch, The Coathangers, Flat Worms, La Luz |
| Burzum | Darkthrone, Mayhem, Bathory, Emperor, Immortal |
| Miles Davis | John Coltrane, Herbie Hancock, Charles Mingus, Duke Ellington |

The plain cosine was chosen over weighted ones by measuring them on the same
dump — weighting each person by how many artists they list let accounts with
three artists name a star's nearest neighbours. The table is in
[ADR 0024](https://github.com/lacodda/lyrid/blob/main/docs/adr/0024-the-sky-from-the-people-who-listen.md).

## Then

The sky is drawn from this metric, and its stars are as bright as their
listeners:

```sh
lyrid layout --metric listenbrainz-listeners --tiles ./tiles
```

See [Building the sky](/lyrid/guides/building-the-sky/).
