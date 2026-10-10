-- Scrobbling: real listening, read from ListenBrainz, becomes openings and light.
--
-- The first personal data that is not something a person typed into lyrid. It
-- is read from their ListenBrainz account, where it is already public, and it
-- is kept for one reason: so their own sky can show what they have heard and
-- what that was worth. See ADR 0019.

-- The ListenBrainz account a lyrid account reads its listening from.
--
-- No token. Linking asks for one, to prove the account belongs to whoever is
-- linking it, and that is all it is used for: listens on ListenBrainz are
-- public and read by name, while a stored token would be the power to submit
-- listens in someone's name -- a credential lyrid has no use for and so should
-- not be holding.
CREATE TABLE listenbrainz_link (
    user_id      integer     PRIMARY KEY REFERENCES app_user (id) ON DELETE CASCADE,
    -- The name as ListenBrainz spells it, taken from its answer to the token
    -- rather than from what was typed.
    name         text        NOT NULL,
    linked_at    timestamptz NOT NULL DEFAULT now(),
    -- How far the listening has been read: every listen before this moment
    -- that ListenBrainz held when it was read has been counted. NULL until the
    -- first read succeeds. A cursor, and not the time of the last read: a read
    -- that stops at its page limit is complete only up to its last listen.
    read_through timestamptz,
    -- When the last read that succeeded finished. What the interface shows as
    -- "read five minutes ago".
    read_at      timestamptz,
    -- When a read was last started, successful or not. The schedule and the
    -- one-a-minute limit on reading by hand both ask this one column, so there
    -- is no second "next read at" to drift away from it.
    tried_at     timestamptz,
    -- Why the last read failed, or NULL when it did not. A code rather than a
    -- sentence: the interface says it in the reader's language, and the four
    -- causes are a closed set the code below handles one by one.
    failure      text        CHECK (failure IN ('unreachable', 'unknown_user', 'throttled', 'malformed'))
);

-- One ListenBrainz account feeds one lyrid account. Two accounts linked to the
-- same listening would be one person's music paying twice. Case-folded because
-- the names come from MusicBrainz, which does not let two accounts differ by
-- case alone.
CREATE UNIQUE INDEX listenbrainz_link_name_idx ON listenbrainz_link (lower(name));

-- One row per listen, and what it paid.
--
-- A ledger rather than a tally per star, because the same listen is read more
-- than once: the window is read again so that listens a phone submits hours
-- late are not lost, and ListenBrainz matches a listen to its artists some time
-- after it arrives. The key below is ListenBrainz's own identity of a listen,
-- so a listen read twice is a conflict the database refuses rather than a
-- second payment someone has to remember not to make.
--
-- What is kept is what the game uses: when, whose, how long, how much light.
-- No track or album names -- the artists are what the sky is made of, and the
-- rest stays on ListenBrainz where it came from.
CREATE TABLE listen (
    user_id        integer     NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    listened_at    timestamptz NOT NULL,
    recording_msid uuid        NOT NULL,
    -- The credited artists' MusicBrainz ids, in credit order. Ids and not the
    -- canon's own keys: an artist outside the canon is still someone the
    -- person heard, and the canon may hold them next year.
    --
    -- Empty when ListenBrainz could not say who it was. Such a listen still
    -- paid -- it was real listening -- but it opens no star.
    artist_mbids   uuid[]      NOT NULL,
    -- The recording's length as the listen reported it, or NULL when it did
    -- not. Not filled with a guess: what the light assumed for an unknown
    -- length is a rule in the code, and the row keeps only what was reported.
    seconds        integer     CHECK (seconds > 0),
    -- What this listen paid when it arrived. Written once: a listen paid under
    -- one rule stays paid under it, as money does.
    light          integer     NOT NULL CHECK (light > 0),
    PRIMARY KEY (user_id, listened_at, recording_msid)
);
