---
title: HTTP API
description: What the browser asks the server for — and what it deliberately does not.
---

The API is thin on purpose. **Browsing the sky barely touches it**: the map is
static tile files, so panning and zooming fetch tiles and nothing else — with
one exception, the list of stars in view beside the canvas, which asks
`/api/nearby` once per settled view because the tiles carry no names. Otherwise
these endpoints serve what a click and a search box need.

## `GET /health`

Liveness and readiness in one place.

```json
{ "status": "ok", "version": "0.6.0", "database": "ok" }
```

Answers `503` with `"status": "degraded"` when the database is unreachable —
the process is alive but cannot do its job, and the two cases are worth
telling apart.

## `GET /api/search?q=…`

Finds stars by name. Terms shorter than two characters answer `[]` without
querying: one letter would match a large share of three million artists.

```json
[
  { "id": 54, "name": "Nirvana", "comment": "1980s–1990s US grunge band", "x": -59.2, "y": -69.5 },
  { "id": 2612, "name": "Nirvana", "comment": "60s band from the UK", "x": 193.4, "y": 79.1 }
]
```

Two decisions worth knowing, both of them corrections to an obvious first
attempt:

**Matching is by substring, not by prefix.** A prefix search for `beatles`
finds a band called "Beatless" and misses The Beatles entirely, because their
name starts with an article.

**Ranking puts connectivity above where the match falls.** An exact name wins
first; after that the most woven-into-the-graph artist leads. Otherwise
`beatles` still leads with "Beatless" — the word does start its name — and
`nirvana` leads with a sitar ensemble.

Only artists that **have a position** are returned: a result the map cannot
fly to is a dead end.

## `GET /api/nearby?min_x=…&min_y=…&max_x=…&max_y=…`

The named stars inside a rectangle of the layout, most prominent first, twelve
at most. All four bounds are required: defaulting a missing edge to zero would
answer a rectangle nobody asked about, and the answer would look plausible.

```json
[
  { "id": 54, "name": "Nirvana", "comment": "1980s–1990s US grunge band", "x": -59.2, "y": -69.5 },
  { "id": 12389, "name": "Red Hot Chili Peppers", "comment": null, "x": -52.8, "y": -71.3 }
]
```

This exists so the sky can be **read by a keyboard and by a screen reader**. A
WebGL canvas is one element with no children: to anything that is not a pointer
aimed at a few pixels of light, it is a blank rectangle. The SPA draws the
answer to this as a list beside the canvas, and that list is the only reachable
path to a star for a reader who is not using a mouse.

It is a question the server has to answer rather than the client. The tiles the
canvas draws from carry ids and positions but **no names**, and at a wide zoom
they are a thinned sample rather than everything in view — so a list built in
the browser would be the stars that happened to survive thinning, named by one
request each.

Two details of the ordering:

**By prominence, not by distance from the middle.** A reader asking what they
are looking at wants the names worth knowing in this patch of sky; the star
nearest the exact centre of an arbitrary pan is nobody in particular.

**Bounds given the wrong way round are normalised, not answered empty.**
`BETWEEN 5 AND -5` matches nothing in Postgres, so a viewport sent backwards
would report an empty patch of sky — which reads as "no stars here" rather
than as "you asked wrongly".

Positions belong to a layout, and this reads the newest one, the same way the
card's own position query does.

## `GET /api/artists/{id}`

Everything a card shows. This is where the import pipelines meet — the name
and years from MusicBrainz, the lead paragraphs from Wikipedia, origin and
influence from Wikidata, the genres from Discogs with a release count behind
each, the neighbours from co-listening.

```json
{
  "id": 54,
  "mbid": "5b11f4ce-a62d-471e-81fc-a69a8278c7da",
  "name": "Nirvana",
  "comment": "1980s–1990s US grunge band",
  "kind": "Group",
  "area": "United States",
  "begin_year": 1987,
  "end_year": 1994,
  "position": { "x": -59.2, "y": -69.5, "brightness": 1.1 },
  "genres": [
    { "name": "Rock", "is_style": false, "releases": 314 },
    { "name": "Grunge", "is_style": true, "releases": 303 }
  ],
  "similar": [
    {
      "id": 12389,
      "name": "Red Hot Chili Peppers",
      "score": 0.0141,
      "why": { "genres": ["Alternative Rock", "Rock"], "co_listening": 0.0141, "influence": null }
    }
  ],
  "origin": {
    "place": "Aberdeen",
    "country": "United States",
    "is_birth": false,
    "inception_year": 1987
  },
  "labels": ["DGC Records", "Geffen Records", "Sub Pop"],
  "influenced_by": [{ "id": 1419, "name": "Black Sabbath", "score": 6.44 }],
  "influenced": [{ "id": 12068, "name": "Nickelback", "score": 11.05 }],
  "prose": {
    "extract": "Nirvana was an American rock band formed in Aberdeen, Washington, in 1987.…",
    "source_title": "Nirvana (band)",
    "source_url": "https://en.wikipedia.org/wiki/Nirvana_(band)",
    "licence": "CC BY-SA 4.0"
  },
  "releases": [{ "name": "Bleach", "primary_type": "Album", "year": 1989 }],
  "listen": [
    { "service": "YouTube", "url": "https://www.youtube.com/channel/UCzGrGrvf9g8CVVzh_LvGf-g" },
    { "service": "Spotify", "url": "https://open.spotify.com/artist/6olE6TJLqED3rqDCT0FyPh" },
    { "service": "Bandcamp", "url": "https://nirvana.bandcamp.com/" }
  ],
  "youtube_uploads": "UUzGrGrvf9g8CVVzh_LvGf-g"
}
```

