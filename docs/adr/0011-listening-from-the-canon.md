# 11. Listening from the canon, not from an API

Date: 2026-08-30

## Status

Accepted.

## Context

The plan for this stage read: "Deezer/iTunes previews (the *scan*) plus an
official YouTube embed (the *landing*), ids from MusicBrainz URL relationships."
It assumed the canon carries track-level addresses. It does not.

Measured on the imported canon:

| What MusicBrainz links | Count |
| --- | --- |
| YouTube **channels** | 15,944 |
| YouTube links pointing at a **video** | **29** |
| Spotify **artist** pages | 43,757 |
| Deezer **artist** pages | 35,170 |
| Apple Music **artist** pages | 31,927 |

The relationship MusicBrainz records is artist-to-service. A recording-to-service
relationship exists in its schema but is not what the artist export carries, and
the ids a preview needs are not in any dump we import.

A thirty-second preview is reachable another way: `api.deezer.com/artist/{id}/top`
answers without a key and returns preview URLs. It was tried and it works. But
that is a rate-limited API on the path of every card, which
[ADR 0002](0002-universe-from-open-dumps.md) rules out — the universe is built
from dumps precisely so that no service being down or throttling can stop the
product working.

## Decision

**The card serves the links the canon already holds, and embeds the artist's own
channel where one exists.** No request leaves the server to build a card.

Three consequences, all deliberate:

- **Artist pages, not tracks.** The card says "Spotify", "Bandcamp", "Deezer" and
  sends the visitor there. It cannot say "play this song", because the canon does
  not know which song.
- **The channel plays in place.** Every YouTube channel has an implicit *uploads*
  playlist whose id is the channel id with `UC` swapped for `UU`. That playlist
  embeds in the standard player, so an artist's own channel plays inside the card
  with no Data API involved. 10,155 of 15,944 channels are the `/channel/UC…`
  form that carries an id; `/user/…` and `/@handle` links name a channel without
  giving its id, and resolving those needs the API this avoids — so they stay
  links rather than becoming a dead player.
- **The iframe loads on click.** A YouTube embed pulls scripts and sets cookies,
  and this card opens on every star a visitor touches. Until someone presses
  play, there is only a button, and a visitor who never presses it never meets
  YouTube at all. The embed uses `youtube-nocookie.com`.

Service names are read from the URL rather than passed through from MusicBrainz's
own vocabulary — a listener thinks in services, not in "free streaming" versus
"purchase for download". Matching is on the **registrable domain name**, because
Bandcamp gives every artist a subdomain (10,006 in the canon) and the shops run
national domains (`music.amazon.co.uk` and `music.amazon.com` are one shop). The
match is exact on that name rather than a substring test, so `notspotify.com`
does not read as Spotify. An unrecognised host keeps MusicBrainz's word for it,
which is a fair description even when it is not a name.

## Consequences

Nothing in the card can be broken by a streaming service, and the card works
against a stand with no internet beyond its own origin — which is what the rest
of the product already promises.

The *scan* and the *landing* from the product vision are not delivered by this
stage. They need track-level addresses, and getting those means either an API on
the critical path (refused here) or a source of track-to-service links that can
be imported in bulk. If such a source appears, this decision is worth revisiting:
the card's shape would not have to change, only what fills it.

Coverage, measured on the 100,000-artist slice: **49,090 artists have somewhere
to listen** and **14,198 have a YouTube channel**. Roughly half of the visible
sky, therefore, is silent — an artist with no links shows no listen block at all,
which is the same rule every other block on the card follows.
