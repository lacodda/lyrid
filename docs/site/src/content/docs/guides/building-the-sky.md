---
title: Building the sky
description: Project the similarity graph into coordinates and cut the tile pyramid the client reads.
---

The canon holds who is similar to whom. This step turns that into a place:
coordinates for every star, and the tiles a browser fetches to draw them.

Run the [listeners import](/lyrid/guides/importing-listeners/) first — the
layout has nothing to project without a graph, and the tiles take each star's
brightness from its listeners.

## Run it

```sh
lyrid layout --metric listenbrainz-listeners --tiles ./tiles
```

`--metric` is needed once a canon holds more than one similarity metric — the
layout refuses to guess which notion of closeness the sky should show.

```
INFO lyrid::layout::build: laying out the sky metric=listenbrainz-listeners
INFO lyrid::layout::build: graph built; running the layout stars=… edges=…
INFO lyrid::layout::build: laying out iteration=10 movement=…
INFO lyrid::layout::build: positions written rows=…
INFO lyrid::layout::build: tiles written tiles=… kilobytes=…
```

Without `--tiles` the positions are stored and no files are cut, which is what
you want while trying parameters.

## Cutting tiles from a stored layout

```sh
lyrid tiles --out ./tiles
```

```
INFO lyrid::layout::cut: cutting the tile pyramid tiles=63
INFO lyrid::layout::cut: tiles written tiles=63 kilobytes=2774
INFO lyrid::layout::cut: labels written labels=148
```

The layout takes minutes to hours; cutting takes seconds. They are separate
commands because they change on different clocks — the pyramid changes whenever
its *format* does, and re-running the forces to ship a format change would be
absurd. `lyrid tiles` reads the newest stored layout (or `--layout KEY`) back
from the database, and `lyrid layout --tiles` calls the same code once the
positions are written, so the two can never cut different skies.

Cut **after** `lyrid slice`, from the database the stand will hold. Tiles cut
before slicing still carry every star of the full canon, and on a stand
holding the slice those extra stars draw but open no card.

## What the layout does

Stars repel each other; edges pull them together. Run that to a settled state
and artists listened to alongside each other end up near each other — which is
the entire claim the map makes.

Repulsion between every pair would be N² — four and a half trillion pairs at
three million stars. A **Barnes-Hut quadtree** replaces distant crowds with
their centre of mass, bringing it to N log N. The `--theta` flag controls when
a cell is far enough to summarise: 0.5 is the usual trade, smaller is more
exact and slower.

A star's **mass** is its weight in the graph, so a well-connected artist pushes
harder and claims room instead of being buried inside its own crowd.

Only artists **with edges** are laid out. An artist with no similarity has
nothing to be near; those are the map's dark matter.

## Why this is written here

Every ready-made option was checked and rejected for a reason worth knowing:

- The **`forceatlas2` crate is AGPL-3.0**, incompatible with this product's
  MIT licence.
- **`annembed`** — the only Rust library with a verified million-scale run —
  accepts a graph only through an HNSW index built from vectors, and its graph
  type cannot be constructed from outside the crate. We have a graph and no
  vectors.
- Rust has **no maintained sparse eigensolver**, so a spectral embedding would
  mean hand-rolling one.
- **sfdp** has no Rust implementation.

