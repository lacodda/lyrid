-- The fresh graph: how many people listen to each star, read from
-- ListenBrainz's statistics dump, in place of the graph's own connectivity as
-- the brightness of the sky. The similarity those same people imply arrives
-- as a second metric beside the 2020 one, in the tables that already hold
-- metrics. See ADR 0024.

-- How many people listen to an artist, as ListenBrainz's statistics dump
-- counts them.
--
-- The dump lists, for each person, the thousand artists they have played most
-- over all time. A listener here is a person with this artist among their
-- thousand, and the listens are theirs, summed. A heavy listener's long tail
-- -- artists played a handful of times, past their thousandth -- is not in the
-- dump: the count is of the people who keep coming back to an artist, not of
-- everyone who ever played one.
--
-- People rather than plays: one account sending a hundred thousand listens is
-- one listener, as it is in the weather (0015).
--
-- Part of the canon -- rebuilt by its importer, keyed by the canon's own id --
-- and the brightness every star is drawn with.
CREATE TABLE artist_listeners (
    artist_id integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,
    listeners integer NOT NULL CHECK (listeners > 0),
    -- Every listener played the artist at least once.
    listens   bigint  NOT NULL CHECK (listens >= listeners)
);

-- The slice keeps the most listened, and lists order by it.
CREATE INDEX artist_listeners_listeners_idx ON artist_listeners (listeners DESC);

-- Brightness was the graph's connectivity while no counts of listeners were
-- published (ADR 0004). They are now, and keeping the proxy beside the
-- measure would be two answers to "how bright is this star".
DROP TABLE artist_prominence;
