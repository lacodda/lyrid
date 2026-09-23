---
title: The observer's instruments
description: Seeing the sky through time, reading why two stars are near, comparing any two, and walking a route.
---

The sky shows *where* music sits. The instruments show what the map cannot say
by position alone: when each act appeared, why two stars are neighbours, and how
two sounds differ.

## Names on the sky

From far away, the fifteen Discogs **genres** are written over the sky as
continents; closer in, the **styles** inside them — Soul, Doo Wop, Big Band. A
name sits at its densest place, not at the average of everyone who carries it,
and a style that is scattered over the whole layout gets no name at all: writing
"Techno" over a patch that is not techno would be the map saying something
false. Styles appear only at the zoom where every star is drawn, so a name
always has its stars beneath it.

Names are part of the map and are cut with the tiles — see
[`labels.json`](/lyrid/reference/tile-format/#labelsjson).

## The era lens

Colours every star by the era its act began: before 1960, then each decade to
the 2010s and since, amber for the oldest through to pale azure. Seven steps and
not thirteen decades, because time is ordered and an ordered palette only reads
when neighbouring steps differ visibly — measured against the sky's own
background, thirteen steps are indistinguishable. A star whose year the canon
does not know is grey. The legend names every era in words beside its colour.

## The time machine

Shows the sky as it stood in a year. A star is not there before its act began,
flares up in its first years, then settles to its ordinary light. Undated stars
stay faint in every year rather than vanishing or claiming all of them. On the
slice a stand carries, about half the stars have a year.

Both instruments are drawn by the GPU from a year carried in every tile, so
moving the slider costs no request and no new tiles.

## Why nearby

Every neighbour on a card says why it is one, in the canon's words: the styles
and genres both carry (each at least a tenth of *each* discography), and any
influence between them with its direction. A line between two stars is a
number; these are what make it knowledge.

## The spectrograph

"Compare with…" holds a star; opening another offers to compare the two. The
comparison lines up their genres as shares of each one's own work, says what
joins them, and lists the stars both are listened alongside — the common
ground on the map.

## Where am I

The compass draws the whole sky small with the view marked on it, and names the
region the middle of the view is in — the style most of the stars there carry
as their main one. It is also the keyboard's way of moving the sky: arrows pan,
`+` and `-` zoom, Home shows everything. On a phone's width the compass and the
instruments are hidden for now; the sky there needs a layout of its own.

## Routes

"Add to route" puts a star at the end of a route; the route is drawn as a line
through its stops and walked with back and next. The route is its address:

```
/route/962,132,54#-5.5,7.09,8
 └ the stops, in order   └ the camera, as always
```

Sending the link sends the whole walk. A route holds up to fifty stops and may
come back to a star; a link with a garbled stop opens no route rather than a
different one. Routes are the foundation of playlists-as-routes later on.
