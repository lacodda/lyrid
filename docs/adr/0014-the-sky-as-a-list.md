# 14. The sky is also a list, and the server answers what is in view

Date: 2026-09-19

## Status

Accepted.

## Context

A WebGL canvas is one element with no children. Whatever the renderer draws —
206,636 stars here, two and a half million when the canon is whole — the
document contains a single `<canvas>` and nothing else.

Two consequences, and neither is a rendering detail:

**There is no keyboard path to any star.** Every one of them is reachable only
by aiming a pointer at a few pixels of light. Tab does nothing. That is not a
gap in the accessibility of the product; it means the product's entire content
is unreachable without a mouse.

**A screen reader is told nothing.** The canvas had no label at all, so it was
announced as a blank rectangle — and even labelled, a label is not content.

A third thing is true of sighted readers too, and it is worth saying because it
changes what the fix should be: the canvas draws points of light, and points of
light have no names on them at any zoom. "What am I actually looking at" is a
fair question for anybody in front of a star field.

## Decision

**The stars in view are also rendered as text, in a list beside the canvas.**
The canvas carries `role="img"` and a label saying what it is and that the list
is the way through it by keyboard.

Three things this is not:

**Not a hidden block for screen readers only.** A list nobody can see is a list
nobody notices has gone stale — and it would answer only two of the three
problems above. It is visible chrome, small, in the stack at the bottom left.

**Not an equal alternative to the map, and it does not pretend to be.** It holds
the twelve most prominent names in view; moving the view is how you get others.
A complete alternative to a two-million-star map is another map, not a list.

**Not built in the browser.** This is the part that looks like it should be
cheaper client-side and is not. The tiles the canvas draws from carry ids and
positions but **no names**, and above level zero they are a thinned sample
rather than everything in view. A list assembled from the loaded tile would be
the stars that happened to survive thinning, each one named by a request of its
own.

So the server answers it: `GET /api/nearby` takes the visible rectangle and
returns up to twelve named stars inside it, ordered by prominence. The
`artist_position (layout_id, x, y)` index already existed for the tile builder
and serves a bounding box directly.

Three details that are decisions rather than mechanics:

**Ordered by prominence, not by distance from the centre.** A reader asking what
they are looking at wants the names worth knowing in this patch of sky. The star
nearest the exact centre of an arbitrary pan is nobody in particular.

**A rectangle given backwards is normalised, not answered empty.** `BETWEEN 5
AND -5` matches nothing in Postgres, so a viewport sent the wrong way round
would report an empty patch of sky — which reads as "there is nothing here"
rather than as "you asked wrongly", and sends whoever wrote the client looking
for missing data. A bound missing altogether *is* refused, with `400`:
defaulting an edge to zero would answer a rectangle nobody asked about, and the
answer would look plausible.

**The visible rectangle is handed out by the sky, not derived by the caller.**
`SkyState` carries it. The projection from a camera to a rectangle needs the
canvas size in device pixels, which only the sky component has; a consumer
computing it from the camera alone would be wrong on every display whose device
pixel ratio is not one.

## Consequences

The camera moves on every animation frame, and `SkyState` is handed out on every
frame with it. A list that refetched on that would spend a request per frame, so
the rectangle is quantised to a thousandth of its own span before it becomes a
dependency, and the request is debounced by 400 ms on top. The step is derived
from the span rather than fixed: the same absolute movement is a different
fraction of the view at every zoom, and a fixed grid would be too coarse zoomed
in and too fine zoomed out. A zero-span rectangle — the first frame, before the
canvas is measured — would divide by zero, so it falls back to a step of one.

Those three are geometry that goes wrong silently, so each is held by its own
test and each was checked by mutation: the camera as the rectangle's corner
instead of its centre, a square viewport instead of the canvas's aspect, a fixed
quantisation step, and the missing zero guard all fail a test that names them.

A request now happens while panning, where previously browsing the sky touched
the API not at all. That claim in the architecture guide is narrower now, and
the guide says so. It is one request per settled view rather than per frame, and
it buys the product a keyboard.
