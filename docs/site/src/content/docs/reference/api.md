---
title: HTTP API
description: What the browser asks the server for — and what it deliberately does not.
---

The API is thin on purpose. **Browsing the sky never touches it**: the map is
static tile files, so panning and zooming produce no requests at all beyond
fetching tiles. These endpoints serve what a click and a search box need.

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

## `GET /api/artists/{id}`

Everything a card shows. This is where the import pipelines meet — the name
and years from MusicBrainz, the genres from Discogs with a release count
behind each, the neighbours from co-listening.

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
  "similar": [{ "id": 1289, "name": "Sonic Youth", "score": 0.0114 }]
}
```

`404` for an unknown id; a non-numeric id is a `400` from routing and never
reaches the database.

**`position` comes from the newest layout.** An older one would place the star
somewhere the map does not draw it. `brightness` there is the raw graph weight
— connectivity, not popularity, since no listen counts are published as a dump
([ADR 0004](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)).
The tiles carry a normalised version of the same number.

`genres` are ordered by their release count, which is what makes a genre a
claim rather than a label: "Rock 314" says more than "Rock".

`similar` reads both columns of `artist_similarity`, since a pair is stored
once.

## What is deliberately absent

- **No endpoint returns the sky.** Positions are in tiles; an API that served
  200,000 stars per pan would defeat the architecture
  ([ADR 0003](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md)).
- **No write endpoints yet.** Accounts, the fog of war and everything per-user
  arrive with the profiles milestone.
- **No pagination on search.** Twelve hits is a search box, not a catalogue;
  browsing the canon is what the map is for.
