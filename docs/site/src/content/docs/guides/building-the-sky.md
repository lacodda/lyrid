---
title: Building the sky
description: Project the similarity graph into coordinates and cut the tile pyramid the client reads.
---

The canon holds who is similar to whom. This step turns that into a place:
coordinates for every star, and the tiles a browser fetches to draw them.

Run the [similarity import](/lyrid/guides/importing-similarity/) first — the
layout has nothing to project without a graph.

## Run it

```sh
lyrid layout --tiles ./tiles
```

```
INFO lyrid::layout::build: laying out the sky metric=listenbrainz-2020
INFO lyrid::layout::build: graph built; running the layout stars=… edges=…
INFO lyrid::layout::build: laying out iteration=10 movement=…
INFO lyrid::layout::build: positions written rows=…
INFO lyrid::layout::build: tiles written tiles=… kilobytes=…
```

Without `--tiles` the positions are stored and no files are cut, which is what
you want while trying parameters.

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
  sky.json        the world bounds and record size
  0/0/0.bin       the whole sky, brightest stars only
  1/0/0.bin  …    four times as many tiles, four times as many stars
```

Levels filter by **brightness**, not by resolution. Level 0 shows the
brightest `--level0-stars`; each level down quadruples both the tile count and
the star budget, so the bytes per tile stay roughly flat while zooming reveals
more stars. A star admitted at one level is present at every deeper one, so
nothing pops in and out as you pan.

The format is deliberately plain — a 16-byte header, then 16-byte records:

| Bytes | Field |
| --- | --- |
| 0–3 | `artist_id`, little-endian `i32` |
| 4–7 | `x`, `f32` |
| 8–11 | `y`, `f32` |
| 12–15 | `brightness`, `f32` in 0..1 |

The client uploads these straight into a GPU buffer, so JSON or protobuf would
be parsing work per frame for no benefit.

**Brightness is connectivity**, not popularity — no listen counts exist as a
dump ([ADR 0004](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)).
It is normalised against the brightest star and square-rooted, so the long
tail stays visible instead of collapsing into the hubs.

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
