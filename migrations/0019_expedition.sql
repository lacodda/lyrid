-- The expedition: the star a person sets out from, and the contracts the game
-- hands them. See ADR 0028.

-- A star named as the one someone loves is open from that moment. They have
-- heard it -- that is what loving an artist means -- and the expedition starts
-- from a lit star with its echoes in sight rather than from a dark one that may
-- have nowhere to be played. Recorded as what it was, like every other way in:
-- a sky with first discoverers will have to tell a star named apart from one
-- heard here.
ALTER TABLE star_opening DROP CONSTRAINT star_opening_how_check;
ALTER TABLE star_opening ADD CONSTRAINT star_opening_how_check
    CHECK (how IN ('played', 'followed', 'brought', 'named'));

-- The home port: the star an expedition set out from, by MusicBrainz id so it
-- outlives a rebuilt canon the way the openings do, and is carried through the
-- redirects with them. NULL until the expedition begins. Not the home star the
-- listening points at (scrobbling::home): that one moves with what a person
-- hears, and this one is where they chose to start.
ALTER TABLE user_profile ADD COLUMN home_port uuid;

-- A task the game sets: open so many new stars of a sector. The sector is a
-- star's neighbourhood by the similarity the sky on show was laid out from,
-- which is where its echoes come from, so the dashed rings a person follows
-- are always the way to fulfil it. Measured on edition 3: the closest similar
-- star of a well-known artist lies 85 units away on the median, six times the
-- reach of the fog's light, so "near on the map" would leave the echoes out.
--
-- Progress is not stored. It is read from the openings, which are already the
-- record of what was heard, so it cannot drift from them. What is stored is
-- the moment it was fulfilled: once fulfilled it stays so whatever a later
-- canon does to the neighbourhood, and a later economy pays against it.
CREATE TABLE contract (
    id        integer     GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id   integer     NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    -- 'home_sector': the expedition's first, about the home port.
    kind      text        NOT NULL CHECK (kind IN ('home_sector')),
    -- The star whose neighbourhood it is about, by MusicBrainz id.
    star_mbid uuid        NOT NULL,
    -- How many stars, first opened after it was issued, fulfil it. Zero only
    -- when the neighbourhood had nothing left to open, and then it was
    -- fulfilled as it was issued.
    goal      smallint    NOT NULL CHECK (goal >= 0),
    issued_at timestamptz NOT NULL DEFAULT now(),
    done_at   timestamptz CHECK (done_at >= issued_at),
    CHECK (goal > 0 OR done_at IS NOT NULL)
);

CREATE INDEX contract_user_idx ON contract (user_id);

-- One expedition per person, so one first contract.
CREATE UNIQUE INDEX contract_home_sector_once ON contract (user_id) WHERE kind = 'home_sector';
