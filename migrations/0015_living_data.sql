-- Living data: the sky's weather, read every day from ListenBrainz's own open
-- data; the editions of the canon and what changed between them; and who may
-- read the state of the stand itself. See ADR 0022 and ADR 0023.

-- ------------------------------------------------------------------ feeds

-- One row per pass over something upstream: a daily dump of ListenBrainz
-- listening, or the list of fresh releases.
--
-- The history of every pass, failures included, is what the stand's admin
-- page reads as the health of its imports -- and a successful row is also what
-- says a dump has been counted, so the same dump can never be counted twice
-- (the unique index below).
CREATE TABLE feed_run (
    id          bigserial   PRIMARY KEY,
    -- 'listening': a daily incremental dump of ListenBrainz listens;
    -- 'releases': ListenBrainz's list of releases out around today.
    feed        text        NOT NULL CHECK (feed IN ('listening', 'releases')),
    -- What was read, verbatim: the dump's directory name, or the UTC day the
    -- list was read on.
    source      text        NOT NULL,
    started_at  timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    -- For a dump: the span of time ListenBrainz says it holds, by when each
    -- listen reached ListenBrainz. Filled when the pass succeeds.
    span_from   timestamptz,
    span_to     timestamptz,
    -- Rows read from upstream, and how many of them became something here.
    rows_read   bigint      CHECK (rows_read >= 0),
    rows_kept   bigint      CHECK (rows_kept >= 0),
    -- Why the pass failed, as the error said it. NULL while running and on
    -- success.
    error       text,
    -- Three states and no fourth: running (nothing filled in), done (finished
    -- with its counts) or failed (finished with its reason).
    CONSTRAINT feed_run_state CHECK (
        (finished_at IS NULL AND rows_read IS NULL AND rows_kept IS NULL AND error IS NULL)
        OR (finished_at IS NOT NULL AND error IS NULL AND rows_read IS NOT NULL AND rows_kept IS NOT NULL)
        OR (finished_at IS NOT NULL AND error IS NOT NULL AND rows_read IS NULL AND rows_kept IS NULL)
    )
);

-- A dump is counted once, and the list is read once a day.
CREATE UNIQUE INDEX feed_run_done_idx ON feed_run (feed, source) WHERE finished_at IS NOT NULL AND error IS NULL;
CREATE INDEX feed_run_recent_idx ON feed_run (feed, started_at DESC);

-- One row per UTC day the listening feed knows about.
--
-- A day is the day ListenBrainz received a listen, not the day it was played:
-- the dumps are cut by arrival, so this is the only day whose coverage can be
-- said exactly. Nearly every listen arrives within hours of being played
-- (measured on 2026-10-16: all of the live ones within a day).
CREATE TABLE listening_day (
    day             date        PRIMARY KEY,
    -- How much of the day the dumps read so far cover. ListenBrainz skips
    -- days now and then -- 3, 4 and 8 October 2026 have no dump at all -- and
    -- a day it skipped must read as missing, not as a day nobody listened.
    covered_seconds integer     NOT NULL CHECK (covered_seconds BETWEEN 0 AND 86400),
    -- How many people were heard listening to stars of this canon that day.
    -- Known once the day is closed.
    listeners       integer     CHECK (listeners >= 0),
    -- When the day was closed: no dump still to come can add to it.
    closed_at       timestamptz,
    CHECK ((closed_at IS NULL) = (listeners IS NULL))
);

-- Who heard whom, on a day still open.
--
-- Distinct listeners cannot be added up across dumps -- the same person in two
-- dumps of one day is one listener -- so a day's pairs wait here until the day
-- is closed and counted, and are deleted then. The listener is ListenBrainz's
-- own number for a person, as its public dump gives it; it never leaves this
-- table and this table never keeps it past the day.
CREATE TABLE listener_seen (
    day         date   NOT NULL,
    artist_mbid uuid   NOT NULL,
    listener    bigint NOT NULL,
    PRIMARY KEY (day, artist_mbid, listener)
);

-- How many people listened to a star on a day.
--
-- People, not listens: one account submitting a hundred thousand listens a day
-- is one listener here (the 2026-10-16 dump held exactly that). A star heard
-- by one person in a day is that person's taste, not a trend, and is not kept.
--
-- Keyed by MusicBrainz id rather than the canon's own key, so rebuilding the
-- canon or cutting a slice -- both of which empty the canon's tables -- does
-- not take the weather with them. Kept for thirteen weeks.
CREATE TABLE artist_day (
    day         date    NOT NULL,
    artist_mbid uuid    NOT NULL,
    listeners   integer NOT NULL CHECK (listeners >= 2),
    PRIMARY KEY (artist_mbid, day)
);

CREATE INDEX artist_day_day_idx ON artist_day (day);

