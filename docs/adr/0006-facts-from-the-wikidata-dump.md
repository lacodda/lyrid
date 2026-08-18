# 0006 · Facts and influence from the full Wikidata dump, streamed

Date: 2026-08-18. Status: accepted.

## Context

Two facts the sky needs exist in neither MusicBrainz nor Discogs:

- **Influence.** MusicBrainz has no "influenced by" relationship of any kind —
  its artist-artist vocabulary covers membership, collaboration and family, not
  lineage. Discogs has nothing comparable.
- **Where an act actually formed.** MusicBrainz records an area, which for most
  artists is a country. "Formed in Aberdeen" is a Wikidata fact (`P740`).

Wikidata is CC0, so it fits the canon's licensing line without the argument
that ADR 0005 had to settle. The obstacle is size: the full JSON dump is
**102.7 GB compressed**, and no smaller music-only dump exists. The options
were measured rather than assumed:

| Source | Size | Verdict |
| --- | --- | --- |
| `latest-all.json.bz2` | 102.7 GB | Everything, including sitelinks |
| `latest-truthy.nt.bz2` | 43.3 GB | All eight properties present, but **no sitelinks** |
| Query service (SPARQL) | — | Rate-limited; forbidden by ADR 0002 |

The truthy dump was checked directly: all eight needed properties appear in it,
but a scan for enwiki article links returned zero. Those titles are the only
bridge to the prose import, and recovering them by article name instead would
reintroduce name-matching — the very thing ADR 0005 refused for genres.

## Decision

**The full JSON dump is read, streamed straight from the network in one pass.**

- **Nothing is stored.** The importer decompresses from an HTTP response, so
  the 102.7 GB never lands on disk. Measured end to end against the live dump:
  400,000 entities in 9.5 minutes, or about 700 entities per second, which puts
  a full pass at **roughly ten hours** — a background job run once per dump
  vintage, not an interactive wait. (Raw transfer alone would suggest eight;
  parsing every entity is the rest.)
- **The canon is the filter, not the property.** Carrying a MusicBrainz id
  (`P434`) does not make an entity a musician: the first such entity in the
  dump is a 17th-century painter with music written about him. Only entities
  whose MBID already names an artist in the canon are kept.
- **Labels are captured during the same pass.** A fact is a reference, not a
  word — place of birth is `Q24826`, not "Liverpool" — and the item it points
  at may have gone past millions of lines earlier. Every English label is held
  in memory (about half a gigabyte, measured) and only the referenced ones are
  written. The alternative was a second ten-hour pass.
- **The enwiki title is taken now**, while the dump is open, because the prose
  import needs it and it exists nowhere else in the canon. Note that the dump's
  sitelinks carry `title` but no `url`, unlike the API's version of the same
  structure.
- **Influence is directed and canon-internal.** "X was influenced by Y" is not
  symmetric, unlike similarity, so both directions are not stored. An influence
  pointing outside the canon — at a painter, a novelist, a genre — is true and
  undrawable, so it is dropped and counted rather than stored dangling.
- **Wikidata genres stay separate from Discogs genres.** Discogs terms carry a
  release count behind them; Wikidata genres are editorial claims with no
  weight. Merging two vocabularies into one table is the mistake
  `similarity_metric` exists to prevent.

## Consequences

- Facts are as fresh as the dump vintage, and refreshing means another
  ten-hour pass. The `--limit` flag exists so the pipeline can be exercised
  against the real dump without paying that.
- `artist_fact.inception_year` can disagree with `artist.begin_year` from
  MusicBrainz. Both are kept: overwriting a curated value with a crowdsourced
  one silently would be worse than showing the disagreement.
- Influence coverage is thin by construction — an edge needs both ends in the
  canon and a Wikidata editor who recorded the claim. It is a garnish on the
  map, not a second similarity graph.
- Reading over the network means a broken connection loses the pass. Acceptable
  at one pass per vintage; a resumable reader would mean storing the file, which
  is exactly what this decision avoids.
