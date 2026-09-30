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

    // The artists worth keeping, held for the duration. Ranked by graph weight
    // rather than by anything resembling popularity: connectivity is what the
    // layout is built from, so keeping the most connected artists is what
    // keeps the shape of the sky (ADR 0004).
    sqlx::query("CREATE TEMPORARY TABLE kept_artist (id integer PRIMARY KEY) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .context("failed to create the table of kept artists")?;

    sqlx::query(
        "INSERT INTO kept_artist (id)
         SELECT p.artist_id
         FROM artist_position p
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id
         ORDER BY COALESCE(pr.weight, 0) DESC, p.artist_id
         LIMIT $1",
    )
    .bind(args.keep)
    .execute(&mut *tx)
    .await
    .context("failed to choose the artists to keep")?;

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

    // By now the cascades have nothing left to find, so this is a plain delete.
    let removed = sqlx::query("DELETE FROM artist WHERE id NOT IN (SELECT id FROM kept_artist)")
        .execute(&mut *tx)
        .await
        .context("failed to remove artists outside the slice")?
        .rows_affected();

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

    let removed = sqlx::query("DELETE FROM label WHERE id NOT IN (SELECT id FROM kept_label)")
        .execute(&mut *tx)
        .await
        .context("failed to remove labels outside the slice")?
        .rows_affected();
    Ok(removed)
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
