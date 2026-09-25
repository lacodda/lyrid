-- The main genre and the main style of a set of artists, as one definition.
--
-- "Main" is the genre (and, separately, the style) carried by the most of an
-- artist's releases, ties going to the name that sorts first. The names on
-- the sky are built from it, the compass votes with it, and the radio of a
-- nebula plays the artists it names -- so the three must agree on who is in
-- "Soul", or the radio would play a sky the map does not show. Until now the
-- rule was written out as a DISTINCT ON in each query; a third copy was the
-- moment to make it one.
--
-- A function over a set of artists rather than a view over all of them, and
-- that is a measured choice. Ranking an artist's genres is cheap once the
-- artists are chosen and expensive before: every caller already has a small
-- set in hand -- the 300 stars the compass samples, the ten thousand with a
-- channel -- and a view cannot be told about it. On the 100,000-star slice, a
-- NOT EXISTS view made the compass 5x slower (400 ms against 70) and a
-- LATERAL view made cutting the names take 32 s; this takes 47 ms, 160 ms and
-- 0.85 s. Plain SQL and STABLE, so the planner inlines it into the caller.
CREATE FUNCTION main_genres(artists integer[])
RETURNS TABLE (artist_id integer, genre_id integer, is_style boolean)
LANGUAGE sql STABLE
AS $$
    SELECT DISTINCT ON (ag.artist_id, g.is_style) ag.artist_id, ag.genre_id, g.is_style
    FROM artist_genre ag
    JOIN genre g ON g.id = ag.genre_id
    WHERE ag.artist_id = ANY (artists)
    ORDER BY ag.artist_id, g.is_style, ag.releases DESC, g.name
$$;
