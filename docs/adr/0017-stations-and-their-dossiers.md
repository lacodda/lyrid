# 17. Stations and their dossiers: labels from the Discogs releases file, scenes from places of origin

Date: 2026-09-30

## Status

Accepted. Amends [ADR 0005](0005-genres-from-discogs.md), which kept the
Discogs releases file out of the import.

## Context

v0.15 opens the sky's stations into pages: a label or a scene, with its roster,
its chronology and where its stars sit on the map. The plan assumed the data
was already in the canon since v0.4. It was not, in three ways.

**Nothing joined a label to an artist.** v0.4 imported the labels file — 2.4
million imprints, each with a description and an owner — and the masters file,
where the genres are. A master carries no label. Discogs joins a label to an
artist only through a release, and the releases file (11.2 GB compressed for
20260801) was deliberately not read: for genres it only repeats what the masters
say.

**The slice threw every label away.** Nothing the map or the card served read a
label, so cutting the canon for a stand dropped the table outright.

**The card's labels were another vocabulary.** The card listed record labels
from Wikidata's P264 claims: Wikidata items, with no years, no weight, and no
link to the Discogs labels the canon already held.

Three sources could give a roster: MusicBrainz's `release_label`, Wikidata's
P264, and the Discogs releases file.

## Decision

**A label station is a Discogs label, and its roster comes from the releases
file of the same dump.** The canon already holds Discogs labels, with the
description kept "for the station page" and the ownership between imprints; a
second label entity from MusicBrainz would be a second truth about the same
thing. Discogs is also the fullest open record of labels there is, and CC0. The
join to the canon is the one genres use — MusicBrainz's own `discogs` artist
link, `artist_discogs` — never names.

- **What counts** is decided once, in the importer: official releases only (a
  release whose format says "Unofficial Release" names a label that never agreed
  to it); no "Not On Label …", which is Discogs saying there was none; a label
  printed twice on one release (two catalogue numbers) counts once; pressings
  count, because a label that issued an album in twelve countries did issue it
  twelve times, as Discogs's own label pages count.
- **`label_artist`** holds the roster: the release count on the label, and the
  first and last year of those releases — an artist's span on the label.
- **`label_year`** holds the label's output year by year over **all** its
  official releases, not only the canon's: this is the label's own history, and
  one drawn from the canon's artists alone would show a label quiet in exactly
  the years it was busiest with acts the canon does not hold. Only labels with a
  roster keep one. A label's first year, last year and total are read from here,
  not stored beside it.
- **The four files of a dump go in together.** `--labels` requires `--releases`
  and `--artists`: a label without its roster is half a station, and a
  description pointing at artists by bare id names nobody.
- **Descriptions are named at import.** Discogs writes `[a674]` for an artist and
  `[l123]` for a label — 81,000 and 32,000 such references in the 20260801 file —
  and an id alone cannot be shown. Each file's pass picks up the names the
  descriptions point at, as the Wikidata import picks up item labels, and the
  stored text says `[a674=Stephan Grieder]`. The API turns it into segments: an
  artist the canon links to that Discogs id becomes a star, a label with a
  dossier becomes a link, and only `http` and `https` addresses become links at
  all.

**A scene is a place of origin**, from the facts the Wikidata import already
holds: P740, where a group formed, and P19, where a person was born. The two are
different claims and are counted apart, as the card keeps them apart. A place
nobody on the sky comes from is not a scene.

**The slice keeps the labels a stand can open**: those with a roster among the
kept artists, and every label that owns one, however far up, so "part of" still
leads somewhere. The rest go — nearly all of 2.4 million.

**The card's labels are the stations.** It lists the Discogs labels an artist
released on, most releases first, each opening its dossier. The Wikidata P264
claims stay in the canon as imported data and are no longer shown.

**A dossier opens over the sky**, like the spectrograph, at an address of its
own — `/label/{discogs id}`, `/scene/Q{wikidata item}`. Its map places every
placed member on the whole sky, because whether a station gathers in one part of
the sky or scatters over it is the thing the dossier exists to show.

## Consequences

- The Discogs import reads 11.2 GB more and holds the roster pairs of every
  canon-linked artist in memory while it does; it stays one command and one
  transaction, and the canon's genres, labels and rosters always share a version.
- `data.discogs.com` sits behind a rate limiter that answers 429 to bursts and
  cuts long transfers; the download is resumed, with long pauses, rather than
  retried.
- Rosters count pressings, so a label that reissues heavily weighs an artist
  more than one that issued them once. The count is stored, not hidden, as with
  genres.
- Scenes are as fine as Wikidata's places: Brooklyn is its own scene beside New
  York City, and Madchester is Manchester. A place hierarchy would need Wikidata's
  P131, which is not imported.
- A person's scene is their birthplace, which is not always where they made
  music. The dossier says "born here" rather than hiding the difference.
