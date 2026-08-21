# 0009 · The frame budget, measured

Date: 2026-08-21. Status: accepted.

## Context

ADR 0003 chose a thin custom WebGL2 renderer over deck.gl or pixi, and made a
promise it deliberately left unproven: *"before integration, the renderer is
validated as a standalone prototype on real-scale data (hundreds of thousands
of points) so FPS numbers are measured, not promised."* It also predicted the
galaxy view would draw 20–100k points and claimed that was "an order of
magnitude below what instanced points comfortably sustain at 60 fps on
integrated GPUs".

The layout stage produced the data that makes the promise testable: 206,636
stars in a real tile pyramid.

## What was measured

A standalone page (`web/prototype/sky.html`) loads the real tiles, draws every
star as an instanced point sprite in **one draw call**, and runs a fixed camera
sweep over every pyramid level. Open with `?benchmark` and it reports frame
times; the sweep is identical each run, so the number does not depend on how
the mouse moved.

Hardware: **Intel Iris Xe integrated graphics** — the low end ADR 0003 names —
at 2400 × 1218 with the device pixel ratio clamped to 1.5.

| Level | Stars | FPS | Median frame | Worst frame |
| --- | --- | --- | --- | --- |
| 0 | 2,000 | 60 | 16.7 ms | 16.9 ms |
| 1 | 8,000 | 60 | 16.7 ms | 16.9 ms |
| 2 | 32,000 | 60 | 16.7 ms | 17.0 ms |
| 3 | 128,000 | 60 | 16.7 ms | 16.9 ms |
| **4** | **206,636** | **60** | **16.7 ms** | **17.0 ms** |

Sixty frames a second at every level, with no dropped frame anywhere: the
worst frame in a three-second sweep is 17.0 ms against a 16.7 ms budget.

**A frame pinned to vsync says only "fast enough", not how much room is left**,
so the prototype can multiply the work with `?stress=N`, drawing the scene N
times per frame:

| Instances per frame | FPS |
| --- | --- |
| 1,024,000 | 60 |
| 4,096,000 | 45.7 |
| 6,612,352 | 26.6 |

Integrated graphics sustains roughly **one million instanced point sprites at
60 fps**. The real sky is 206,636 of them — about a fifth of that, a five-fold
headroom on the weakest hardware in scope.

## Decision

**The architecture of ADR 0003 is confirmed by measurement**, and its
prediction was conservative: the sky draws twice the top of the predicted
range at 60 fps with room to spare.

The prototype stays in the repository as `web/prototype/`. It is not part of
the product, but it is the instrument that answers "did that change cost us
frames?" — and re-running it costs twenty seconds.

Specific choices the measurement validates:

- **One instanced draw call.** No batching strategy is needed; the whole sky
  is one call.
- **The 16-byte record.** Uploading a tile is a `Float32Array` view and a
  `bufferData`; no parsing appears in the profile.
- **The device pixel ratio clamp of 1.5.** Fill rate is the risk in a
  glow-heavy scene, and the stress runs show the fragment stage is where the
  ceiling eventually appears.
- **Gaussian glow in the fragment shader**, rather than textured sprites: a
  texture fetch would buy nothing at these frame times and would cost a
  binding.

## Consequences

- The star budget per zoom level does not need to adapt to measured FPS on
  this class of hardware, as ADR 0003 hedged it might. That guard can stay
  unimplemented until a device is found that needs it.
- Nebulae, fog of war and edge rendering are still unmeasured. Each adds a
  pass, and the headroom above is the budget they draw from.
- The measurement is one machine. It is the weak end of the range rather than
  the strong one, which is the useful direction to be certain in, but a phone
  is not an Iris Xe and remains untested.
