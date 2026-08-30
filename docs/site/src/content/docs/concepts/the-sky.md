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

### The address is the view

Because the map is the same for everyone, the URL can carry a place:

```
/star/54#-59.17,-69.55,12
 └ which card is open   └ where the camera is
```

The two halves behave differently on purpose. The **path** is what a link is
*about* — "look at this artist" — so it is a real route the server answers, and
it is what a chat preview or a crawler reads. The **fragment** is the camera,
and a fragment is never sent to the server: it is a client-side bookmark, which
is exactly what a camera position is. It also changes on every pan and zoom, and
putting that in the path would fill the session history with entries nobody
wants to press Back through — so the address is replaced rather than pushed.

A link to a star with no fragment flies to it. A link with one honours the
fragment instead, because it says where the sender was looking — which may be a
wide view holding that star among others. A fragment that has been truncated or
hand-edited opens the whole sky rather than a camera at nowhere.

The star whose card is open wears a **halo**, because arriving somewhere is no
use if you cannot tell which point of light you arrived at. It is drawn as a
second pass over the field rather than as a property of every star — the field
is one instanced draw call whose cost is measured, and a per-star comparison
would spend that budget on two hundred thousand stars to change one. Its size
is in pixels, not world units, so it stays a marker at every zoom rather than
becoming an object that grows as you approach.

The other way to take a view with you is a **poster**: the frame as drawn, saved
as a PNG at the resolution it is drawn. It is captured inside the render loop —
a WebGL colour buffer is undefined the moment a frame ends, so a screenshot
taken from outside would save an empty image.

## The fog is yours

The only per-user layer of the map is the fog of war: what you have and have not heard. It is a small coverage texture, kilobytes in size, sampled while stars are drawn.

In **exploration** mode the fog starts closed and lifts as you listen. In **creative** mode the sky is open from the first minute — the fog is still recorded, but as a record of your listening rather than a gate on it.

## Why it renders the way it does

The sky is a map, not a game world: a huge static universe browsed with a camera. That shape decides the architecture — a tile pyramid served as static files, and a thin custom WebGL2 renderer that draws every visible star in one instanced draw call. The reasoning is recorded in [ADR 0003](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md).
