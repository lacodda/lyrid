# 0003 · Sky map: tile pyramid + custom WebGL2 point renderer

Date: 2026-08-13. Status: accepted.

## Context

The sky must show a universe of ~2.5M artists with seamless pan/zoom from the whole galaxy down to a single artist's system, hold 60 fps on integrated GPUs and mobile, and carry the approved visual language (nebulae, fog of war, glow, object classes drawn by shape). This is the problem shape of a map service, not of a game engine: a huge static world browsed with a camera.

## Decision

**Data side — like a map service:**

- The canonical 2D layout is computed **offline** by a pipeline job (similarity embedding → projection) and **versioned**; the canon is never silently re-laid.
- A **tile pyramid** is built from the layout: each zoom level contains stars above a popularity threshold for that LOD; a tile is a small binary blob (~16 bytes/star: position, brightness, color, id). Tiles are **static files** served by axum, CDN- and browser-cacheable; browsing the sky never touches the database.
- The client fetches only visible tiles for the current zoom and caches them (memory LRU + IndexedDB).

**Render side — thin custom WebGL2:**

- Stars are **instanced point sprites**: all stars in view render in a single draw call; the vertex shader projects for the current camera, the fragment shader draws the gaussian glow. Twinkle is a time uniform + per-star phase, computed on the GPU.
- **Nebulae** are one full-screen shader pass over a low-resolution genre-density texture — not particles.
- **Fog of war** is a small per-user coverage texture (rasterized from the player's explored regions) sampled in the star shader with a smoothstep edge. It is the only truly per-user map data, kilobytes in size.
- **Similarity edges** render only past a zoom threshold, with a per-frame cap.
- **Labels** are an HTML overlay with collision culling (top-N visible names) — crisp, accessible text.
- Special objects (pulsars, supernovae, binaries, wormholes) are few per frame and drawn on top individually.
- The artist **system view** is a separate small scene (dozens of objects); the galaxy↔system transition is a crossfade at a zoom threshold.
- No heavy framework: the scene is homogeneous (points, lines, a few quads) and the product's look lives in our shaders. deck.gl/pixi are rejected; the tile logic follows the maplibre pattern without the dependency.

**Performance guards (designed in, not patched later):**

- Glow radius cap + additive blending to bound overdraw — the main fill-rate risk.
- Device pixel ratio clamped to 1.5; star budget per zoom level adapts to measured FPS of the first frames.
- The render loop pauses when the camera is idle or the tab is hidden; `prefers-reduced-motion` disables twinkle.
- Baseline is WebGL2 (~97% of browsers); a reduced-density fallback covers the rest. WebGPU is a later upgrade, not the foundation.

## Consequences

- The galaxy view draws 20–100k points — an order of magnitude below what instanced points comfortably sustain at 60 fps on integrated GPUs.
- Server load for map browsing is static-file traffic only; a layout release is a batch job producing a new tile set version.
- Before integration, the renderer is validated as a standalone prototype on real-scale data (hundreds of thousands of points) so FPS numbers are measured, not promised.
