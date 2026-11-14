-- The sky that remembers itself: an edition keeps where its stars were and
-- how bright each one was, and a new layout starts from there. See ADR 0029.

-- A frame is the coordinate system a layout is drawn in. A layout continued
-- from the edition before keeps that edition's frame -- its stars start where
-- they were and move little -- so a camera or a view link taken in it still
-- points at the same part of the sky. A layout drawn fresh, from the spiral,
-- starts a frame of its own, named by its key, and everything taken in the
-- old one stops pointing anywhere.
--
-- Every layout so far was drawn fresh, so each is a frame of its own.
ALTER TABLE sky_layout ADD COLUMN frame text;
UPDATE sky_layout SET frame = key;
ALTER TABLE sky_layout ALTER COLUMN frame SET NOT NULL;

-- A layout that names no frame was drawn fresh, and starts its own. Held by
-- the database rather than by each writer, so no writer can leave a layout
-- without a frame, and only the one that continues an edition has to say so.
CREATE FUNCTION sky_layout_own_frame() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.frame := coalesce(NEW.frame, NEW.key);
    RETURN NEW;
END;
$$;

CREATE TRIGGER sky_layout_own_frame
    BEFORE INSERT OR UPDATE ON sky_layout
    FOR EACH ROW EXECUTE FUNCTION sky_layout_own_frame();

-- The frame an edition was sealed in, copied like its layout's key: filling a
-- stand replaces the layouts under the editions.
ALTER TABLE canon_edition ADD COLUMN frame text;
UPDATE canon_edition SET frame = layout_key;
ALTER TABLE canon_edition ALTER COLUMN frame SET NOT NULL;

-- How far the stars this edition shares with the one before moved, in layout
-- units: the median, the ninetieth percentile and the furthest. Measured at
-- sealing, and only within one frame: across frames the numbers would compare
-- two unrelated pictures. NULL for the first edition and for a fresh frame.
-- The fog lights 14 units around an open star (ADR 0026), which is the scale
-- these are read against.
ALTER TABLE canon_edition
    ADD COLUMN drift_stars  integer CHECK (drift_stars >= 0),
    ADD COLUMN drift_median real    CHECK (drift_median >= 0),
    ADD COLUMN drift_p90    real    CHECK (drift_p90 >= 0),
    ADD COLUMN drift_max    real    CHECK (drift_max >= 0),
    ADD CONSTRAINT canon_edition_drift_whole
        CHECK ((drift_stars IS NULL) = (drift_median IS NULL)
           AND (drift_stars IS NULL) = (drift_p90 IS NULL)
           AND (drift_stars IS NULL) = (drift_max IS NULL));

-- Where each star of an edition was, and its place by brightness: 1 is the
-- brightest, in the order the tiles draw and the slice keeps -- the most
-- listened first, the canon's id breaking ties. The next layout starts from
-- the coordinates; "gone dark" names the brightest of the stars that left by
-- the place they had in the edition they left.
--
-- Nullable because the editions sealed before this one recorded neither.
-- Sealing always writes them from here on.
ALTER TABLE canon_edition_star
    ADD COLUMN x    real,
    ADD COLUMN y    real,
    ADD COLUMN rank integer CHECK (rank > 0),
    ADD CONSTRAINT canon_edition_star_placed CHECK ((x IS NULL) = (y IS NULL));

-- A camera is kept by the frame it was taken in rather than by the layout.
-- The layout is replaced by every new edition, and on a stand by every fill,
-- which set the old reference to NULL and dropped every camera even when the
-- sky had not moved. A frame is a name, so it outlives the layouts drawn in it.
ALTER TABLE user_profile ADD COLUMN camera_frame text;
UPDATE user_profile p SET camera_frame = l.frame FROM sky_layout l WHERE l.id = p.layout_id;
ALTER TABLE user_profile DROP COLUMN layout_id;
