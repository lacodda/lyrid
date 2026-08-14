-- The canonical universe: the shared, read-only half of lyrid.
--
-- These tables are a projection of MusicBrainz, not a mirror of it. Only what
-- the sky needs is kept, and it is reshaped for reading: names denormalised
-- onto the artist, relationship tables collapsed into columns. Re-importing a
-- newer dump must be able to replace this content wholesale, so nothing here
-- may be edited by users -- per-user data lives in its own tables later.

-- Which dump produced the current contents. One row per completed import, so
-- "which version of the universe is this" is answerable from the database
-- rather than from someone's memory.
CREATE TABLE dump_import (
    id            serial PRIMARY KEY,
    source        text        NOT NULL,
    -- The upstream version string, verbatim: MusicBrainz publishes its
    -- fullexport timestamp (e.g. 20260813-220122) as an opaque identifier.
    version       text        NOT NULL,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz,
    rows_imported bigint,
    UNIQUE (source, version)
);

-- Artists are the stars.
CREATE TABLE artist (
    -- MusicBrainz's own integer id. Kept as the primary key so relationship
    -- tables can be loaded without a lookup pass.
    id            integer PRIMARY KEY,
    -- The MBID: stable across merges and the only id safe to expose publicly
    -- or to trace back to musicbrainz.org.
    mbid          uuid    NOT NULL UNIQUE,
    name          text    NOT NULL,
    sort_name     text    NOT NULL,
    -- Denormalised from artist_type: "Person", "Group", "Orchestra"...
    kind          text,
    -- Denormalised from area: the country or region name.
    area          text,
    -- ISO 3166-1 alpha-2 where the area has one; NULL for regions that do not.
    area_code     text,
    begin_year    smallint,
    end_year      smallint,
    ended         boolean NOT NULL DEFAULT false,
    -- MusicBrainz's disambiguation comment ("US punk band"), shown wherever
    -- two artists share a name.
    comment       text
);

CREATE INDEX artist_name_idx ON artist (name);

-- Release groups are the planets of an artist's system: an album rather than
-- each of its pressings.
CREATE TABLE release_group (
    id           integer PRIMARY KEY,
    mbid         uuid    NOT NULL UNIQUE,
    name         text    NOT NULL,
    -- "Album", "Single", "EP", "Compilation"... from release_group_primary_type.
    primary_type text,
    -- The credited artist. A release group can credit several artists; the
    -- first credited one is what the sky needs to place it in a system.
    artist_id    integer REFERENCES artist (id) ON DELETE CASCADE,
    year         smallint
);

CREATE INDEX release_group_artist_idx ON release_group (artist_id);

-- Artist URL relationships: the addresses the artist card links out to, and
-- the source of YouTube ids for embedded listening -- which is why the
-- YouTube Data API is not needed anywhere in this product.
CREATE TABLE artist_url (
    artist_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    -- MusicBrainz's link type name, verbatim: "official homepage",
    -- "youtube", "bandcamp", "wikidata"...
    kind      text    NOT NULL,
    url       text    NOT NULL,
    PRIMARY KEY (artist_id, kind, url)
);

CREATE INDEX artist_url_kind_idx ON artist_url (kind);
