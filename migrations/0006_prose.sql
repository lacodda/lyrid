-- Prose: the words on an artist card.
--
-- Wikipedia lead paragraphs, under CC BY-SA 4.0. Unlike every other source in
-- the canon, this one carries an obligation that travels with each snippet:
-- attribution. So the attribution is stored **in the same row as the text**
-- rather than attached when something is rendered. A snippet physically cannot
-- travel through the system without the credit it requires -- no query can
-- select the prose and forget the licence, because they are the same row.

CREATE TABLE artist_prose (
    artist_id      integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,

    -- The lead paragraphs, as plain text: templates resolved or removed,
    -- wikilinks flattened to their display words, references and HTML
    -- comments gone.
    extract        text    NOT NULL,

    -- Everything the licence requires, alongside the words it covers.
    --
    -- The article title as it appears in the dump ("Nirvana (band)"), which is
    -- also what the URL is built from, and the licence the text arrived under.
    -- Both are columns rather than one rendered string: a card in another
    -- language, or an export, needs to compose the credit its own way.
    source_title   text    NOT NULL,
    source_url     text    NOT NULL,
    licence        text    NOT NULL,

    -- Which dump the words came from, so a claim can be dated and traced back
    -- to a specific revision of the encyclopaedia.
    dump_version   text    NOT NULL,
    -- The revision id of the article in that dump, when the dump carries one.
    revision_id    bigint,

    -- Kept so a later pass can tell how much of the original survived the
    -- template stripping without re-reading 27 GB. A ratio far from the usual
    -- one is the signature of a lead this parser mangled.
    source_chars   integer NOT NULL,
    extract_chars  integer NOT NULL
);

-- Answering "which artists have prose" is a page-one question for the card
-- and for coverage reporting.
CREATE INDEX artist_prose_dump_idx ON artist_prose (dump_version);
