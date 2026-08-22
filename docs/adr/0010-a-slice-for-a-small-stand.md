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

**Children are cleared in bulk before their parents.** This is the one place
where the obvious implementation is unusably slow, and the reason is worth
recording. `ON DELETE CASCADE` in PostgreSQL is enforced by a **per-row
trigger**, not a set operation: `EXPLAIN ANALYZE` on a 200-row delete reports
`calls=200` for each of the sixteen constraints. Measured at 5.46 ms per
artist, deleting 2.86M artists directly projects to **over four hours**, and a
first attempt was cancelled after 44 minutes without finishing.

Emptying the child tables first is a large win but not a complete one, and the
distinction is worth being precise about. The triggers still fire once per row
— they simply find less to do. Measured after the children were cleared, the
per-artist cost fell from 5.46 ms to roughly 0.9 ms, so the final delete over
2.86M artists still takes tens of minutes. A full cut of the real canon,
measured end to end, took **about an hour**: the fourteen child tables in
roughly five minutes and the artists themselves for the rest.

**The list of tables comes from the catalogue, not from this code.** Reading
`pg_constraint` for everything referencing `artist` with `ON DELETE CASCADE`
keeps the property the cascade was providing: a table added later is covered
without anyone remembering to edit the command. `label` references no artist at
all and is dropped outright.

Two smaller traps sit alongside it. The temporary table of kept artists must be
`ANALYZE`d after it is filled, or the planner assumes a handful of rows and
picks a nested loop for every prune. And the identifiers interpolated into
those statements are validated before use — they come from the catalogue rather
than from input, but a statement that cannot be parameterised should not rest
on that alone.

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

## Confirmed

A full cut was run against a copy of the real canon. It kept exactly 100,000
artists and 5,462,460 edges, every one of them with a position, and left **no
orphaned row** in `artist_similarity` or `artist_position` — the property the
cascade was there to guarantee, verified rather than assumed.
