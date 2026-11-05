//! Cutting the canon down to a slice small enough for a modest stand.
//!
//! The full canon is roughly four gigabytes, most of it in tables the sky
//! never reads: URL relationships, release groups and labels together outweigh
//! everything the map and the card actually show. A stand sharing a small
//! machine with other services cannot hold that, and does not need to: the
//! brightest hundred thousand artists carry the overwhelming majority of the
//! similarity graph, so the sky above a slice looks like the sky above the
//! canon.
//!
//! This is deliberately a separate command rather than a flag on each
//! importer. Five importers filtering independently would be five chances for
//! the filters to disagree and leave a star with no edges or an edge with no
//! star. Importing in full and pruning once keeps a single definition of what
//! is kept.
//!
//! The tables to prune are read from the catalogue rather than listed here, so
//! a table added later is covered without this module being edited. They are
//! emptied in bulk before the artists themselves: `ON DELETE CASCADE` is
//! enforced by a per-row trigger, so deleting the artists first would fire
//! sixteen triggers for each of nearly three million rows and take hours.
//!
//! `label` references no artist, so no cascade from the artists reaches it.
//! A label stays when it still has a roster in the slice -- a station the
//! slice can open -- or owns one that does, so the line "part of Warner" on a
//! station still leads somewhere; the other two million go.

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::AssertSqlSafe;
#[cfg(test)]
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool};

#[derive(Parser)]
pub struct Args {
    /// How many artists to keep, brightest first.
    #[arg(long, default_value_t = 100_000)]
    pub keep: i64,

    /// Report what would be removed and change nothing.
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    if args.keep < 1 {
        bail!("--keep must be at least 1: a slice of nothing is an empty sky");
    }

    // Brightness comes from the layout, so a slice can only be cut once the
    // sky has been laid out. Saying so beats a foreign-key error.
    let placed: i64 = sqlx::query_scalar("SELECT count(*) FROM artist_position")
        .fetch_one(pool)
        .await
        .context("failed to count placed artists")?;
    if placed == 0 {
        bail!("no artist has a position yet: run `lyrid layout` before cutting a slice");
    }

    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM artist")
        .fetch_one(pool)
        .await
        .context("failed to count artists")?;

    if total <= args.keep {
        tracing::info!(artists = total, keep = args.keep, "the canon is already smaller than the slice");
        return Ok(());
    }

    if args.dry_run {
        tracing::info!(
            artists = total,
            keep = args.keep,
            would_remove = total - args.keep,
            "dry run: nothing was changed"
        );
        return Ok(());
    }

    // One transaction: a half-cut canon is worse than an uncut one, and a
    // stand that fails mid-prune should still have a sky to serve.
    let mut tx = pool.begin().await.context("failed to open a transaction")?;

    keep(&mut tx, args.keep).await?;

    // Without statistics the planner assumes this temporary table holds a
    // handful of rows and picks a nested loop for every prune below.
    sqlx::query("ANALYZE kept_artist")
        .execute(&mut *tx)
        .await
        .context("failed to analyse the table of kept artists")?;

    // Children are cleared in bulk before their parents, because a foreign key
    // marked ON DELETE CASCADE is enforced by a per-row trigger: deleting the
    // artists directly fires sixteen of them for each of nearly three million
    // rows. Measured at roughly 3-5 ms per artist, which is hours -- and the
    // stand this exists for is slower than the machine it was measured on.
    let pruned = prune_children(&mut tx, "artist", "kept_artist").await?;
    if pruned == 0 {
        bail!("no table references artist: refusing to cut a slice against an unexpected schema");
    }

    let removed = remove_outside(&mut tx, "artist", "kept_artist").await?;

    let labels = prune_labels(&mut tx).await?;

    tx.commit().await.context("failed to commit the slice")?;

    tracing::info!(
        removed_artists = removed,
        removed_labels = labels,
        kept = args.keep,
        "the canon was cut down to a slice"
    );
    tracing::info!("run `VACUUM FULL` to return the freed space to the filesystem");

    Ok(())
}

