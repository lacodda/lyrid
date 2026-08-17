---
title: Data sources
description: Every dump and dataset the universe is built from, what it contributes, and the licence it carries.
---

The universe is assembled locally from full open dumps. No rate-limited API sits in the critical path — the reasoning, and the failed prototype that produced it, are recorded in [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md).

| Source | What it contributes | Form |
| --- | --- | --- |
| **MusicBrainz** | Artists, release groups, memberships, countries, years, URL relationships | PostgreSQL dump |
| **ListenBrainz** | Artist similarity from co-listening; scrobbles | Published relations dataset (CC0) + user API for scrobbling |
| **Discogs** | Genres, styles and labels — the only genre source, see below | Monthly XML dumps (CC0) |
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

**MLHD+.** The one corpus that would give per-artist listen counts directly is licensed for non-commercial use only, and that restriction travels to anything computed from it. Brightness is derived from the similarity graph instead — see [Importing similarity](/lyrid/guides/importing-similarity/).

**MusicBrainz's own genre tags.** The same restriction, found the same way. MusicBrainz splits its licensing: core data is CC0, but `mbdump-derived.tar.bz2` — whose own `COPYING` file reads Attribution-NonCommercial-ShareAlike 3.0 US — holds "user submitted annotations, tags (including genre associations) and ratings". Genres feed the sky layout, the tiles and everything drawn from them, so a non-commercial input there would make every layer above it non-commercial too, with ShareAlike requiring the derived work back under the same terms. Genres come from the CC0 Discogs dumps instead, which are also deeper: two vocabularies, genre and style. See [ADR 0005](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md).

MusicBrainz has no genre-to-artist table at all, incidentally — genre there is a tag whose name matches a curated vocabulary entry — and no "influenced by" relationship of any kind. Influence links are a Wikidata fact (`P737`).

## What is not available as a dump

Worth knowing before planning against it, because the documentation reads as though it were:

- **Current artist similarity** lives only behind ListenBrainz's live API. The published relations dataset (2020, CC0) is the newest bulk form of it.
- **Per-artist listen counts** are served by an API capped at the top 1,000 artists sitewide, or by MLHD+ under its non-commercial licence. Neither can fill a three-million-star sky.

Both gaps are closable by computing from the CC0 listen dumps, which is a stage of its own rather than an import.

- **Ready-made Wikipedia lead extracts.** There is no longer any free dump of them. The Wikimedia Enterprise HTML dumps, which carried an `abstract` field per article, stopped being replicated to `dumps.wikimedia.org` on 24 March 2025, and `enwiki-latest-abstract.xml.gz` is absent from the current listings. The live REST summary endpoint is a rate-limited API, which ADR 0002 forbids. Extracts must therefore be parsed out of the article wikitext in the `pages-articles` dump — which the multistream index makes tractable, since it allows seeking to one article's stream instead of decompressing 27 GB.

- **A music-only Wikidata dump.** The full JSON dump is the only complete form (102.7 GB compressed); the "truthy" variant is 43.3 GB and drops qualifiers. The query service is rate-limited and so ruled out. Filtering the full dump by the presence of a MusicBrainz id (`P434`) while streaming is the way in.
