-- Facts and influence, from Wikidata.
--
-- Wikidata is the only bulk source for two things the sky needs and neither
-- MusicBrainz nor Discogs carries: who influenced whom, and where an act
-- actually came from. MusicBrainz has no "influenced by" relationship at all,
-- and its area is a country rather than a city.
--
-- The dump is CC0, like the rest of the canon. See ADR 0006.

-- Which Wikidata item a canonical artist is.
--
-- The link comes from MusicBrainz's own `wikidata` artist-URL relationship,
-- already stored in artist_url by the MusicBrainz import -- and it is
-- confirmed from the other side too: the item's P434 must hold that artist's
-- MBID. Two independent statements agreeing is what makes this join safe
-- enough to hang biography on.
CREATE TABLE artist_wikidata (
    artist_id integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,
    -- The Q-number without its prefix: Q11649 is stored as 11649.
    qid       integer NOT NULL,
    -- The English Wikipedia article title from the item's sitelinks, kept
    -- verbatim ("Nirvana (band)"). This is the address the prose import needs,
    -- and it exists nowhere else in the canon -- so it is stored now, while
    -- the dump is open, rather than paid for with a second 100 GB pass.
    enwiki_title text
);

CREATE INDEX artist_wikidata_qid_idx ON artist_wikidata (qid);
CREATE INDEX artist_wikidata_enwiki_idx ON artist_wikidata (enwiki_title) WHERE enwiki_title IS NOT NULL;

-- Names for the items facts point at.
--
-- A Wikidata fact is a reference, not a word: place of birth is "Q24826", not
-- "Liverpool". Resolving those references would mean a second pass over 100 GB,
-- so instead every referenced item's label is captured during the same pass --
-- the dump visits every entity anyway, and an item that is somebody's
-- birthplace is itself an entity in the file.
CREATE TABLE wikidata_item (
    qid   integer PRIMARY KEY,
    -- The English label. NULL where the item has none, which happens.
    label text
);

-- Biographical facts, one row per artist.
--
-- Denormalised onto one row rather than an entity-attribute-value table: these
-- are a fixed handful of fields the artist card reads together, and a generic
-- fact table would make the card's query a self-join per field.
CREATE TABLE artist_fact (
    artist_id       integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,
    -- Where the act comes from: P740 (location of formation) for groups,
    -- P19 (place of birth) for people. Which of the two answered is recorded,
    -- because "formed in Seattle" and "born in Seattle" are different claims.
    origin_qid      integer,
    origin_is_birth boolean,
    -- P571: inception. MusicBrainz already has begin_year, and the two can
    -- disagree; both are kept so the card can show the better-sourced one
    -- rather than silently overwriting a curated value with a crowdsourced one.
    inception_year  smallint,
    -- P495: country of origin.
    country_qid     integer
);

-- Genres as Wikidata sees them (P136), kept apart from the Discogs vocabulary.
--
-- Not merged into artist_genre: those are Discogs terms with a release count
-- behind them, these are editorial claims with no weight. Mixing two
-- vocabularies in one table is exactly the mistake similarity_metric exists to
-- prevent.
CREATE TABLE artist_wikidata_genre (
    artist_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    genre_qid integer NOT NULL,
    PRIMARY KEY (artist_id, genre_qid)
);

-- Record labels an artist has been signed to (P264).
CREATE TABLE artist_wikidata_label (
    artist_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    label_qid integer NOT NULL,
    PRIMARY KEY (artist_id, label_qid)
);

-- Influence: the currents of the map (P737).
--
-- Directed, unlike similarity: "X was influenced by Y" does not mean Y was
-- influenced by X, and the arrow is the whole point of the fact. Both ends
-- must be artists in the canon -- an influence pointing at a painter is true
-- but undrawable, so it is dropped rather than stored as a dangling id.
CREATE TABLE artist_influence (
    -- The artist who was influenced.
    artist_id    integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    -- The one who influenced them.
    influence_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    PRIMARY KEY (artist_id, influence_id),
    -- Wikidata does contain self-influence typos; they are not currents.
    CONSTRAINT artist_influence_not_self CHECK (artist_id <> influence_id)
);

-- "Who did this artist influence" is the more interesting direction on a card,
-- and it reads the second column.
CREATE INDEX artist_influence_reverse_idx ON artist_influence (influence_id);