/// Chooses the artists the slice keeps, into `kept_artist`, and leaves the
/// slice one sky.
///
/// A stand shows the newest layout only, and a canon can hold several: the
/// sky of the 2020 graph is still there after v0.20 laid out the fresh one.
/// Choosing among every layout's positions would count a star once per layout
/// and keep stars of a sky nobody is shown -- so the older layouts go, and
/// the kept artists are the most listened of the stars the newest one places,
/// which are the brightest it draws (ADR 0024).
async fn keep(tx: &mut PgConnection, keep: i64) -> Result<()> {
    let layout: i16 = sqlx::query_scalar("SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1")
        .fetch_one(&mut *tx)
        .await
        .context("failed to find the newest layout")?;

    // Positions first, in one statement: their cascade from the layouts would
    // be the same delete, made by a trigger.
    let older = sqlx::query("DELETE FROM artist_position WHERE layout_id <> $1")
        .bind(layout)
        .execute(&mut *tx)
        .await
        .context("failed to remove the positions of older layouts")?
        .rows_affected();
    sqlx::query("DELETE FROM sky_layout WHERE id <> $1")
        .bind(layout)
        .execute(&mut *tx)
        .await
        .context("failed to remove older layouts")?;
    tracing::info!(positions = older, "older layouts removed: a stand shows one sky");

    sqlx::query("CREATE TEMPORARY TABLE kept_artist (id integer PRIMARY KEY) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .context("failed to create the table of kept artists")?;

    // Every position left is the newest sky's.
    sqlx::query(
        "INSERT INTO kept_artist (id)
         SELECT p.artist_id
         FROM artist_position p
         LEFT JOIN artist_listeners al ON al.artist_id = p.artist_id
         ORDER BY COALESCE(al.listeners, 0) DESC, p.artist_id
         LIMIT $1",
    )
    .bind(keep)
    .execute(&mut *tx)
    .await
    .context("failed to choose the artists to keep")?;
    Ok(())
}

/// Empties, in bulk, the rows of every table whose `ON DELETE CASCADE` key
/// into `parent` points outside `kept`, and returns how many such keys there
/// were.
///
/// The table list comes from the catalogue rather than from a list written
/// here, so a table added later is covered without this code being edited.
/// That was the property the cascade was providing, and it is worth keeping
/// once the cascade itself is no longer doing the work.
async fn prune_children(tx: &mut PgConnection, parent: &str, kept: &str) -> Result<usize> {
    let children: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname::text, a.attname::text
         FROM pg_constraint con
         JOIN pg_class c ON c.oid = con.conrelid
         JOIN pg_class f ON f.oid = con.confrelid
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY (con.conkey)
         WHERE con.contype = 'f' AND f.relname = $1 AND con.confdeltype = 'c'
         ORDER BY c.relname, a.attname",
    )
    .bind(parent)
    .fetch_all(&mut *tx)
    .await
    .with_context(|| format!("failed to read the tables referencing {parent}"))?;

    for (table, column) in &children {
        // These identifiers come from the catalogue rather than from any
        // input, but they are still checked before being interpolated: the
        // statement cannot be parameterised, so the guarantee should be
        // enforced here rather than asserted in a comment.
        if ![table.as_str(), column, kept].into_iter().all(is_plain_identifier) {
            bail!("refusing to prune `{table}`.`{column}`: not a plain identifier");
        }
        let statement = format!("DELETE FROM {table} WHERE {column} NOT IN (SELECT id FROM {kept})");
        // The statement is assembled rather than literal, so sqlx requires the
        // assertion below. It is honest here: every identifier in it comes
        // from the catalogue or from this module, and all were just checked
        // against `is_plain_identifier`. There is nothing a caller could
        // influence.
        let rows = sqlx::raw_sql(AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to prune {table}.{column}"))?
            .rows_affected();
        tracing::info!(table = %table, column = %column, removed = rows, "pruned");
    }
    Ok(children.len())
}

/// Deletes the rows of `parent` outside `kept`, once nothing points at them.
///
/// The cascades have nothing left to find by now, but their triggers still run:
/// one per foreign key per row, nineteen for each of 2.86 million artists, queued
/// in memory for the length of the transaction. That was fifty minutes, and enough
/// memory that the process running it was stopped for it on 2026-11-05. So the
/// triggers are switched off for this one statement, and the whole cut takes five
/// and a half. That is safe only because the catalogue's every key into `parent` is
/// asked first whether anything still points outside: a reference left behind stops
/// the slice, rather than becoming a dangling key nothing would ever report.
///
/// Switching them off takes more than owning the tables. Without that right the
/// delete runs with its triggers on: slower, and just as correct.
async fn remove_outside(tx: &mut PgConnection, parent: &str, kept: &str) -> Result<u64> {
    if ![parent, kept].into_iter().all(is_plain_identifier) {
        bail!("refusing to delete from `{parent}` by `{kept}`: not a plain identifier");
    }
    let delete = format!("DELETE FROM {parent} WHERE id NOT IN (SELECT id FROM {kept})");

    let may_switch: bool = sqlx::query_scalar("SELECT current_setting('is_superuser') = 'on' OR has_parameter_privilege('session_replication_role', 'SET')")
        .fetch_one(&mut *tx)
        .await
        .context("failed to ask whether triggers can be switched off")?;
    if !may_switch {
        tracing::warn!(
            table = parent,
            "this role cannot switch triggers off: deleting with them on, which is much slower"
        );
        return Ok(sqlx::raw_sql(AssertSqlSafe(delete))
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to remove {parent} outside the slice"))?
            .rows_affected());
    }

    nothing_points_outside(tx, parent, kept).await?;

    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .context("failed to switch triggers off")?;
    let removed = sqlx::raw_sql(AssertSqlSafe(delete))
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to remove {parent} outside the slice"))?
        .rows_affected();
    // Straight back on: what follows in this transaction relies on its cascades.
    sqlx::query("SET LOCAL session_replication_role = origin")
        .execute(&mut *tx)
        .await
        .context("failed to switch triggers back on")?;
    Ok(removed)
}

/// Refuses if any foreign key into `parent`, of whatever kind, still has a value
/// outside `kept`. From the catalogue, for the reason `prune_children` is: a table
/// added later is covered without this being edited.
async fn nothing_points_outside(tx: &mut PgConnection, parent: &str, kept: &str) -> Result<()> {
    let keys: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname::text, a.attname::text
         FROM pg_constraint con
         JOIN pg_class c ON c.oid = con.conrelid
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY (con.conkey)
         WHERE con.contype = 'f' AND con.confrelid = to_regclass($1)
         ORDER BY c.relname, a.attname",
    )
    .bind(parent)
    .fetch_all(&mut *tx)
    .await
    .with_context(|| format!("failed to read the keys into {parent}"))?;

    for (table, column) in &keys {
        if ![table.as_str(), column].into_iter().all(is_plain_identifier) {
            bail!("refusing to check `{table}`.`{column}`: not a plain identifier");
        }
        let question = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE {column} NOT IN (SELECT id FROM {kept}))");
        let row = sqlx::raw_sql(AssertSqlSafe(question))
            .fetch_one(&mut *tx)
            .await
            .with_context(|| format!("failed to check {table}.{column}"))?;
        let dangling: bool = sqlx::Row::try_get(&row, 0).with_context(|| format!("failed to read the check of {table}.{column}"))?;
        if dangling {
            bail!("{table}.{column} still points at {parent} outside the slice: refusing to delete under it");
        }
    }
    Ok(())
}

