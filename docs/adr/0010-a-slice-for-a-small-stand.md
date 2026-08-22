# 10. A slice for a small stand

Date: 2026-08-21

## Status

Accepted.

## Context

The canon measures 4251 MB on a real import of 2,959,388 artists. The deployed
stand is a Raspberry Pi 4 with 8 GB of memory that already runs other services,
so it cannot hold the full canon, and it does not need to in order to show a
sky.

Where the size actually sits is not where the product's value sits:

| Table | Size | Read by the sky or the card |
| --- | --- | --- |
| `artist_url` | 1264 MB | no |
| `release_group` | 742 MB | no |
| `artist` | 633 MB | yes |
| `artist_similarity` | 557 MB | yes, as the layout input |
| `label` | 427 MB | no |
| `artist_genre` | 151 MB | yes |

Only 206,636 artists have a position at all, because a star without edges has
nowhere to be placed. Everything else is import residue that a stand carries
without ever serving.

Measured on the real graph, the brightest 100,000 artists retain 5,462,464 of
5,955,657 similarity edges — **92% of the graph from 3.4% of the artists**.
Connectivity is concentrated, so a slice does not look like a thinned sky; it
looks like the sky.

## Decision

Cut the slice **after** importing in full, with a separate `lyrid slice`
command, rather than adding a filter flag to each importer.

Artists are ranked by graph weight — the same number the layout is built from
(ADR 0004) — so the slice keeps precisely what gives the sky its shape.

## Consequences

**The definition of "kept" lives in one place.** Five importers filtering
independently would be five chances to disagree, and disagreement here produces
a star with no edges or an edge with no star. One `DELETE` cannot disagree with
itself.

**The schema does the work.** All sixteen artist-referencing tables cascade on
delete, so removing an artist removes their URLs, release groups, genres, prose
and edges without the command naming any of those tables — and a table added
later is covered automatically, provided it cascades. `label` is the one large
table standing outside that graph and is dropped outright.

**A slice requires a layout.** Brightness comes from `artist_position`, so the
command refuses to run before `lyrid layout` and says so, rather than failing
on a constraint.

**The cut is one transaction.** A half-cut canon would be worse than an uncut
one; a stand that fails mid-prune still has a sky to serve.

**Space returns only after `VACUUM FULL`.** Postgres marks the rows dead but
keeps the files, which on a machine chosen for being small is the difference
that matters. The command says so when it finishes.

**Someone still needs a machine that can import in full.** This trades a
one-time cost on a capable machine for a permanently smaller stand, which is
the right way round: the import runs occasionally, the stand runs always.
