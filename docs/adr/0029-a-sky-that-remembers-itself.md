# 29. A sky that remembers itself

Date: 2026-11-14

## Status

Accepted. Answers the question [ADR 0026](0026-the-fog-and-progress-across-editions.md)
left open -- "a layout moves every star" -- and extends the editions of
[ADR 0023](0023-editions-of-the-canon.md).

## Context

The canon is rebuilt every few stages, and until now every rebuild laid the sky
out from the same spiral. The fog follows a star to its new place (ADR 0026), but
nothing else does: a person who has spent a month learning where their home sector
is finds it somewhere else, a camera saved in the account is dropped because it
belongs to a layout that is gone, and a link to a view (`#x,y,scale`) opens on
empty space. On a stand it is worse: the profile's camera named its layout by a
foreign key with `ON DELETE SET NULL`, so every fill of the stand dropped every
camera, even a fill that changed nothing.

Three smaller things rode on the same rebuild:

- **"Gone dark" was alphabetical.** Edition 2 named `!!*`, `!ASQ!`, `!D!` first of
  117,831. Ordering by today's listeners cannot help: a star that left the canon
  took its listeners with it.
- **MusicBrainz's special purpose artists were stars.** "[unknown]" stood 194th
  brightest on the sky, "Various Artists" 2,747th; v0.23 kept them out of search,
  radio and the echoes, but only a new layout takes them off the sky.
- **The Wikidata label pass decompressed the whole dump again** -- three hours --
  for tens of thousands of labels in a small share of its blocks.

## Decision

**An edition keeps where its stars were and how bright they were.**
`canon_edition_star` gains `x`, `y` and `rank` (1 the most listened, in the order
the tiles draw and the slice keeps). The history is a table, so it survives the
canon it describes, as the editions already do.

**A new layout continues the newest edition by default.** Each star starts where
the edition had it, found by MusicBrainz id and, failing that, through the
redirect a merge left; when several old stars became one, the artist's own place
wins, then the brightest of the merged. A new star starts at the middle of its
placed neighbours, weighted by similarity, in rounds outward, so a newcomer whose
neighbours are all new lands beside them; a new island of the graph starts on a
ring just outside the sky. Then 40 rounds of the same forces, started at the
heat a fresh run of 300 ends at (a tenth), with every old star held to its old
place by an anchor of 10 per unit of distance, after the division by degree so
a hub and a leaf are held alike. `--fresh` draws from the spiral instead, for a
graph unlike the one before.

Why an anchor that firm: a fresh run stops when its rounds run out, not when its
forces balance, so a settled sky is not at rest. Continued with an anchor of 1,
the same graph with next to nothing changed drifted 7.5 units on the median and
14.8 at the ninetieth percentile -- the whole reach of the 14 units the fog lights
around an open star. At 10 it held.

**A frame names the coordinates a layout is drawn in.** `sky_layout.frame`: a
continued layout keeps its edition's frame, a fresh one starts its own (the
database fills in its key when no frame is given, so no writer can leave one
out). `canon_edition` copies it. The camera is kept by the frame
(`user_profile.camera_frame` replaces `layout_id`), so it survives a new edition in
the same frame and a fill of the stand, and is dropped by a sky drawn fresh. With
that, nothing outside the canon points into it any more.

**Sealing measures the drift.** When the edition before is in the same frame,
sealing records how far the stars both editions place moved -- median, ninetieth
percentile and the furthest -- on the edition, prints it, and the admin page shows
it. Across frames there is nothing to measure, and the sky says the map was drawn
afresh.

**"Gone dark" is ordered by the place a star had in the edition it left.**

**The special purpose artists are off the sky.** The layout leaves their edges out
of the graph and the tiles leave them out of the cut, from the one list
(`import::special`).

**The label pass reads only the blocks it needs.** The fact pass already sees
every line go by: it writes down the block each item's line starts in, by
Q-number (four bytes each, half a gigabyte at 140 million), and the parallel
decoder keeps where each block lies in the file. The label pass then decompresses
the wanted items' blocks on every core, straight from the file, following a line
into the blocks after it when it runs on. A dump streamed from the network cannot
be read from the middle, and is still read through.

## Consequences

- **Measured on edition 3 of the canon** (282,887 stars, 6.88 million edges), with
  one star in a hundred hidden from the previous places so it had to be placed as
  new. The 279,957 held stars moved 0.84 units on the median, 1.67 at the ninetieth
  percentile and 3.2 at the ninety-ninth. The 2,930 placed as new sat as close to
  the stars they are similar to as the whole layout had put them: a
  similarity-weighted distance to their neighbours of 297 units on the median
  against the full layout's 332, and within a tenth of it or closer for 98.7% of
  them. Forty rounds took five minutes in a debug build.
- **A continued sky cannot leave a bad layout behind.** The anchor holds every old
  star near where it was, so a region drawn badly stays drawn badly; drawing fresh
  is the cure, and costs the frame. That is a choice to make knowingly, which is
  why it is a flag and not a default.
- **Edition 4 is the first in a continued frame.** Its drift is the one recorded on
  the stand; the editions before it were each drawn fresh.
- **The editions before this one were filled in once, by hand**, on the full
  canon, from what it still held: the places from each edition's layout, still
  stored; the ranks of editions 2 and 3 from `artist_listeners`, and of edition 1
  from the 2020 graph's prominence, recomputed from its edges (the backup it was
  to come from was no longer on disk). The 80 stars of edition 1 and 166 of edition 2
  the canon has since lost have neither, so edition 3's thirty gone-dark stars are
  still named by name. Edition 2's list now opens with The Overlanders, Michael
  Morgan and Nico Gemba, where it opened with `!!*`.
