-- Imports recorded while the importers were being tried, in the founding
-- weeks: a Wikidata run over a test file of 86 entities, a live probe that
-- read nothing, and a MusicBrainz run over 21 rows. An edition is sealed from
-- the newest finished import of each source, so they never reached a canon,
-- but the stand's admin page lists every import and they read as history.
--
-- Matched on every value they hold, so a real import that happened to share a
-- version string is not touched. On a database that never had them this
-- deletes nothing.
DELETE FROM dump_import
WHERE (source, version, rows_imported) IN (
    ('wikidata', 'test-mini', 86),
    ('wikidata', 'live-probe', 0),
    ('musicbrainz', '20260815-002140', 21)
);
