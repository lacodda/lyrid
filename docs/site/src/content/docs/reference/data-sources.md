---
title: Data sources
description: Every dump and dataset the universe is built from, what it contributes, and the licence it carries.
---

The universe is assembled locally from full open dumps. No rate-limited API sits in the critical path — the reasoning, and the failed prototype that produced it, are recorded in [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md).

| Source | What it contributes | Form |
| --- | --- | --- |
| **MusicBrainz** | Artists, release groups, memberships, countries, years, URL relationships | PostgreSQL dump |
| **ListenBrainz** | Similarity from co-listening, popularity, trends; scrobbles | Open datasets + user API |
| **Discogs** | Genres and styles over MusicBrainz's sparse tags; labels and scenes | Monthly XML dumps |
| **Wikidata** | Influence links, cities, biographical facts | Dump / query service |
| **Wikipedia** | Prose extracts for artist cards | Dump |
| **AcousticBrainz** | Audio features — energy, tempo, mood ("spectra") | Frozen dump |

Every import pins the dump version it read, and re-running an import is idempotent.

## Attribution and licensing

- **Wikipedia extracts** are CC BY-SA. The attribution is stored alongside the text, not attached at render time, so a snippet cannot travel through the system without it.
- **MusicBrainz** data is used under its own terms; the schema keeps MBIDs so anything shown can be traced back.
- **YouTube** is embedded through the official player only. Video and channel ids come from MusicBrainz URL relationships, so the YouTube Data API is not used at all and no quota applies.
- **Previews** are the official 30-second clips from Deezer and iTunes. lyrid never streams full tracks itself.

## What is deliberately absent

**Spotify.** Its similarity and audio-features endpoints have been closed to new applications since late 2024, so building on them is not an option regardless of their quality.

**Any per-request public API in the hot path.** Unauthenticated limits are per IP, which means shared fate: anyone behind the same NAT can exhaust the quota for everyone. The universe must never be one stranger's traffic away from stalling.
