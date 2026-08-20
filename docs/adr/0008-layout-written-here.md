# 0008 · The sky layout is written here, force-directed over a Barnes-Hut tree

Date: 2026-08-20. Status: accepted.

## Context

ADR 0003 settled that the canonical 2D layout is computed offline and
versioned, and that a tile pyramid is built from it. It did not settle *how*
the coordinates are computed. The input is the similarity graph: about three
million artists and seven million weighted edges, to be laid out on one
developer machine, in hours rather than days.

Every ready-made option was checked and failed for a specific reason:

| Option | Why not |
| --- | --- |
| **`forceatlas2` crate** (0.8.0) | **AGPL-3.0-only**, verified through the crates.io API. Incompatible with this product's MIT licence — the third infectious-licence wall on this project after MLHD+ and the MusicBrainz tags |
| **`annembed`** (0.1.6, MIT) — the only Rust library with a verified million-scale run | Accepts a graph **only through an HNSW index built from vectors**. `Embedder::new` takes a `&KGraph`, whose fields are `pub(crate)`; an existing graph cannot be handed to it without forking the crate. We have a graph and no vectors |
| **Spectral embedding** (Laplacian eigenmaps) | Rust has no maintained sparse eigensolver: `lanczos` last released 2024, `arpack-ng` 2023 and exposes one routine, `faer`'s EVD/SVD are dense-only. It would mean hand-rolling LOBPCG or an FFI dependency with Windows build risk |
| **sfdp** — the best measured number found (~96 min at 4M nodes / 35M edges) | No Rust implementation; shelling out to a Graphviz binary puts a foreign dependency in the release path |
| **node2vec → UMAP** | ~19 hours at one million nodes, ~45 at ten million. Outside the budget |
| **t-SNE, LargeVis, PaCMAP** | t-SNE is confirmed infeasible past ~200k; LargeVis has no Rust implementation and a dormant upstream; the newer methods are validated only to ~70k points |

## Decision

**The layout is written here**: force-directed, with repulsion approximated
through a Barnes-Hut quadtree.

This is affordable, and the numbers were computed rather than hoped:

- **Memory.** The graph in CSR form is about 250 MB at full scale (positions
  23 MB, both edge directions 107 MB, offsets 11 MB, force buffer 23 MB); the
  quadtree adds about as much again. Half a gigabyte against 32 available.
- **Time.** Barnes-Hut turns N² repulsion into N log N — roughly 65 million
  node-cell interactions per iteration at three million stars.
- **Only artists with edges are laid out.** An artist with no similarity has
  nothing to be near; those are the map's dark matter and are placed
  separately, which also keeps the graph smaller than the canon.

**Determinism is part of the design, not a footnote.** A layout is versioned,
so a seed has to mean one sky. Floating-point addition is not associative, so
accumulating forces across threads gives different results on different thread
counts even with a fixed seed — a documented trap in UMAP and in every
force-directed library that offers a `seed` parameter. Therefore:

- forces are accumulated **in index order**, not in parallel;
- artist ids are sorted into dense indices rather than taken in hash order;
- the iteration count is **fixed**, not "until it converges" — a convergence
  test makes the result depend on floating-point noise;
- initial positions come from a **golden-angle spiral**, a closed form needing
  no random number generator, with the seed rotating the whole arrangement.

**The tile format is deliberately dumb**: a 16-byte header and 16-byte records
of `(artist_id, x, y, brightness)`, little-endian. The client's hot path
uploads these straight into a GPU buffer, so anything requiring parsing would
be per-frame work for no benefit. Levels filter by brightness rather than by
resolution: a star admitted at one level stays present at every deeper one, so
zooming reveals more stars instead of making them pop in and out.

## Consequences

- The layout is ours to tune and ours to maintain. That is the cost of the
  licence and API walls above; the algorithm itself is well understood and the
  quadtree is covered by tests that compare it against the exact summation.
- Single-threaded force accumulation leaves performance on the table. This is
  deliberate: reproducibility of a versioned artefact is worth more than a
  constant factor on a batch job that runs once per canon.
- Layout quality is a judgement, not a proof. What is testable is tested —
  connected stars end up closer than unconnected ones, clusters separate,
  stronger edges pull harder — and the rest has to be looked at.
- A future GPU implementation would be much faster, and the tile format does
  not care how the positions were produced.
