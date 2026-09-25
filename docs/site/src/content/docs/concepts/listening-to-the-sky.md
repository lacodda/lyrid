---
title: Listening to the sky
description: The one player, the radio of a nebula, and the signal of the day.
---

The sky shows where music sits; listening is how you find out what it sounds
like. Everything plays through **one player**, in the corner under the card, and
keeps playing while you fly somewhere else — close the card, search for another
star, open a third, and the music carries on.

What plays is always an artist's own YouTube channel, embedded. The canon links
an artist to a channel, not a song to a service, so the unit here is a star, not
a track — see
[ADR 0011](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md).

## A star's channel

A card with a channel offers to play it. The channel plays through its uploads,
newest first, as long as you leave it on.

## The radio of a nebula

A radio plays the stars of one genre or style, one song each, one star after
another. It is started two ways:

- **From a card** — the radio of that star's nebula.
- **From the player's corner** — the radio of the place the view is looking at,
  the same place the compass names.

The nebula is the stars that carry it as their **main** genre or style — the one
most of their releases carry — which is exactly who is written under that name
on the sky. A style is offered when its radio has at least eight stars to play;
below that it would loop two or three channels, and its genre is offered
instead.

The order is shuffled for every radio, with bright stars coming up more often
than faint ones — the same brightness the sky draws — and the faint ones still
on the air. When a queue of fifty runs out, a fresh one follows without a gap,
without bringing back what has just played.

**The view goes along only if you do.** If the card open is the star that is
playing, the next song opens the next star and flies there. If you have gone off
to look at something else, the radio changes the song and leaves the view
alone.

The minimap marks the star that is playing, so you can see where the sound comes
from.

## The signal of the day

One faint star, somewhere in the dark — a star the wide sky does not draw, that
only shows once you have zoomed in. Its name is hidden: you hear it first, the
minimap shows where it is, and you find out who it is by going there — press
**Go to the signal**, or fly there yourself and open it.

It is the same signal for everyone on the same day, so it can be talked about,
and it turns over at your own midnight.

## When a channel will not play

About a third of the channels in the canon do not play in an embedded player,
and they say nothing — no error, just a black box. The player gives a channel six
seconds:

- a **radio** skips it and goes on to the next star;
- the **signal** moves to the next star of the day's list — the same list for
  everyone, so it stays one signal;
- a **card's channel** says it does not play outside YouTube; the link to it is
  on the card.

If eight stars in a row stay silent, the radio stops and says it has lost
YouTube, rather than skipping through its whole queue.

The player loads YouTube's script only when you first press play. A visitor who
never listens never meets YouTube.
