-- Which artist a MusicBrainz artist_credit resolves to.
--
-- Kept because other datasets key on artist_credit rather than on the artist:
-- ListenBrainz's similarity dump is artist_credit-to-artist_credit, and
-- resolving it needs this mapping long after the MusicBrainz import has
-- finished. Holding it only in the importer's memory made the credit ids
-- unusable by every later pipeline.
CREATE TABLE artist_credit (
    -- MusicBrainz's artist_credit.id.
    id        integer PRIMARY KEY,
    -- The first credited artist: a credit may name several, and the sky
    -- places the work in the system of the one credited first.
    artist_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE
);

CREATE INDEX artist_credit_artist_idx ON artist_credit (artist_id);
