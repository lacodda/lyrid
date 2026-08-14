# 0004 · Similarity from a published dataset, brightness from the graph

Date: 2026-08-14. Status: accepted.

## Context

The sky needs two numbers per artist that MusicBrainz does not carry: how
close artists are to each other, and how brightly each one should burn. ADR
0002 forbids rate-limited APIs in the critical path, so both must come from
data we can hold on disk.

Investigating what ListenBrainz actually publishes turned up a gap between the
documentation's framing and the files that exist:

- **Current similarity is API-only.** `similarity.artist` is a live database
  table served per request; there is no export path in the server source and
  no such table in any dump tree. Covering three million artists would take
  three million requests.
- **Per-artist listen counts are API-only too**, capped at the top 1,000
  artists sitewide — useless at the scale of the whole sky.
- **MLHD+** would give counts in bulk, but is licensed for non-commercial use
  only, and that restriction travels to everything derived from it.
- **One bulk similarity dataset does exist**: artist-credit-to-artist-credit
  relations, published 2020, CC0, ~7 million edges, keyed by MusicBrainz
  artist credit ids.

## Decision

**Similarity comes from the published 2020 relations dataset**, imported as a
named metric rather than as the similarity.

- `similarity_metric` names each set of edges with its corpus, method and
  vintage. Scores are comparable within a metric and never across them.
- Edges are stored once per unordered pair, with `source_id < target_id`
  enforced by a check constraint: the relation is symmetric, and two copies
  invite disagreement.
- Credits are resolved to artists through a mapping the MusicBrainz import now
  persists. Two credits of one artist collapsing onto one pair keep the
  strongest score; an edge from a star to itself is dropped.
- Importing a metric replaces only its own edges.

**Brightness is derived from the graph**, as `artist_prominence`: the number
of edges an artist has and the sum of their scores. The columns are named for
what they measure, so nothing can be mistaken for play counts.

## Consequences

- The graph reflects listening up to 2020. Artists who rose later arrive with
  no edges — which the design already accounts for as the "dark matter" at the
  map's margins, and which the vintage makes more visible rather than
  introducing.
- Musical similarity ages slowly, so most of the map is unaffected: the
  strongest relationships in the dataset are between artists whose closeness
  is not a function of this decade.
- The comparison that matters: a predecessor prototype spent a month querying
  rate-limited APIs to collect ~1,500 edges. This dataset yields roughly four
  thousand times that, in one pass over a 117 MB file.
- A freshly computed graph remains open and is now cheap to add: computing
  co-listening from the CC0 listen dumps lands as a second metric beside this
  one, with no schema change and no loss of the first.
- Prominence is honest about being connectivity rather than popularity. For a
  map this is arguably the better measure — it answers "how woven into the
  music is this artist", which is what position on a similarity map means.
