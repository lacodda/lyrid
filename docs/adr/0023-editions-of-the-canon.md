# 23. Editions of the canon

Date: 2026-10-16

## Status

Accepted.

## Context

The canon is rebuilt from dumps every few stages. Each importer records its
own run in `dump_import`, and the MusicBrainz import empties every canon table
(`TRUNCATE ... CASCADE`) before filling them again. So after a rebuild nothing
remembers the sky that was there before: "what changed in the sky" has no
answer, and "which sky is this" means reading import timestamps -- among them
probes and reruns that never built a sky at all.

v0.22 needs both answers: progress keyed by MusicBrainz id has to survive a
rebuild, and a profile has to say which sky its progress was made in.

## Decision

**An edition is declared, by `lyrid canon seal`,** at the end of the canon
pipeline: after the layout and the tiles, before a slice is cut. Sealing
records, in one transaction:

- the edition (`canon_edition`): the layout it is drawn by, by key, and how many
  artists and stars it holds;
- the newest finished import of each source (`canon_edition_source`), copied;
- its stars with their names (`canon_edition_star`);
- what changed from the edition before (`canon_change`): stars that are new,
  stars that left -- **merged** when MusicBrainz left a redirect from their id
  to another artist, **gone** otherwise.

One layout is one edition; sealing it twice is refused. Nothing in these
tables references the canon's, so they outlive the canon they describe.

**Merges come from MusicBrainz's `artist_gid_redirect`**, now imported with the
canon as `artist_redirect`: the same table v0.22 applies to progress.

**The sky says which edition it is.** The interface shows the edition beside
the version, and opens what changed in it: the brightest new stars, the merges
by the names they had and the stars they became, and the stars gone dark --
next to this week's risers and releases (ADR 0022).

## Consequences

- A slice is cut from a sealed canon and carries its edition, so a stand can
  tell it holds part of one: fewer artists than the edition counted.
- An edition sealed on a slice would compare a slice with a canon next time;
  the guide says to seal before cutting, and the admin page shows both counts.
- The first edition changed nothing, because nothing came before it. What
  changed becomes visible from the second, which arrives with the next rebuild.
- Rebuilding the sky without sealing it leaves the newest edition naming a
  layout that is no longer shown; the interface says so rather than hiding it.
