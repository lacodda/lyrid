-- The sky layout: where every star sits.
--
-- ADR 0003 settles the shape: the canonical 2D layout is computed offline by a
-- batch job and versioned, and the tile pyramid is built from it. Browsing the
-- sky never touches these tables -- it reads static tiles -- but the tiles are
-- generated from here, and a player's saved position refers to coordinates
-- that must keep meaning across a canon rebuild.

-- One row per layout run.
--
-- Versioned for the same reason similarity is: a layout is not a fact but a
-- rendering of one. Recomputing it moves every star, so an old layout has to
-- remain addressable while a new one is built and checked. Coordinates are
-- only comparable within one layout, never across them.
CREATE TABLE sky_layout (
    id          smallserial PRIMARY KEY,
    -- Stable identifier, e.g. 'listenbrainz-2020-fa2-v1'.
    key         text        NOT NULL UNIQUE,
    -- Which similarity graph was projected. A layout of a different metric is
    -- a different sky.
    metric_id   smallint    NOT NULL REFERENCES similarity_metric (id) ON DELETE CASCADE,
    -- Algorithm, parameters and seed, in enough detail to run it again and get
    -- the same sky. Stochastic layouts are reproducible only if the seed is
    -- recorded, so it is part of the description rather than a footnote.
    description text        NOT NULL,
    -- The random seed the run used.
    seed        bigint      NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    -- How many stars it placed, so a truncated run is visible as a number
    -- rather than as a suspicion.
    stars       integer
);

-- Where each star sits in one layout.
--
-- Coordinates are stored as `real`: the layout is a picture, not a
-- measurement, and single precision is far finer than any zoom level shows.
-- The pair is what the tile builder reads, sorted by the tile it falls into.
CREATE TABLE artist_position (
    layout_id smallint NOT NULL REFERENCES sky_layout (id) ON DELETE CASCADE,
    artist_id integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    x         real     NOT NULL,
    y         real     NOT NULL,
    PRIMARY KEY (layout_id, artist_id)
);

-- The tile builder walks a layout in spatial order; without this it would sort
-- millions of rows per level.
CREATE INDEX artist_position_xy_idx ON artist_position (layout_id, x, y);