-- The stars rising this week: worked out once, when a day is closed, rather
-- than on every request, because it reads five weeks of the table above.
CREATE TABLE star_rising (
    artist_mbid uuid    PRIMARY KEY,
    rank        integer NOT NULL UNIQUE CHECK (rank > 0),
    -- Listeners a day over the last seven days, and over the four weeks
    -- before them, each divided by how much of its days the dumps cover.
    recent      real    NOT NULL CHECK (recent >= 0),
    earlier     real    NOT NULL CHECK (earlier >= 0),
    -- The last day of the seven.
    through     date    NOT NULL
);

-- Releases out around now, as ListenBrainz lists them.
--
-- One row per release group, with its date as the list last gave it. Kept
-- past its week: the dates a star releases on are what a pulsar is made of
-- later.
-- No tags (MusicBrainz's tags are not CC0 -- ADR 0005) and no cover art.
CREATE TABLE new_release (
    release_group_mbid uuid        PRIMARY KEY,
    title              text        NOT NULL,
    -- How the release credits its artists, verbatim ("A feat. B").
    credit             text        NOT NULL,
    artist_mbids       uuid[]      NOT NULL CHECK (cardinality(artist_mbids) > 0),
    -- 'Album', 'Single', 'EP'... as MusicBrainz names it; NULL when unset.
    primary_type       text,
    out_on             date        NOT NULL,
    first_seen_at      timestamptz NOT NULL DEFAULT now(),
    seen_at            timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX new_release_out_idx ON new_release (out_on);
CREATE INDEX new_release_artists_idx ON new_release USING gin (artist_mbids);

-- ---------------------------------------------------------- canon editions

-- An edition of the canon: one sky, sealed once it is built and checked.
--
-- The dumps a canon is built from and the layout drawn over them are recorded
-- by their importers; an edition is the moment they are declared to be the
-- sky, and the thing a person's progress will refer to (ADR 0023). Nothing
-- here references the canon's tables: rebuilding the canon empties those, and
-- the editions are its history.
CREATE TABLE canon_edition (
    id         smallserial PRIMARY KEY,
    sealed_at  timestamptz NOT NULL DEFAULT now(),
    -- The layout the stars were placed by, by its key: a rebuild renumbers
    -- layouts along with everything else.
    layout_key text        NOT NULL UNIQUE,
    -- Artists in the canon, and how many of them the layout placed.
    artists    integer     NOT NULL CHECK (artists > 0),
    stars      integer     NOT NULL CHECK (stars > 0 AND stars <= artists)
);

-- Which import of each source the edition was built from. Copied rather than
-- referenced: importing a source again rewrites its record.
CREATE TABLE canon_edition_source (
    edition_id    smallint    NOT NULL REFERENCES canon_edition (id) ON DELETE CASCADE,
    source        text        NOT NULL,
    version       text        NOT NULL,
    imported_at   timestamptz NOT NULL,
    rows_imported bigint,
    PRIMARY KEY (edition_id, source)
);

-- The stars of an edition, with the names they had: what the next edition is
-- compared against, after the canon they came from is gone.
CREATE TABLE canon_edition_star (
    edition_id smallint NOT NULL REFERENCES canon_edition (id) ON DELETE CASCADE,
    mbid       uuid     NOT NULL,
    name       text     NOT NULL,
    PRIMARY KEY (edition_id, mbid)
);

-- What changed in the sky from the edition before: a star that is new, one
-- that MusicBrainz merged into another, and one that is simply gone.
CREATE TABLE canon_change (
    edition_id smallint NOT NULL REFERENCES canon_edition (id) ON DELETE CASCADE,
    mbid       uuid     NOT NULL,
    change     text     NOT NULL CHECK (change IN ('new', 'merged', 'gone')),
    -- The star a merged one became.
    into_mbid  uuid,
    -- The name as the edition knew it: the canon no longer holds a star that
    -- is gone.
    name       text     NOT NULL,
    PRIMARY KEY (edition_id, mbid),
    CHECK ((change = 'merged') = (into_mbid IS NOT NULL))
);

-- MusicBrainz ids that now lead to another artist: what is left behind when
-- two artists are merged. Part of the canon, imported with it.
CREATE TABLE artist_redirect (
    mbid      uuid    PRIMARY KEY,
    artist_id integer NOT NULL REFERENCES artist (id) ON DELETE CASCADE
);

CREATE INDEX artist_redirect_artist_idx ON artist_redirect (artist_id);

-- ------------------------------------------------------------------ admin

-- Who may read the stand's own state: its canon, its feeds and the counts of
-- its accounts. Granted from the server's command line and nowhere else, so
-- holding a shell on the stand is what it takes to become one.
ALTER TABLE app_user ADD COLUMN admin boolean NOT NULL DEFAULT false;
