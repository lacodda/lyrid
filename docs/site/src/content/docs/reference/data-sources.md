---
title: Data sources
description: Every dump and dataset the universe is built from, what it contributes, and the licence it carries.
---

The universe is assembled locally from full open dumps. No rate-limited API sits in the critical path — the reasoning, and the failed prototype that produced it, are recorded in [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md).

| Source | What it contributes | Form |
| --- | --- | --- |
| **MusicBrainz** | Artists, release groups, memberships, countries, years, URL relationships | PostgreSQL dump |
| **ListenBrainz** | How many people listen to each artist and who is listened to by the same people; the older similarity of 2020; who is listened to each day; releases out around now; scrobbles | The statistics dump of each full export (CC0); the 2020 relations dataset (CC0); daily incremental listen dumps (CC0, Parquet); the fresh-releases list, once a day; user API for scrobbling |
| **Discogs** | Genres, styles and labels — the only genre source, see below | Monthly XML dumps (CC0) |
| **Wikidata** | Influence links, cities, biographical facts, Wikipedia article titles | Full JSON dump (CC0), streamed |
| **Wikipedia** | Prose extracts for artist cards | Multistream dump (CC BY-SA 4.0), reached by index |
| **AcousticBrainz** | Audio features — tempo, energy, mood and four more bands ("spectra") | Frozen 2022 dump (CC0) |

Every import pins the dump version it read, and re-running an import is idempotent.

## Attribution and licensing

- **Wikipedia extracts** are CC BY-SA 4.0. The attribution — title, URL, licence, dump version and revision id — is stored in the same row as the text, not attached at render time, so a snippet cannot travel through the system without it. See [Importing prose](/lyrid/guides/importing-prose/).
- **AcousticBrainz** data is CC0, as the dump's own `COPYING` says. The card still names it under every spectrum: a measurement should say who measured it. See [Importing spectra](/lyrid/guides/importing-spectra/).
- **MusicBrainz** data is used under its own terms; the schema keeps MBIDs so anything shown can be traced back.
- **YouTube** is embedded through the official player only. Video and channel ids come from MusicBrainz URL relationships, so the YouTube Data API is not used at all and no quota applies.
- **Previews** are the official 30-second clips from Deezer and iTunes. lyrid never streams full tracks itself.

## What is deliberately absent

**Spotify.** Its similarity and audio-features endpoints have been closed to new applications since late 2024, so building on them is not an option regardless of their quality.

**Any per-request public API in the hot path.** Unauthenticated limits are per IP, which means shared fate: anyone behind the same NAT can exhaust the quota for everyone. The universe must never be one stranger's traffic away from stalling.

**MLHD+.** A corpus of per-artist listen counts licensed for non-commercial use only, and that restriction travels to anything computed from it. It is not needed: ListenBrainz's own statistics dump is CC0 — see [Importing listeners](/lyrid/guides/importing-listeners/).

**MusicBrainz's own genre tags.** The same restriction, found the same way. MusicBrainz splits its licensing: core data is CC0, but `mbdump-derived.tar.bz2` — whose own `COPYING` file reads Attribution-NonCommercial-ShareAlike 3.0 US — holds "user submitted annotations, tags (including genre associations) and ratings". Genres feed the sky layout, the tiles and everything drawn from them, so a non-commercial input there would make every layer above it non-commercial too, with ShareAlike requiring the derived work back under the same terms. Genres come from the CC0 Discogs dumps instead, which are also deeper: two vocabularies, genre and style. See [ADR 0005](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md).

MusicBrainz has no genre-to-artist table at all, incidentally — genre there is a tag whose name matches a curated vocabulary entry — and no "influenced by" relationship of any kind. Influence links are a Wikidata fact (`P737`).

## What is not available as a dump

Worth knowing before planning against it, because the documentation reads as though it were:

- **Current artist similarity** as ListenBrainz computes it lives only behind its live API; the published relations dataset (2020, CC0) is the newest bulk form of that. lyrid computes its own instead, from the people who listen.
- **Per-artist listen counts** across all listeners are served by an API capped at the top 1,000 artists sitewide.

Both were closed without the API. Every full export carries a statistics dump
(CC0) whose first file lists each person's thousand most played artists: how
many people listen to each star, and who is listened to by the same people, are
both computed from it — see [Importing listeners](/lyrid/guides/importing-listeners/).
Since v0.19 the daily incremental dumps are read as they appear too: how many
people listened to each star, day by day, which is what rising stars are made
of — see [What changed in the sky](/lyrid/concepts/what-changed-in-the-sky/).

- **Ready-made Wikipedia lead extracts.** There is no longer any free dump of them. The Wikimedia Enterprise HTML dumps, which carried an `abstract` field per article, stopped being replicated to `dumps.wikimedia.org` on 24 March 2025, and `enwiki-latest-abstract.xml.gz` is absent from the current listings. The live REST summary endpoint is a rate-limited API, which ADR 0002 forbids. Extracts must therefore be parsed out of the article wikitext in the `pages-articles` dump — which the multistream index makes tractable, since it allows seeking to one article's stream instead of decompressing 27 GB.

- **A music-only Wikidata dump.** The full JSON dump is the only complete form (102.7 GB compressed). The "truthy" variant is 43.3 GB and does carry all the properties this product reads — but **no Wikipedia sitelinks**, checked directly, and those article titles are the only dump-only bridge to the prose. The query service is rate-limited and so ruled out. Filtering the full dump by the presence of a MusicBrainz id (`P434`) while streaming is the way in — and the 102.7 GB is read over the network without ever being stored, so its size is a matter of hours rather than of disk. See [Importing facts](/lyrid/guides/importing-facts/).
