-- Accounts: the first personal data in the canon's database.
--
-- Everything above this migration is the shared universe -- reproducible from
-- open dumps, losable without regret. Everything from here is not: a profile
-- cannot be rebuilt from a dump, which is why the stand's restore refuses to
-- overwrite a non-empty database without --force (decision of 2026-08-27,
-- taken before it was needed).
--
-- The tables stay deliberately small. A mode, a camera and a marker are what
-- an account remembers today; fog, light and a log arrive with the game
-- (v0.18 onwards) as their own tables referencing this one, not as columns
-- speculatively added here.

CREATE TABLE app_user (
    id            serial      PRIMARY KEY,
    -- Stored case-folded: addresses are compared case-insensitively, and
    -- folding on the way in makes the UNIQUE constraint do that work rather
    -- than every query having to remember to.
    email         text        NOT NULL UNIQUE,
    -- A PHC string: the algorithm, its parameters and the salt travel with
    -- the hash, so a future change of parameters does not invalidate rows
    -- written under the old ones.
    password_hash text        NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- The mode is a column of the profile rather than of the account because it
-- describes the game being played, not the person playing it -- and it is
-- written once. There is deliberately no UPDATE path for it: "the mode is
-- chosen once at profile creation and never changes" is a product rule
-- (Vision, principle 5), and a rule enforced only by the client is a rule
-- until the first curl.
CREATE TABLE user_profile (
    user_id      integer     PRIMARY KEY REFERENCES app_user (id) ON DELETE CASCADE,
    -- 'explore' or 'create'. A check constraint rather than an enum type:
    -- the set is closed today, and a domain-level enum would need its own
    -- migration to grow while a check does not change how the column reads.
    mode         text        NOT NULL CHECK (mode IN ('explore', 'create')),
    -- How this user likes their marked star drawn. Moved here from the
    -- browser's local storage: a preference kept per browser is lost on the
    -- next machine, and an account exists to carry it across.
    halo_shape   text,
    halo_colour  text,
    -- Where the sky was last left. Coordinates are only comparable within one
    -- layout (see 0007), so the layout they were taken in travels with them:
    -- a rebuilt sky moves every star, and restoring a camera into the wrong
    -- layout would open the map on empty space that used to be somewhere.
    camera_x     real,
    camera_y     real,
    camera_scale real,
    layout_id    smallint    REFERENCES sky_layout (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Sessions live in the database rather than in a signed cookie so that
-- logging out actually ends the session server-side: a stateless token stays
-- valid until it expires no matter what the user pressed.
CREATE TABLE user_session (
    -- The token as sent to the browser. Random, not derived from anything.
    token      text        PRIMARY KEY,
    user_id    integer     NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);

-- Logging out one browser should not walk every session in the table, and
-- deleting an account cascades through this index rather than a sequential
-- scan.
CREATE INDEX user_session_user_idx ON user_session (user_id);
