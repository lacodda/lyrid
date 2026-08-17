-- Genres, styles and labels: what a star is made of, and who pressed it.
--
-- MusicBrainz carries genres only as folksonomy tags, and those ship in
-- mbdump-derived under CC BY-NC-SA -- a non-commercial licence that travels to
-- everything computed from it, which is why MLHD+ was already turned down for
-- brightness. Discogs publishes its genre and style vocabulary under CC0
-- instead, so the canon stays public domain end to end. See ADR 0005.

-- Discogs attaches genres to releases, never to artists. The importer resolves
-- an artist's genres by aggregating over their releases, and the counts below
-- are what make that aggregate honest: a band with thirty techno releases and
-- one ambient EP is a techno band, and the numbers say so rather than the
-- alphabet deciding.

-- Which Discogs artist a canonical artist is, so later imports (and anyone
-- reading the database) can follow the join without redoing the URL parsing.
--
-- The link comes from MusicBrainz's own `discogs` artist-URL relationship, not
-- from name matching: names collide constantly ("Nirvana" is at least three
-- acts) and a wrong join here would put someone else's genres on a star.
CREATE TABLE artist_discogs (
    artist_id  integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,
    -- The integer from discogs.com/artist/<id>.
    discogs_id integer NOT NULL
);

-- Two artists can legitimately point at one Discogs id (MusicBrainz splits an
-- act that Discogs keeps whole), so this is an index rather than a constraint.
CREATE INDEX artist_discogs_id_idx ON artist_discogs (discogs_id);

-- The genre vocabulary, kept as a table rather than repeated as text on every
-- row: Discogs has roughly fifteen genres and a few thousand styles, and the
-- sky needs to enumerate them to draw nebulae.
--
-- Genres and styles share this table, separated by `is_style`. They are the
-- same kind of fact at two depths -- "Electronic" and "Techno" -- and keeping
-- them apart in two tables would double every query the map makes.
CREATE TABLE genre (
    id       serial PRIMARY KEY,
    name     text    NOT NULL,
    -- false for a Discogs genre ("Electronic"), true for a style ("Techno").
    is_style boolean NOT NULL,
    UNIQUE (name, is_style)
);

-- An artist's genres, aggregated from their releases.
CREATE TABLE artist_genre (
    artist_id integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    genre_id  integer  NOT NULL REFERENCES genre (id) ON DELETE CASCADE,
    -- How many of the artist's releases carry this genre. The weight behind
    -- "this artist is techno", and the tie-breaker when a discography spans
    -- several scenes.
    releases  integer  NOT NULL,
    PRIMARY KEY (artist_id, genre_id)
);

-- Answers "who else is in this nebula", which is how the map reads this table.
CREATE INDEX artist_genre_genre_idx ON artist_genre (genre_id, releases DESC);

-- Labels are the stations of the map: an imprint with a roster and a sound.
CREATE TABLE label (
    -- Discogs's own label id, from discogs.com/label/<id>.
    id              integer PRIMARY KEY,
    name            text    NOT NULL,
    -- Discogs's free-text description, kept for the station page. Contact
    -- blocks are deliberately not imported: they carry postal addresses and
    -- personal e-mail of small-label owners, which this product has no use for.
    profile         text,
    -- An imprint's owner, when Discogs records one. Self-references are
    -- possible in the dump and are dropped by the importer.
    parent_label_id integer REFERENCES label (id) ON DELETE SET NULL
);

CREATE INDEX label_name_idx ON label (name);
CREATE INDEX label_parent_idx ON label (parent_label_id);
