-- Opening the doors: a confirmed address, a way back in, and a mode that is
-- no longer asked for.
--
-- Three changes that belong together because all three are about what an
-- account costs a person: an address that is checked rather than assumed, a
-- password that can be recovered rather than lost for good, and one less
-- question at the door.

-- Whether the address has been confirmed, and when.
--
-- Nullable rather than a boolean: "confirmed" and "confirmed at 14:02 on
-- Tuesday" are the same fact, and the timestamp answers questions the boolean
-- cannot -- how long a confirmation took, whether a batch of accounts was made
-- at once. A boolean beside a timestamp would be two truths about one thing.
--
-- Deliberately not a gate on signing in. The sky is public (S4) and an account
-- remembers a camera and a marker; locking that behind a mail round-trip would
-- trade a real loss for a theoretical one. What confirmation buys is a
-- deliverable address to send a password reset to, and that is stated where
-- the reset is refused rather than at the door.
ALTER TABLE app_user ADD COLUMN email_confirmed_at timestamptz;

-- Every account that existed before this migration is treated as confirmed.
--
-- They were made by one person on a stand with no mail server, and marking
-- them unconfirmed would mean the owner's own accounts could not reset their
-- password -- inventing a problem in order to have solved it correctly.
UPDATE app_user SET email_confirmed_at = created_at;

-- The mode is no longer chosen at the door (decision of 2026-09-11).
--
-- ADR 0012 says the mode is chosen once and never changes, and that stands.
-- What changes is *when*: there is nothing to choose between until the game
-- exists, so asking now is asking someone to decide the shape of a thing they
-- have not seen. Everyone gets the creative mode -- the one that is actually
-- built -- and the choice arrives with the fog (v0.22), offered once to the
-- profiles that predate it.
--
-- A default rather than a dropped NOT NULL: a row without a mode would be a
-- fourth state to handle everywhere, where a default is none.
ALTER TABLE user_profile ALTER COLUMN mode SET DEFAULT 'create';

-- One-time tokens: confirming an address, and getting back into an account.
--
-- One table for both because they are the same mechanism with different words
-- on the button -- a random secret, mailed to an address, good once and not
-- for long. Two tables would be the same five columns twice, and the second
-- copy is where the expiry check gets forgotten.
CREATE TABLE user_token (
    -- The token as it travels in the link. Random, not derived from the
    -- account: a token that can be computed from an address is not a secret.
    token      text        PRIMARY KEY,
    user_id    integer     NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    -- 'confirm' or 'reset'. A check constraint rather than an enum type, as
    -- with the mode: the set is closed today and reads the same either way.
    purpose    text        NOT NULL CHECK (purpose IN ('confirm', 'reset')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    -- When the token was spent. A spent token is kept rather than deleted so
    -- that a second click on the same link can say "this link has been used"
    -- instead of the same words as a forged one -- and so a reset that is
    -- replayed is visible as a fact rather than absent as a row.
    used_at    timestamptz
);

-- Issuing a new token invalidates the account's older ones of the same kind,
-- and that sweep must not walk the table.
CREATE INDEX user_token_user_purpose_idx ON user_token (user_id, purpose);

-- How often each mechanic is used, and nothing else.
--
-- A counter incremented in place rather than a row per event. That is the
-- whole design: there is no row to join to an account, no order to read a
-- session out of, and nothing that becomes personal later when someone thinks
-- of a clever query. A table of events with the user id left out is still a
-- table of events -- and the version after it adds a timestamp "for
-- debugging" and has rebuilt the log this exists not to keep.
--
-- Bucketed by day because a day is the coarsest unit that still answers "did
-- the change on Tuesday help", and fine enough that nothing about one person
-- can be read out of it.
CREATE TABLE mechanic_use (
    -- One of the names listed in src/api/metrics.rs. Not a foreign key: the
    -- list is code, and a lookup table would be a second place to keep it in
    -- step.
    mechanic text NOT NULL,
    day      date NOT NULL,
    uses     bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (mechanic, day)
);