The full reasoning is in
[ADR 0008](https://github.com/lacodda/lyrid/blob/main/docs/adr/0008-layout-written-here.md).

## A layout is a version, not a fact

Recomputing moves every star, so layouts are versioned the way similarity
metrics are. Coordinates are comparable **within** a layout and never across
two.

```sql
SELECT key, description, seed, stars, created_at FROM sky_layout;
```

Reproducibility here takes more than a seed. Floating-point addition is not
associative, so accumulating forces across threads gives different results on
different machines even with the same seed — a documented trap in every
library of this kind. So the layout:

- accumulates forces **in index order**, not in parallel,
- sorts artist ids into dense indices rather than taking them in hash order,
- runs a **fixed** number of iterations rather than stopping when it looks
  settled,
- starts from a golden-angle spiral, a closed form that needs no random number
  generator at all — `--seed` rotates the whole arrangement.

Same input, same flags, same sky. That is what makes `--key` meaningful.

## The tile pyramid

```
tiles/
  sky.json        the world bounds, record size and the cut's stamp
  labels.json     genre and style names, and where to write them
  0/0/0.bin       the whole sky, brightest stars only
  1/0/0.bin  …    four times as many tiles, four times as many stars
```

Levels filter by **brightness**, not by resolution. Level 0 shows the
brightest `--level0-stars`; each level down quadruples both the tile count and
the star budget, so the bytes per tile stay roughly flat while zooming reveals
more stars. A star admitted at one level is present at every deeper one, so
nothing pops in and out as you pan.

The format is deliberately plain — a 16-byte header, then 20-byte records:

| Bytes | Field |
| --- | --- |
| 0–3 | `artist_id`, little-endian `i32` |
| 4–7 | `x`, `f32` |
| 8–11 | `y`, `f32` |
| 12–15 | `brightness`, `f32` in 0..1 |
| 16–17 | `begin_year`, `i16`, `0` when unknown |
| 18–19 | reserved |

The [tile format](/lyrid/reference/tile-format/) has the details, including
`labels.json`.

The client uploads these straight into a GPU buffer, so JSON or protobuf would
be parsing work per frame for no benefit.

**Brightness is how many people listen** — `artist_listeners`, from the
[listeners import](/lyrid/guides/importing-listeners/) ([ADR 0024](https://github.com/lacodda/lyrid/blob/main/docs/adr/0024-the-sky-from-the-people-who-listen.md)).
It is normalised against the brightest star and square-rooted, so the long
tail stays visible instead of collapsing into the hubs. Measured, that gives
nearly the same sky the graph's own connectivity gave before v0.20 — the
median star at 0.024 against 0.021 — with the stars people listen to burning
in place of the ones most woven into an old graph. Cut tiles again after
importing listeners: the brightness is baked into them.

## What a real run looks like

Measured against the full canon on 2026-10-22 — 2.96M artists, of which
283,062 have neighbours in the `listenbrainz-listeners` metric, joined by 6.89M
edges:

| | |
| --- | --- |
| Stars placed | 283,062 |
| Iterations | 300 |
| Time | 11.4 minutes |
| Tiles | 255 files, 8.9 MB |

**Distance tracks similarity**, which is the claim the map makes. Averaged over
200,000 edges of the finished layout:

| Edge strength | Average distance |
| --- | --- |
| Strong (≥ 0.5) | 73 |
| Medium (0.2–0.5) | 189 |
| Weak (0.1–0.2) | 357 |
| Faint (< 0.1) | 810 |
| Two stars picked at random | 1,612 |

**Genres separate without being told to.** Discogs genres take no part in the
layout — it sees only who listens to whom — yet the stars of one main style sit
in a region about a twelfth of the sky's width: the median spread of the 208
styles with more than a hundred stars is 0.089 of the width, where the sky of
the 2020 graph managed 0.159. Blues lands with blues because the same people
listen to it.

## Judging a layout

Some of it is testable and tested: connected stars end up closer than
unconnected ones, clusters separate, stronger edges pull harder. The rest has
to be looked at. Useful questions to ask of a finished layout:

```sql
-- Do artists of one genre sit together? Compare with a random sample.
SELECT g.name, count(*), stddev(p.x) AS spread_x, stddev(p.y) AS spread_y
FROM artist_position p
JOIN artist_genre ag ON ag.artist_id = p.artist_id AND ag.releases > 5
JOIN genre g ON g.id = ag.genre_id
WHERE p.layout_id = 1 AND g.is_style
GROUP BY g.name HAVING count(*) > 50
ORDER BY spread_x + spread_y
LIMIT 20;
```

A genre whose members are spread as widely as a random sample is a sign the
layout has not separated it — either too few iterations, or too weak an
attraction.

## Sealing the edition

Once the layout is drawn, the tiles are cut and the sky has been looked at, seal it:

```sh
lyrid canon seal
```

Sealing records the sky as the next **edition** of the canon: the newest import of each source, the layout by its key, and every star with its name. It then compares that list with the edition before and records what changed — stars that are new, stars MusicBrainz merged into another artist (the MusicBrainz import keeps its redirects in `artist_redirect` for this), and stars that went dark. The sky names its edition in the status line and shows what changed in it; see [What changed in the sky](/lyrid/concepts/what-changed-in-the-sky/).

Seal the full canon, before cutting a slice: a slice sealed as an edition would be compared with a whole canon next time and report everything it left out as gone. One layout is one edition — sealing the same layout again is refused, so laying the sky out again is what makes room for a new edition. Measured on the full canon: 206,636 stars sealed in 15 seconds.
