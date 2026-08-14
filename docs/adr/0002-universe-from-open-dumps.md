# 0002 · The universe is assembled from open dumps

Date: 2026-08-13. Status: accepted.

## Context

A predecessor prototype grew its similarity graph by querying rate-limited public APIs (last.fm, Deezer) one edge at a time: a month of hydration produced ~1.5k edges and the growth rate was capped by other people's throttles. Unauthenticated APIs are also shared-fate: rate limits are per IP, so anyone behind the same NAT can exhaust them for everyone.

## Decision

The universe is built **locally, from full open dumps and datasets**; rate-limited APIs are forbidden in the critical path.

- **MusicBrainz** (Postgres dump): artists, release groups, membership links, countries, years, URL relationships.
- **ListenBrainz** (open datasets): artist similarity from co-listening, popularity, trends.
- **Discogs** (monthly XML dumps): genres/styles as a second layer over MusicBrainz's sparse tags; labels and scenes.
- **Wikidata**: influence links, cities, biographical facts.
- **Wikipedia**: prose extracts, shown with CC BY-SA attribution.
- **AcousticBrainz** (frozen dump): audio features ("spectra": energy, tempo, mood).

Similarity and the sky layout are computed by lyrid's own pipelines. Listening is layered and fully licensed: Deezer/iTunes 30-second previews ("scan"), official YouTube embeds ("landing" — video/channel ids come from MusicBrainz URL relationships, so the YouTube Data API is not needed at all), and ListenBrainz scrobbling as the engine of real-listening progress. Spotify's API is rejected (similarity and audio-features endpoints closed to new apps since late 2024).

## Consequences

- Import pipelines must be reproducible and incremental; every dump version is pinned and recorded.
- The long tail of artists with no similarity and no previews is not a defect — it is the "dark matter" game mechanic.
- No external quota can stall the product; universe growth is limited only by our own pipeline runs.
