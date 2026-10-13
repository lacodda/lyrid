-- Your life on the sky: the listening from before the link, a history brought
-- in a file, the year someone was born, and a letter once a week.
--
-- See ADR 0020 for the history and the file, and ADR 0021 for the letter.

-- Where a listen came from.
--
-- 'listenbrainz' is read from the linked account; 'lastfm' was brought in a
-- file someone exported from Last.fm and uploaded. Kept on the row because the
-- two are not the same promise: a file is whatever its owner says it is, and
-- the person can take a file's history back without touching what
-- ListenBrainz said.
ALTER TABLE listen ADD COLUMN source text NOT NULL DEFAULT 'listenbrainz'
    CHECK (source IN ('listenbrainz', 'lastfm'));
ALTER TABLE listen ALTER COLUMN source DROP DEFAULT;

-- The listen's recording, as its source identifies it.
--
-- For ListenBrainz, its own recording_msid, as before. A Last.fm export has
-- no such id, so a listen from a file is identified by a UUID derived from its
-- artist and track names: the same file uploaded twice is the same rows, and
-- the key refuses the second copy the way it refuses a window read twice.
ALTER TABLE listen RENAME COLUMN recording_msid TO recording;

-- What a listen paid, or NULL for a listen that paid nothing.
--
-- Light is gathered by listening while the account is linked. A listen from
-- before the link -- the history the link brings with it -- and a listen from
-- a file open stars and count towards what is familiar, and pay nothing: the
-- past is not fuel, and a file is whatever someone typed into it. NULL rather
-- than zero because "paid nothing" and "paid zero" are different statements,
-- and the rule never prices a listen at zero.
ALTER TABLE listen ALTER COLUMN light DROP NOT NULL;
ALTER TABLE listen ADD CONSTRAINT listen_a_file_pays_nothing CHECK (source = 'listenbrainz' OR light IS NULL);

-- How often one person heard one star is asked every time a card opens, and
-- with a whole history in the ledger that question read every listen the
-- person has: half a second at twenty years (220,291 rows, measured on
-- 2026-10-13). The index answers it from the star's side.
CREATE INDEX listen_artist_mbids_idx ON listen USING gin (artist_mbids);

-- Where a person's listening on ListenBrainz begins, as ListenBrainz said when
-- it was first asked. Reading starts here rather than at the link, so the sky
-- lights up with the whole of someone's history and not only what comes
-- after; and it is what the panel measures the history's progress against.
-- NULL until the first read has asked.
ALTER TABLE listenbrainz_link ADD COLUMN history_from timestamptz;

-- Every account linked before this migration reads its history from the
-- start. The ledger's key makes the listens it already holds cost nothing to
-- read again, and they keep the light they paid.
UPDATE listenbrainz_link SET read_through = NULL;

-- A name from a Last.fm file is looked up among the canon's names, folded the
-- way Last.fm folds them.
CREATE INDEX artist_lower_name_idx ON artist (lower(name));

-- The stars of someone's youth: who put out a record in a span of years, and
-- when each of them first did. Partial, because only these three kinds of
-- record count as music coming out (a compilation is a catalogue going round
-- again), and the index on everything would be twice the size for nothing.
CREATE INDEX release_group_year_idx ON release_group (year, artist_id)
    WHERE primary_type IN ('Album', 'EP', 'Single');
CREATE INDEX release_group_artist_year_idx ON release_group (artist_id, year)
    WHERE primary_type IN ('Album', 'EP', 'Single');

-- The year the person was born, if they said: what turns "the stars of your
-- youth" into years. Optional, cleared by clearing it, listed by the charter.
ALTER TABLE user_profile ADD COLUMN born smallint CHECK (born BETWEEN 1900 AND 2100);

-- The weekly letter, for those who asked for it.
--
-- A row is the asking: no row, no letter. Deleting the row is how it stops,
-- from the account or from the link at the bottom of any letter.
CREATE TABLE weekly_letter (
    user_id      integer     PRIMARY KEY REFERENCES app_user (id) ON DELETE CASCADE,
    -- The language the letter is written in: the one the person was reading
    -- lyrid in when they asked. The interface keeps its language per browser,
    -- and a letter has no browser to ask.
    language     text        NOT NULL CHECK (language IN ('en', 'ru')),
    subscribed_at timestamptz NOT NULL DEFAULT now(),
    -- The Monday that began the last week a letter was considered for. A
    -- letter goes out once per week at most: taking the week is one UPDATE,
    -- so two servers cannot both send it.
    sent_week    date        NOT NULL CHECK (extract(isodow FROM sent_week) = 1),
    -- The secret in the link that stops the letter without signing in. Not
    -- derived from anything, and good for nothing but stopping this letter.
    token        text        NOT NULL UNIQUE
);