/// Keeps the labels a slice can still open, and returns how many went.
///
/// A label stays when it has a roster among the kept artists -- the pruning
/// of `label_artist` has already happened by now -- or when it owns, however
/// far up, one that does: a station's "part of" line should lead somewhere.
/// Everything else goes, which on a full canon is nearly all of 2.4 million.
async fn prune_labels(tx: &mut PgConnection) -> Result<u64> {
    sqlx::query("CREATE TEMPORARY TABLE kept_label (id integer PRIMARY KEY) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .context("failed to create the table of kept labels")?;

    // UNION rather than UNION ALL: it stops the walk at a label already seen,
    // so an ownership cycle in the dump ends instead of recursing forever.
    sqlx::query(
        "INSERT INTO kept_label (id)
         WITH RECURSIVE kept (id) AS (
             SELECT label_id FROM label_artist
             UNION
             SELECT l.parent_label_id FROM label l JOIN kept k ON k.id = l.id
             WHERE l.parent_label_id IS NOT NULL
         )
         SELECT id FROM kept",
    )
    .execute(&mut *tx)
    .await
    .context("failed to choose the labels to keep")?;

    sqlx::query("ANALYZE kept_label")
        .execute(&mut *tx)
        .await
        .context("failed to analyse the table of kept labels")?;

    // The chronology and whatever else hangs off a label, in bulk.
    prune_children(&mut *tx, "label", "kept_label").await?;

    // A label's owner is set to NULL when the owner goes -- a per-row trigger
    // again, and nearly every label that goes owns or is owned by another that
    // goes too. Cleared in one statement first, so the delete has nothing
    // left to update. No kept label points at a removed one: owners are kept.
    sqlx::query(
        "UPDATE label SET parent_label_id = NULL
         WHERE parent_label_id IS NOT NULL AND id NOT IN (SELECT id FROM kept_label)",
    )
    .execute(&mut *tx)
    .await
    .context("failed to detach the labels outside the slice")?;

    remove_outside(tx, "label", "kept_label").await
}

/// Whether a name is safe to interpolate into a statement unquoted: the
/// unremarkable `lower_snake_case` every table in this schema uses.
fn is_plain_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: Args,
    }

    fn parse(argv: &[&str]) -> Args {
        Cli::parse_from(argv).args
    }

    #[test]
    fn the_default_slice_is_a_hundred_thousand_and_changes_nothing_by_itself() {
        // The default is the measured one: a hundred thousand artists keep 92%
        // of the graph. It is also not destructive on its own -- cutting is
        // something someone asks for.
        let args = parse(&["lyrid"]);
        assert_eq!(args.keep, 100_000);
        assert!(!args.dry_run, "a slice should never default to having been run");
    }

    #[test]
    fn only_plain_identifiers_are_interpolated() {
        // The schema's own names pass.
        for name in ["artist_similarity", "source_id", "a", "x9_y"] {
            assert!(is_plain_identifier(name), "{name} should be accepted");
        }

        // Anything that could end a quoted name or carry a payload does not.
        // No such name can reach here from the catalogue today; the point is
        // that it could not do damage if one ever did.
        for name in ["", "Artist", "artist-url", "artist url", "artist\"", "a;DROP TABLE artist", "9lives"] {
            assert!(!is_plain_identifier(name), "{name:?} should be refused");
        }

        assert!(!is_plain_identifier(&"a".repeat(64)), "an over-long name should be refused");
    }

    #[tokio::test]
    async fn a_slice_of_nothing_is_refused_before_a_database_is_touched() {
        // Guarded here rather than left to the query, because `--keep 0` would
        // otherwise silently delete the entire canon.
        let pool = PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("a lazy pool does not touch the network");

        for keep in [0, -1] {
            let args = Args { keep, dry_run: false };
            let error = run(&pool, &args).await.expect_err("a slice of nothing should be refused");
            assert!(
                error.to_string().contains("--keep must be at least 1"),
                "the refusal should name the flag, got: {error}"
            );
        }
    }

    /// A parent and a child with a real foreign key, and a table saying which
    /// parents stay, in a schema of the test's own inside a transaction that is
    /// rolled back. A trigger that refuses every delete is on the parent, so a test
    /// can tell whether triggers were really off.
    async fn tiny_canon(conn: &mut PgConnection) {
        let schema = format!("cut_{:016x}", rand::random::<u64>());
        for statement in [
            format!("CREATE SCHEMA {schema}"),
            format!("SET LOCAL search_path TO {schema}, pg_temp"),
            "CREATE TABLE star (id integer PRIMARY KEY)".to_string(),
            "CREATE TABLE moon (star_id integer REFERENCES star (id) ON DELETE CASCADE)".to_string(),
            "CREATE TABLE kept_star (id integer PRIMARY KEY)".to_string(),
            "INSERT INTO star VALUES (1), (2), (3)".to_string(),
            "INSERT INTO kept_star VALUES (1)".to_string(),
            "INSERT INTO moon VALUES (1)".to_string(),
        ] {
            sqlx::raw_sql(AssertSqlSafe(statement)).execute(&mut *conn).await.unwrap();
        }
    }

    async fn refuse_deletes(conn: &mut PgConnection) {
        sqlx::raw_sql(
            "CREATE FUNCTION refuse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'a trigger ran'; END $$;
             CREATE TRIGGER refuse BEFORE DELETE ON star FOR EACH ROW EXECUTE FUNCTION refuse();",
        )
        .execute(conn)
        .await
        .unwrap();
    }

    async fn stars(conn: &mut PgConnection) -> Vec<i32> {
        sqlx::query_scalar("SELECT id FROM star ORDER BY id").fetch_all(conn).await.unwrap()
    }

    async fn test_pool() -> Option<PgPool> {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the delete of a slice was NOT checked against a database");
            return None;
        };
        Some(PgPoolOptions::new().max_connections(1).connect(&url).await.expect("test database"))
    }

    #[tokio::test]
    async fn the_delete_runs_with_triggers_off_and_turns_them_back_on() {
        let Some(pool) = test_pool().await else { return };
        let mut tx = pool.begin().await.unwrap();
        tiny_canon(&mut tx).await;
        refuse_deletes(&mut tx).await;

        // The refusing trigger would stop this if triggers were on.
        let removed = remove_outside(&mut tx, "star", "kept_star").await.expect("nothing points outside");
        assert_eq!(removed, 2);
        assert_eq!(stars(&mut tx).await, [1]);

        // And they are on again for whatever the transaction does next.
        let role: String = sqlx::query_scalar("SHOW session_replication_role").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(role, "origin");
        sqlx::raw_sql("SAVEPOINT back_on").execute(&mut *tx).await.unwrap();
        let refused = sqlx::raw_sql("DELETE FROM star WHERE id = 1").execute(&mut *tx).await;
        assert!(refused.is_err(), "a trigger should run again after the delete");
        sqlx::raw_sql("ROLLBACK TO SAVEPOINT back_on").execute(&mut *tx).await.unwrap();

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_reference_left_behind_stops_the_delete_rather_than_dangling() {
        let Some(pool) = test_pool().await else { return };
        let mut tx = pool.begin().await.unwrap();
        tiny_canon(&mut tx).await;
        sqlx::raw_sql("INSERT INTO moon VALUES (2)").execute(&mut *tx).await.unwrap();

        let error = remove_outside(&mut tx, "star", "kept_star").await.expect_err("a moon still points at star 2");
        assert!(error.to_string().contains("moon.star_id"), "the refusal should name the key, got: {error}");
        assert_eq!(stars(&mut tx).await, [1, 2, 3], "nothing was deleted");
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn every_kind_of_key_is_asked_and_not_only_the_cascading_ones() {
        let Some(pool) = test_pool().await else { return };
        let mut tx = pool.begin().await.unwrap();
        tiny_canon(&mut tx).await;
        sqlx::raw_sql("CREATE TABLE ring (star_id integer REFERENCES star (id) ON DELETE SET NULL); INSERT INTO ring VALUES (3);")
            .execute(&mut *tx)
            .await
            .unwrap();

        let error = remove_outside(&mut tx, "star", "kept_star").await.expect_err("a ring still points at star 3");
        assert!(error.to_string().contains("ring.star_id"), "got: {error}");
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_role_that_cannot_switch_triggers_off_deletes_with_them_on() {
        let Some(pool) = test_pool().await else { return };
        let mut tx = pool.begin().await.unwrap();
        tiny_canon(&mut tx).await;
        // Here a moon pointing outside is fine: its cascade is what removes it.
        sqlx::raw_sql("INSERT INTO moon VALUES (2)").execute(&mut *tx).await.unwrap();

        let schema: String = sqlx::query_scalar("SELECT current_schema()::text").fetch_one(&mut *tx).await.unwrap();
        let role = format!("cut_{:016x}", rand::random::<u64>());
        for statement in [
            format!("CREATE ROLE {role}"),
            format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
            format!("GRANT ALL ON ALL TABLES IN SCHEMA {schema} TO {role}"),
            format!("SET LOCAL ROLE {role}"),
        ] {
            sqlx::raw_sql(AssertSqlSafe(statement)).execute(&mut *tx).await.unwrap();
        }

        assert_eq!(remove_outside(&mut tx, "star", "kept_star").await.expect("slower, and just as correct"), 2);
        assert_eq!(stars(&mut tx).await, [1]);
        let moons: Vec<i32> = sqlx::query_scalar("SELECT star_id FROM moon").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(moons, [1], "the cascade ran");
        tx.rollback().await.unwrap();
    }

    /// The slice keeps the most listened stars of the newest sky, and that sky
    /// alone -- in a schema of the test's own, inside a transaction that is
    /// rolled back, because the test database is a working canon with layouts
    /// of its own.
    #[tokio::test]
    async fn keeps_the_most_listened_stars_of_the_newest_sky_alone() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the choice of kept stars was NOT checked against a database");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        let mut tx = pool.begin().await.unwrap();

        let schema = format!("slice_{:016x}", rand::random::<u64>());
        sqlx::raw_sql(AssertSqlSafe(format!("CREATE SCHEMA {schema}"))).execute(&mut *tx).await.unwrap();
        for table in ["sky_layout", "artist_position", "artist_listeners"] {
            sqlx::raw_sql(AssertSqlSafe(format!("CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)")))
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        sqlx::raw_sql(AssertSqlSafe(format!("SET LOCAL search_path TO {schema}")))
            .execute(&mut *tx)
            .await
            .unwrap();

        // The old sky places 1, 2 and 3; the new one 2, 3 and 4. Star 1 is the
        // most listened of all, but the sky being shown has no place for it.
        sqlx::raw_sql(AssertSqlSafe(
            "INSERT INTO sky_layout (id, key, metric_id, description, seed, created_at)
             VALUES (1, 'old', 1, 'old', 1, now() - interval '1 day'), (2, 'new', 2, 'new', 1, now());
             INSERT INTO artist_position (layout_id, artist_id, x, y)
             VALUES (1, 1, 0, 0), (1, 2, 0, 0), (1, 3, 0, 0), (2, 2, 1, 1), (2, 3, 1, 1), (2, 4, 1, 1);
             INSERT INTO artist_listeners (artist_id, listeners, listens)
             VALUES (1, 100, 100), (2, 5, 5), (3, 50, 50), (4, 20, 20);"
                .to_string(),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();

        keep(&mut tx, 2).await.expect("the choice runs");

        let kept: Vec<i32> = sqlx::query_scalar("SELECT id FROM kept_artist ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(kept, vec![3, 4], "the two most listened of the newest sky");
        let layouts: Vec<String> = sqlx::query_scalar("SELECT key FROM sky_layout").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(layouts, vec!["new".to_string()]);
        let positions: i64 = sqlx::query_scalar("SELECT count(*) FROM artist_position WHERE layout_id <> 2")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(positions, 0, "no position of an older sky is left behind");

        tx.rollback().await.unwrap();
    }

    /// Cuts the labels of a fixture canon inside a transaction that is rolled
    /// back. The rule is SQL -- a recursive walk up the owners -- so it runs
    /// against Postgres, and says so when there is none to run against.
    #[tokio::test]
    async fn keeps_the_labels_with_a_roster_and_everything_that_owns_them() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the label cut was NOT checked against a database");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        let mut tx = pool.begin().await.unwrap();

        // Ids far above anything Discogs or MusicBrainz issues.
        let artist = 2_000_000_301;
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, gen_random_uuid(), 'Fixture', 'Fixture')")
            .bind(artist)
            .execute(&mut *tx)
            .await
            .unwrap();

        // station -> owner -> grand owner; an unrelated pair that goes; and
        // an ownership cycle around a second station, which must end.
        let (station, owner, grand, gone, gone_owner, looped, loop_owner) = (
            2_000_000_001,
            2_000_000_002,
            2_000_000_003,
            2_000_000_004,
            2_000_000_005,
            2_000_000_006,
            2_000_000_007,
        );
        for id in [station, owner, grand, gone, gone_owner, looped, loop_owner] {
            sqlx::query("INSERT INTO label (id, name) VALUES ($1, 'Fixture label')")
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        for (child, parent) in [(station, owner), (owner, grand), (gone, gone_owner), (looped, loop_owner), (loop_owner, looped)] {
            sqlx::query("UPDATE label SET parent_label_id = $2 WHERE id = $1")
                .bind(child)
                .bind(parent)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        for label in [station, looped] {
            sqlx::query("INSERT INTO label_artist (label_id, artist_id, releases) VALUES ($1, $2, 1)")
                .bind(label)
                .bind(artist)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        for label in [station, gone] {
            sqlx::query("INSERT INTO label_year (label_id, year, releases) VALUES ($1, 1990, 3)")
                .bind(label)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        prune_labels(&mut tx).await.expect("the cut runs");

        let left: Vec<i32> = sqlx::query_scalar("SELECT id FROM label WHERE id >= 2000000001 ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(left, vec![station, owner, grand, looped, loop_owner]);
        let years: Vec<i32> = sqlx::query_scalar("SELECT label_id FROM label_year WHERE label_id >= 2000000001")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(years, vec![station], "the chronology goes with its label");

        tx.rollback().await.unwrap();
    }
}
