-- The fog: a sky that starts dark, stars a person opened by listening, and the
-- choice between the two modes, offered at last. See ADR 0026.

-- The mode is chosen now, once, by everybody.
--
-- NULL means not chosen yet, which is a state and not a gap: every profile
-- made before this version was put in the creative mode by a default nobody
-- chose (migration 0009 explains why), and they are all asked now, on the same
-- screen as someone who has just registered. So the default goes, the existing
-- answers are forgotten, and there is one way to arrive at a mode.
ALTER TABLE user_profile ALTER COLUMN mode DROP DEFAULT;
ALTER TABLE user_profile ALTER COLUMN mode DROP NOT NULL;
UPDATE user_profile SET mode = NULL;

-- "Chosen once and never changed" (Vision, principle 5) is held here rather
-- than only in the route: no code path, today's or a later one, can change a
-- mode that has been chosen.
CREATE FUNCTION user_profile_mode_is_final() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'the mode of a profile is chosen once'
        USING ERRCODE = 'restrict_violation';
END
$$;

CREATE TRIGGER user_profile_mode_final
    BEFORE UPDATE OF mode ON user_profile
    FOR EACH ROW
    WHEN (OLD.mode IS NOT NULL AND NEW.mode IS DISTINCT FROM OLD.mode)
    EXECUTE FUNCTION user_profile_mode_is_final();

-- Which edition of the canon this person's progress has been carried into, by
-- `canon_edition.id`. NULL until the first carry; behind the newest edition
-- once a new sky has arrived and not been carried into yet.
--
-- No foreign key, on purpose. Filling a stand replaces `canon_edition` under
-- the accounts, and a reference into it would have to let go of the row or
-- stop the replacement; the number has to survive that, because it is what
-- says whether this person has been carried into what just arrived.
ALTER TABLE user_profile ADD COLUMN canon_edition smallint;

-- Stars opened in lyrid itself.
--
-- A star is open when someone has listened to it. Listening elsewhere is
-- already a fact in the ledger (`listen`) and is not written again here: this
-- table holds only what the ledger cannot, the listening that happened in the
-- player on the sky, and the stars someone followed a link out to hear.
--
-- Keyed by MusicBrainz id and not by the canon's own key, as the ledger is. The
-- canon's keys are renumbered by every rebuild; a person's progress has to
-- outlive that, which is the whole of ADR 0026's second half. An id the canon
-- no longer holds stays here rather than being deleted: it opens nothing today,
-- and the canon may hold it again next year.
CREATE TABLE star_opening (
    user_id     integer     NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    artist_mbid uuid        NOT NULL,
    opened_at   timestamptz NOT NULL DEFAULT now(),
    -- 'played': thirty seconds of the star's channel really sounded here.
    -- 'followed': a link out to hear them was followed, which is what there is
    --   for the half of the sky with no channel to embed.
    -- 'brought': opened in a browser before signing in. Only that browser said
    --   so.
    --
    -- Written because it cannot be worked out later: a sky whose first
    -- discoverers mean something will have to tell these apart.
    how         text        NOT NULL CHECK (how IN ('played', 'followed', 'brought')),
    PRIMARY KEY (user_id, artist_mbid)
);
