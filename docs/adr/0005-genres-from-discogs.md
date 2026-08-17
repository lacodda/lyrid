# 0005 · Genres come from Discogs, not from MusicBrainz tags

Date: 2026-08-17. Status: accepted.

## Context

MusicBrainz is the canon's backbone, so its own genre data was the obvious
first choice. Checking what that data actually is, and what it costs, turned up
two problems — the second decisive.

**MusicBrainz has no genre-to-artist table.** Genre is modelled as a
folksonomy tag: `genre` is a curated vocabulary of names, and an artist is
"in" a genre when `artist_tag` points at a `tag` whose name matches one. Tag
counts are net scores — upvotes minus downvotes — so they can be zero or
negative.

**The tag tables are not in the core archive, and not under CC0.** They ship
in `mbdump-derived.tar.bz2`, whose own `COPYING` file is
Attribution-NonCommercial-ShareAlike 3.0 US. MusicBrainz splits its licensing
deliberately: core data is CC0, while "user submitted annotations, tags
(including genre associations) and ratings" are the non-commercial half.

That is the same restriction that ruled out MLHD+ for brightness in ADR 0004,
for the same reason: it travels to everything computed from it. Genres feed the
sky layout, which feeds the tiles, which feed poster export and a possible
read-only canon API. A non-commercial input at the bottom of that stack makes
every layer above it non-commercial too, and ShareAlike would require handing
the derived work back under the same terms.

Discogs publishes the alternative under CC0 — a formal public-domain
dedication, stated on the dump index page itself. Its monthly XML dumps carry
both a genre and a finer style vocabulary, which is deeper than MusicBrainz's
single flat list.

## Decision

**Genres and styles come from the Discogs monthly dumps.** MusicBrainz tags are
not imported, and `mbdump-derived.tar.bz2` is not downloaded at all. The canon
stays CC0 end to end.

- Discogs attaches genres to **releases**, never to artists, so an artist's
  genres are aggregated from their discography, keeping a **count of releases**
  per genre. The count is what makes the aggregate honest: Nirvana comes out
  Rock 314 / Grunge 303 / Alternative Rock 113, with the remix and interview
  tail at 1–3, and any consumer can threshold on weight instead of treating
  "has genre" as a boolean.
- Genres and styles share one table, separated by `is_style`. They are the same
  kind of fact at two depths ("Electronic" and "Techno"), and splitting them
  would double every query the map makes.
- **Artists are joined through MusicBrainz's own `discogs` artist-URL
  relationship**, which the MusicBrainz import already stores. Never by name:
  Discogs disambiguates with numeric suffixes ("Jack Jones (4)") precisely
  because names collide, and a wrong join would put someone else's genres on a
  star. URLs pointing at label or release pages under the same relationship
  kind are ignored.
- The 10.4 GB `releases` file is not read. A master release carries the same
  genres as its pressings; the pressings add ten gigabytes to repeat them.

## Consequences

- Genre coverage is Discogs coverage of artists that MusicBrainz links to
  Discogs. An artist with no Discogs link gets no genres — more dark matter at
  the margins, which the design already accounts for.
- Genre names are Discogs's vocabulary, not MusicBrainz's. Anything comparing
  the two later must treat them as different vocabularies, as with similarity
  metrics.
- Aggregating over releases weights prolific artists' reissues. This is visible
  rather than hidden: the count is stored, so a consumer can normalise.
- The canon carries no non-commercial obligation, which keeps the read-only
  canon API and community quest packs open as future options.
- Wikipedia prose remains CC BY-SA and will be handled separately, with
  attribution stored alongside the text rather than attached at render time.
  Attribution is a per-snippet obligation, not a licence on the whole canon,
  which is why it does not collide with this decision.
