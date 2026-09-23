# 15. The sky carries time and names; the pyramid is cut on its own

Date: 2026-09-23

## Status

Accepted.

## Context

v0.13 adds the observer's instruments: an era lens and a time machine that need
the year every star's act began, and names of genres and styles written over the
sky. ADR 0003 holds that browsing never touches the database — the map is static
tiles. Both features needed data the tiles did not carry.

Three questions came with them.

**Where the year lives.** A request per star, or per view, would be the map
reaching into the database on every pan, and a slider dragged through a century
would be a request per step. The tile record had no room: sixteen bytes, all
used.

**Where the names live.** Computed by the client from the stars it holds, a
name would be placed among the stars that happened to survive a level's
thinning, and the client does not know genres at all. Computed per view by the
server, it would again be the map asking the database.

**How a new format reaches browsers.** Tiles are cached for a day under paths
the next cut reuses. A client that
refuses format 1 tiles, meeting a cached format 1 tile, draws an empty sky for a
day.

Measuring the stand on the way turned up a fourth: its pyramid had been cut from
the full canon before `lyrid slice`, so it drew about twice as many stars as its
database knows — every star outside the slice drew and opened no card.

## Decision

**Tile format 2**: each record grows from 16 to 20 bytes, adding the begin year
(`i16`, `0` for unknown) and two reserved bytes to keep records aligned. The
year follows the card's rule — MusicBrainz first, Wikidata's inception where it
has none — and a year outside 1000–2100 is written as unknown. The client
refuses any other version.

**`labels.json` beside the tiles**, cut in the same pass: each genre and style
anchored at its densest 3×3 block of a grid, with its member count and its
*lift* — how much denser it is there than an even spread. Styles below a lift of
8 are not written: they are scattered, and a name over a patch that is not that
style is false.

**A stamp in `sky.json`**, the moment of cutting, put by the client into every
tile and label address as `?v=`. `sky.json` is never cached, so a new cut
reaches every visitor at once, and no cached file of an older cut is ever read
as this one.

**`lyrid tiles`** cuts the pyramid and the names from a stored layout. `lyrid
layout --tiles` calls the same code after writing positions. A stand's tiles are
cut after the slice, from the database the stand holds, and `stage-seed` empties
the old cut before unpacking a new one.

## Consequences

- Moving the time slider or turning the lens on costs a uniform per frame and
  nothing else; the measured frame budget of ADR 0009 is untouched by it.
- Tiles are a quarter larger per star (2.7 MB for the 100,000-star slice).
- A format change now ships with a recut and nothing more: no layout run, no
  cache to wait out.
- "Where am I" is *not* answered from the names: in the dense core a dozen
  anchors sit within a few units of each other. It is a vote of the stars in the
  middle of the view, asked of the server like the nearby list is
  (`/api/region`).
- The stand's pyramid now matches its database: every star drawn opens a card.