`404` for an unknown id; a non-numeric id is a `400` from routing and never
reaches the database.

### Why a neighbour is one

Each entry in `similar` carries `why`: the reasons the canon gives for the edge,
computed by the same code as [`/api/compare`](#get-apicomparea-b), so a card and
a comparison never explain one pair two ways.

| Field | Meaning |
| --- | --- |
| `genres` | Styles, then genres, that **both** carry — each at least a tenth of *each* discography. Three at most, most shared first |
| `co_listening` | The edge's score, when the two are listened to together |
| `influence` | `shaped_by` (the neighbour shaped this artist), `went_on_to_shape`, `mutual`, or `null` |

The tenth is a floor measured against a wrong answer: counted by releases,
Marvin Gaye and Nirvana came out as "both Electronic, Stage & Screen, Electro"
— one remix and one soundtrack each. A shared genre has to be part of what both
artists are.

## `GET /api/compare?a=…&b=…`

Two stars side by side: what joins them, their genres as shares of each one's
own discography, and the stars both are listened alongside.

```json
{
  "why": { "genres": ["Soul", "Rhythm & Blues", "Funk / Soul"], "co_listening": 0.388, "influence": null },
  "spectrum": [
    { "name": "Funk / Soul", "is_style": false, "a": 0.811, "b": 0.832 },
    { "name": "Pop", "is_style": false, "a": 0.073, "b": 0.098 }
  ],
  "shared_neighbours": [{ "id": 12783, "name": "Four Tops", "score": 0.272 }]
}
```

**Shares, not counts**: a prolific act and a sparse one are compared by what
their work is, not by how much of it there is. Genres and styles are shared out
separately — Discogs tags a release with both, and summing across the two would
count one record twice. Up to six genre bands and eight style bands, the ones
that matter most to either star; a band one star is all about and the other has
none of is kept, because that difference is what a comparison is for.

A shared neighbour's `score` is the weaker of its two edges. `404` when either
star is unknown; `400` for a star compared with itself.

## `GET /api/stars?ids=…`

Several stars by id — up to 50, the same bound a route in the address has —
with their places, in the order asked:

```json
[
  { "id": 962, "name": "Marvin Gaye", "comment": null, "x": -5.50, "y": 7.09 },
  { "id": 132, "name": "The Temptations", "comment": "Motown soul vocal group", "x": -2.43, "y": 15.49 }
]
```

The order is kept because a route's order is its meaning. An id the canon does
not know is left out rather than failing the rest; a list with anything that is
not a positive id is a `400` whole — one garbled stop is a garbled address, not
a shorter route.

## `GET /api/region?min_x=…&min_y=…&max_x=…&max_y=…`

What a patch of sky is: the commonest **main** style and genre among the 300
most prominent stars in the rectangle. The compass asks it about the middle half
of the view.

```json
{ "style": "House", "genre": "Electronic" }
```

Answered by the stars, not by the names written on the map: in the dense core a
dozen name anchors sit within a few units of each other, and "the nearest name"
there is a coin toss. Same rectangle rules as `/api/nearby`.

**`position` comes from the newest layout.** An older one would place the star
somewhere the map does not draw it. `brightness` there is the raw graph weight
— connectivity, not popularity, since no listen counts are published as a dump
([ADR 0004](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)).
The tiles carry a normalised version of the same number.

`genres` are ordered by their release count, which is what makes a genre a
claim rather than a label: "Rock 314" says more than "Rock".

`similar` reads both columns of `artist_similarity`, since a pair is stored
once. Both it and the influence lists pin the newest similarity metric: an
edge is keyed by `(metric_id, source_id, target_id)`, so joining without one
would list the same neighbour once per metric — on scores that are not
comparable across metrics anyway.

**`prose` is never served without its attribution.** The extract and the
credit are one row in the database and one object here, so no response can
carry the words alone. Wikipedia leads arrive under CC BY-SA, and the licence
travels with them to the client, which renders both together or neither
([ADR 0007](https://github.com/lacodda/lyrid/blob/main/docs/adr/0007-prose-parsed-from-wikitext.md)).

`origin` answers a different question from `area`: Wikidata records a city,
MusicBrainz a country. `is_birth` says which claim it is — a person's
birthplace or a group's place of formation — because "born in Seattle" and
"formed in Seattle" are not the same statement. `inception_year` sits beside
MusicBrainz's `begin_year` rather than replacing it: the two can disagree, and
a curated value is not silently overwritten by a crowdsourced one.

`influenced_by` and `influenced` are separate lists because influence is
directed. Merging them would assert a symmetry Wikidata never claimed.

`releases` are ordered albums first and then oldest first, which approximates
"the records this artist is known for" from the only fields the canon holds.
Newest-first looks obvious and is wrong: the Beatles carry 696 release groups
typed `Album`, nearly all reissues, so the newest dozen are recent repackagings
and the famous records are nowhere. The honest limit is that MusicBrainz marks
a live album or a compilation through *secondary* types, which the import does
not read yet, so concert recordings typed `Album` still appear among the studio
records.

Most artists have neither prose nor influence links: in a canon of three
million, an encyclopaedia article is the exception. Those fields are `null` or
empty rather than absent, and the card hides the block.

**`listen` comes from the canon, not from a streaming API.** MusicBrainz
records where an artist can be found, and those relationships are already
imported — so building this card sends no request to Spotify or Deezer, which
is what [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md)
requires of the critical path. The honest consequence is on the card: these
are **artist pages, not tracks**, because MusicBrainz relates an artist to a
service and not a recording to one — only 29 of its 15,944 YouTube links point
at a video. A per-track preview would need an API call per star; the reasoning
and what would change that is in
[ADR 0011](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md).

The service name is read from the URL rather than passed through from
MusicBrainz's own vocabulary ("free streaming", "purchase for download"),
because a listener thinks in services. Matching is on the registrable domain
name: Bandcamp gives every artist a subdomain (10,006 in the canon) and the
shops run national domains, so `music.amazon.co.uk` and `music.amazon.com` are
one shop. A host the list does not know keeps MusicBrainz's word for it. The
list is capped at eight: a median artist has four of these, 90% have nine or
fewer, and the tail reaches 53.

**`youtube_uploads` is a playlist id, not a channel URL.** Every channel has an
implicit "uploads" playlist whose id is the channel id with `UC` swapped for
`UU`, and that playlist embeds in the standard player — so the card can play an
artist's own channel without the YouTube Data API. Only the `/channel/UC…` link
form carries the id; `/user/…` and `/@handle` name a channel without giving it,
and resolving those needs the very API this avoids. Measured: **10,155 of
15,944 channels** are the embeddable form, and 49,090 of 100,000 placed artists
in the slice have somewhere to listen at all.

Measured on the full canon: of the **206,636 artists with a place in the sky**,
**77,616 (38%) carry Wikidata facts** and **55,738 (27%) carry prose**. The
`score` on an influence is the neighbour's graph weight, which is why the
better-known names lead the list — it is not a strength of influence, which
Wikidata does not record.

## Usage metrics

`POST /api/metrics/{mechanic}` counts one use of a mechanic. It takes no body
and answers `204`.

```
POST /api/metrics/sky_opened
204 No Content
```

The mechanic is one of a closed list the server holds, not free text from the
client:

```
sky_opened      card_opened     listen_opened
view_shared     charter_read    data_requested
lens_used       time_travelled  stars_compared
route_opened
```

A name outside the list answers `404`. The list is what stops a client
writing something that identifies a person — a search term, an artist id —
into the table; a check that only polite clients honour is not a promise.

**Storage is a counter incremented in place, keyed by `(mechanic, day)`, not a
row per event.** There is nothing to join to an account, no order to read a
session out of, and nothing that becomes personal later when a clever enough
query is written against it. A table of events with the user id left out is
still a table of events.

The accepted consequence: these numbers can answer "how many times was the
radio opened today" and can never answer "did the people who opened the radio
come back". That second question is the one the [privacy
charter](/lyrid/reference/accounts/#the-privacy-charter-export-and-delete)
does not let this service ask.

A counter that fails to write is logged and still answers `204` — a counter
is not worth failing a page over, and the visitor is here to look at the sky,
not to retry a beacon. For the same reason, [deleting an
account](/lyrid/reference/accounts/#the-privacy-charter-export-and-delete)
leaves the counters untouched: they hold no row about that person to remove.

## What is deliberately absent

- **No endpoint returns the sky.** Positions are in tiles; an API that served
  200,000 stars per pan would defeat the architecture
  ([ADR 0003](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md)).
- **Nothing per-user on these routes.** The sky, the card and the search never
  ask who is asking, and they answer the same for a visitor as for a
  signed-in account. Accounts and the profile have
  [their own endpoints](/lyrid/reference/accounts/); the fog of war and the
  rest of the game arrive with their own versions.
- **No pagination on search.** Twelve hits is a search box, not a catalogue;
  browsing the canon is what the map is for.
- **No track previews.** A thirty-second preview needs a per-track lookup
  against a streaming API, and no dump carries the track ids to avoid one.
  What the canon holds is artist pages, so that is what `listen` serves; the
  channel embed is as close to playing as this can get without an API in the
  critical path.
