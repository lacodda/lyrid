-- Similarity: the routes between stars, and how brightly each one burns.
--
-- Part of the canon -- shared by everyone, rebuilt by importers -- and the
-- input the sky layout projects into the 2D coordinates every player sees.

-- Where a set of edges came from and how it was computed. Similarity is not
-- one fact but a family of them: co-listening today, co-listening from an
-- older corpus, and later a deliberately different metric for the "prestige"
-- lens, which shows the same music arranged by another notion of closeness.
-- Scores are only comparable within one metric, never across.
CREATE TABLE similarity_metric (
    id          smallserial PRIMARY KEY,
    -- Stable identifier used by importers and by the layout job, e.g.
    -- 'listenbrainz-2020'.
    key         text NOT NULL UNIQUE,
    -- What a reader needs to judge the numbers: corpus, method, vintage.
    description text NOT NULL
);

-- Edges of the similarity graph: who is listened to alongside whom.
--
-- Stored once per unordered pair (source_id < target_id) rather than twice.
-- The relation is symmetric, and storing both directions doubles a table of
-- millions of rows while inviting the two copies to disagree.
CREATE TABLE artist_similarity (
    metric_id smallint NOT NULL REFERENCES similarity_metric (id) ON DELETE CASCADE,
    source_id integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    target_id integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    -- Strength on the metric's own scale.
    score     real     NOT NULL,
    PRIMARY KEY (metric_id, source_id, target_id),
    CONSTRAINT artist_similarity_ordered CHECK (source_id < target_id)
);

-- Neighbours of an artist come from both columns, so both directions need an
-- index to answer "who is near this star" without a sequential scan.
CREATE INDEX artist_similarity_target_idx ON artist_similarity (metric_id, target_id);

-- How brightly a star is drawn, and how much weight it carries in the layout.
--
-- ListenBrainz publishes no listen counts as a dump -- only a top-1000 API
-- and a non-commercially licensed corpus -- so brightness is derived from the
-- graph itself: an artist woven into many strong edges is a prominent one.
-- The columns are named for what they actually measure, so nothing here can
-- be mistaken for play counts.
CREATE TABLE artist_prominence (
    metric_id  smallint NOT NULL REFERENCES similarity_metric (id) ON DELETE CASCADE,
    artist_id  integer  NOT NULL REFERENCES artist (id) ON DELETE CASCADE,
    -- Number of edges the artist has in this metric.
    degree     integer  NOT NULL,
    -- Sum of those edges' scores: one strong tie counts for more than many
    -- faint ones, which is what keeps a well-connected obscurity from
    -- outshining a genuinely central artist.
    weight     real     NOT NULL,
    PRIMARY KEY (metric_id, artist_id)
);

CREATE INDEX artist_prominence_weight_idx ON artist_prominence (metric_id, weight DESC);
