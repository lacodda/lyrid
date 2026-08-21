---
title: The sky
description: Stars, nebulae, and the fog over them — what the map is made of and why the layout is the same for everyone.
---

The sky is the product. Everything else — the server, the imports, the pipelines — exists to put it on screen.

## Stars are artists

Every artist in the universe is a star. Brightness is popularity, so the map has an immediate visual hierarchy: the names everyone knows are the ones that carry across a zoomed-out view, and the long tail is there but faint.

Position is not decorative. Artists that are listened to together sit near each other, because the layout is a projection of a similarity graph. Distance on the map means something, which is what makes travelling across it worth doing.

## Nebulae are genres

Genres are not labels pinned to stars; they are regions. A nebula is drawn from the density of a genre in an area, so scenes appear as glowing clouds with real shapes — overlapping where genres overlap, sharp where they do not.

## The layout is canonical

One layout, computed offline and versioned, shared by everyone. This is a deliberate constraint:

- A coordinate is a **place**. You can link to it, screenshot it, and describe it to someone else — and they will see the same thing.
- Landmarks are shared. "Between the two big post-punk clusters" is a direction, not a private impression.
- Re-laying the map silently would move everyone's landmarks, so the canon is versioned rather than continuously recomputed.

How the coordinates are actually computed — and why the layout is written here rather than taken from a library — is in [Building the sky](/lyrid/guides/building-the-sky/) and [ADR 0008](https://github.com/lacodda/lyrid/blob/main/docs/adr/0008-layout-written-here.md). What the client receives is in the [tile format](/lyrid/reference/tile-format/).

## The fog is yours

The only per-user layer of the map is the fog of war: what you have and have not heard. It is a small coverage texture, kilobytes in size, sampled while stars are drawn.

In **exploration** mode the fog starts closed and lifts as you listen. In **creative** mode the sky is open from the first minute — the fog is still recorded, but as a record of your listening rather than a gate on it.

## Why it renders the way it does

The sky is a map, not a game world: a huge static universe browsed with a camera. That shape decides the architecture — a tile pyramid served as static files, and a thin custom WebGL2 renderer that draws every visible star in one instanced draw call. The reasoning is recorded in [ADR 0003](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md).
