-- Stations: the labels and the scenes a dossier opens on.
--
-- Who released on a label, and when: what turns a label into a place on the
-- map rather than a name on a card.
--
-- Discogs links a label to an artist only through a release -- the labels file
-- says what an imprint is, the masters file carries no labels at all -- so both
-- tables below come from a pass over the releases file of the same dump that
-- filled `label`. See ADR 0017.

-- A label's roster: the canonical artists with releases on it.
--
-- Counted over official releases, pressings included: a label that issued an
-- album in twelve countries did issue it twelve times, and Discogs's own
-- label pages count the same way. Unofficial releases are not counted -- a
-- bootleg names a label that never agreed to it -- and neither is "Not On
-- Label", which is Discogs's way of saying there was none.
CREATE TABLE label_artist (
    label_id   integer  NOT NULL REFERENCES label (id) ON DELETE CASCADE,
    artist_id  integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    -- How many of the artist's releases carry this label: the weight that
    -- ranks a roster, the way `artist_genre.releases` ranks a genre.
    releases   integer  NOT NULL,
    -- The years of the first and the last of them, where Discogs dates them.
    -- An artist's span on the label, which is what a label's chronology is
    -- made of.
    first_year smallint,
    last_year  smallint,
    PRIMARY KEY (label_id, artist_id),
    CONSTRAINT label_artist_releases_positive CHECK (releases > 0),
    CONSTRAINT label_artist_years_ordered CHECK (first_year <= last_year)
);

-- "Which labels did this artist release on" is the card's question, and it
-- reads the second column.
CREATE INDEX label_artist_artist_idx ON label_artist (artist_id, releases DESC);

-- A label's output year by year, over all of its official releases.
--
-- All of them, not only the canon's: this is the label's own history, and a
-- chronology drawn from the canonical artists alone would show a label as
-- quiet in exactly the years it was busiest with acts the canon does not hold.
-- Kept only for labels that have a roster -- the ones a dossier can be opened
-- for -- because the rest of the two million are never shown.
--
-- A label's first year, last year and total are read from here rather than
-- stored on `label` beside it: one fact, one place.
CREATE TABLE label_year (
    label_id integer  NOT NULL REFERENCES label (id) ON DELETE CASCADE,
    year     smallint NOT NULL,
    releases integer  NOT NULL,
    PRIMARY KEY (label_id, year),
    CONSTRAINT label_year_releases_positive CHECK (releases > 0)
);

-- A scene is a place of origin, and its dossier asks "who comes from here":
-- the question reads `artist_fact` by place rather than by artist.
CREATE INDEX artist_fact_origin_idx ON artist_fact (origin_qid) WHERE origin_qid IS NOT NULL;
