---
title: Tile format
description: The binary layout of a sky tile, and the pyramid it belongs to.
---

Tiles are what the client draws. They are static files — no database, no API —
so they can be served by anything and cached anywhere. This page is the
contract between the layout job that writes them and the renderer that reads
them.

## The pyramid on disk

```
tiles/
  sky.json
  labels.json
  0/0/0.bin
  1/0/0.bin
  1/0/1.bin
  1/1/0.bin
  …
```

A tile's path is `{level}/{column}/{row}.bin`. Level `n` divides the sky into
`2^n × 2^n` squares, so level 0 is one tile and level 6 is 4096.

A tile that would be empty is not written. The client must treat a `404` as
"no stars here", not as an error.

## `sky.json`

```json
{"min_x":-812.4,"min_y":-799.1,"max_x":804.6,"max_y":817.9,"max_level":6,"record_bytes":20,"stamp":1790148513}
```

The world bounds are a property of **this layout**, not of the product: a new
layout moves every star and gets its own bounds. The client needs them to turn
a screen position into a tile:

```js
const side = 2 ** level;
const span = sky.max_x - sky.min_x;
const col  = Math.floor((x - sky.min_x) / span * side);
const row  = Math.floor((y - sky.min_y) / span * side);
```

The sky is square by construction, so one scale factor serves both axes.

### The stamp

`stamp` names this cut of the sky — the moment it was written. A client puts it
into every tile and label address:

```
/tiles/2/1/3.bin?v=1790148513
```

Tiles are served with a day of browser cache, under paths the next cut reuses.
Without the stamp a returning visitor would draw yesterday's pyramid for a day —
and after a format change, refuse it and draw nothing. `sky.json` itself is
never cached, so the stamp reaches every visitor at once and each cut's files
become addresses of their own. The server ignores the query string; it only has
to differ.

## A tile

Little-endian throughout. A 16-byte header, then fixed-size records — nothing
that needs parsing, because the client's hot path uploads the record block
straight into a GPU buffer.

### Header

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | Magic `LYST` |
| 4 | 2 | Format version, currently `2` |
| 6 | 1 | Level |
| 7 | 1 | Padding, zero — keeps the records 4-byte aligned |
| 8 | 4 | Star count, `u32` |
| 12 | 4 | Reserved, zero |

A reader must check the magic and version and refuse anything else: a tile
left over from an older layout would otherwise be drawn as garbage.

### Records

`count` records of 20 bytes each, starting at offset 16:

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `artist_id`, `i32` — what a click turns into an artist |
| 4 | 4 | `x`, `f32` — world coordinates, the same space as `sky.json` |
| 8 | 4 | `y`, `f32` |
| 12 | 4 | `brightness`, `f32` in 0..1 |
| 16 | 2 | `begin_year`, `i16` — the year the act began, `0` when unknown |
| 18 | 2 | Reserved, zero — keeps the next record 4-byte aligned |

The year is what the time machine and the era lens draw from. It follows the
card's rule: MusicBrainz's own begin year, and Wikidata's inception only where
MusicBrainz has none. A year outside 1000–2100 is a typo in crowdsourced data
and is written as `0`, so it is drawn as undated rather than at the wrong end of
the slider.

Reading one in the browser is a typed-array view, no loop:

```js
const buffer = await (await fetch(url)).arrayBuffer();
const count  = new DataView(buffer).getUint32(8, true);
const stars  = new DataView(buffer, 16, count * 20);
```

## What levels mean

**Levels filter by brightness, not by resolution.** Level 0 holds the
brightest stars in one tile; each level down quadruples both the tile count
and the number of stars admitted, so the bytes per tile stay roughly flat
while zooming reveals more of the sky's population.

Two consequences worth relying on:

- **A star admitted at one level is present at every deeper level.** Zooming
  in never makes a star disappear, which would read as a bug rather than as
  detail.
- **The pyramid stops early when every star fits.** A small canon may have no
  level 3 at all; `max_level` in `sky.json` says where it ended.

## Brightness

Brightness is **connectivity**, not popularity — no per-artist listen counts
are published as a dump, which
[ADR 0004](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)
settles. It is the star's weight in the similarity graph, normalised against
the brightest star and square-rooted so the long tail stays visible instead of
collapsing into the hubs.

Because it is normalised per layout, brightness is comparable **within** a sky
and not across two.

## `labels.json`

The names written on the sky, cut in the same pass as the tiles:

```json
{"version":1,"labels":[
  {"name":"Rock","kind":"genre","x":-41.6,"y":-56.4,"members":17026,"lift":4.7},
  {"name":"Soul","kind":"style","x":3.9,"y":4.6,"members":1189,"lift":29.6}
]}
```

| Field | Meaning |
| --- | --- |
| `kind` | `genre` — one of Discogs's fifteen, a continent; `style` — a scene inside one |
| `x`, `y` | Where to write the name: the mean of the members in the densest 3×3 block of a grid laid over the sky |
| `members` | Placed artists that carry this as their **main** genre or style |
| `lift` | How many times denser the name is in that block than an even spread would be |

A label sits at its genre's **densest place**, not at the mean of all its
members: a genre is one dense core and a long scatter of crossover acts, and the
mean of both lands in the empty sky between them.

A style is written only when it gathers somewhere — `lift` of 8 or more — and
when at least 25 artists carry it. A style spread evenly across the layout has
no constellation to name, and writing its name over a patch of sky that is not
that style would be the map saying something false. Genres are always written.

## Versioning

The format version in the header is bumped only when the record layout
changes. Format 2 added the year; a reader of format 1 must refuse these tiles
rather than read them with the old stride, which would put every star after the
first one somewhere else.

Tiles belong to a layout; see
[Building the sky](/lyrid/guides/building-the-sky/) for how they are cut and
[ADR 0008](https://github.com/lacodda/lyrid/blob/main/docs/adr/0008-layout-written-here.md)
for why the layout is computed the way it is.
