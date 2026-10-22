-- Empties the tables `stage-seed.sh --tables` is about to restore, on the
-- stand, in one transaction. Run by psql with `-v tables=a,b,c`.
--
-- Not one TRUNCATE: Postgres refuses to truncate a table that a table outside
-- the statement references, whatever either holds -- and the canon is
-- referenced from outside. A profile remembers the layout its camera was on
-- (`user_profile.layout_id`, ON DELETE SET NULL), so `sky_layout` cannot be
-- truncated under a stand with accounts, nor anything `sky_layout` points at.
--
-- Not one DELETE per table either: measured on a slice of 100,000 artists,
-- deleting `artist` after its children took 7 minutes 20 seconds, because the
-- foreign-key checks walk the dead rows the children's deletes left behind.
--
-- So the tables an outside table points at, and every listed table those
-- point at, are deleted -- children first, so the schema's ON DELETE actions
-- happen as written: the camera forgets a layout that is gone -- and the rest
-- are truncated in one statement, which leaves nothing to walk.
--
-- What the schema says is checked first. A table outside the list that would
-- lose rows with the canon (ON DELETE CASCADE) or stop it from going
-- (RESTRICT, NO ACTION) stops the script before anything is emptied: only a
-- reference that is set to NULL or to its default may point into the list.
\set ON_ERROR_STOP on

BEGIN;

-- Handed to the block below through a setting, since psql does not expand
-- its variables inside a dollar-quoted body; \gset keeps the echo quiet.
SELECT set_config('seed.tables', :'tables', true) AS seed_tables \gset

DO $$
DECLARE
    listed     text[] := string_to_array(current_setting('seed.tables'), ',');
    blocking   text;
    deleted    text[];
    truncated  text[];
    remaining  text[];
    t          text;
    progressed boolean;
    started    timestamptz;
BEGIN
    SELECT string_agg(format('%s.%s -> %s', c.relname, a.attname, f.relname), ', ' ORDER BY c.relname)
      INTO blocking
      FROM pg_constraint con
      JOIN pg_class c ON c.oid = con.conrelid
      JOIN pg_class f ON f.oid = con.confrelid
      JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY (con.conkey)
     WHERE con.contype = 'f'
       AND f.relnamespace = 'public'::regnamespace
       AND f.relname = ANY (listed)
       AND NOT c.relname = ANY (listed)
       AND con.confdeltype NOT IN ('n', 'd');
    IF blocking IS NOT NULL THEN
        RAISE EXCEPTION 'tables outside the list point into it and would not let go: %', blocking;
    END IF;

    -- Deleted: what an outside table points at, and, transitively, every
    -- listed table those point at -- a table that stays behind a TRUNCATE
    -- must not point at one inside it.
    WITH RECURSIVE kept_back (name) AS (
        SELECT f.relname::text
          FROM pg_constraint con
          JOIN pg_class c ON c.oid = con.conrelid
          JOIN pg_class f ON f.oid = con.confrelid
         WHERE con.contype = 'f'
           AND f.relnamespace = 'public'::regnamespace
           AND f.relname = ANY (listed)
           AND NOT c.relname = ANY (listed)
        UNION
        SELECT f.relname::text
          FROM kept_back k
          JOIN pg_class c ON c.relname = k.name AND c.relnamespace = 'public'::regnamespace
          JOIN pg_constraint con ON con.conrelid = c.oid AND con.contype = 'f'
          JOIN pg_class f ON f.oid = con.confrelid
         WHERE f.relname = ANY (listed)
    )
    SELECT coalesce(array_agg(name), '{}') INTO deleted FROM kept_back;
    truncated := ARRAY(SELECT unnest(listed) EXCEPT SELECT unnest(deleted));

    IF cardinality(truncated) > 0 THEN
        started := clock_timestamp();
        EXECUTE 'TRUNCATE ' || (SELECT string_agg(format('public.%I', x), ', ') FROM unnest(truncated) AS x);
        RAISE NOTICE 'truncated % tables in %', cardinality(truncated), clock_timestamp() - started;
    END IF;

    -- Children before parents: a table is emptied once no other table still
    -- waiting points at it.
    remaining := deleted;
    WHILE cardinality(remaining) > 0 LOOP
        progressed := false;
        FOREACH t IN ARRAY remaining LOOP
            IF NOT EXISTS (
                SELECT 1
                  FROM pg_constraint con
                  JOIN pg_class c ON c.oid = con.conrelid
                  JOIN pg_class f ON f.oid = con.confrelid
                 WHERE con.contype = 'f'
                   AND f.relname = t
                   AND c.relname <> t
                   AND c.relname = ANY (remaining)
            ) THEN
                started := clock_timestamp();
                EXECUTE format('DELETE FROM public.%I', t);
                RAISE NOTICE 'deleted the rows of % in %', t, clock_timestamp() - started;
                remaining := array_remove(remaining, t);
                progressed := true;
            END IF;
        END LOOP;
        IF NOT progressed THEN
            RAISE EXCEPTION 'the listed tables point at each other in a cycle: %', remaining;
        END IF;
    END LOOP;
END
$$;

COMMIT;
