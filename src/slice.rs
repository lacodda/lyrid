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
//! is kept, and the schema enforces the rest: all sixteen artist-referencing
//! tables cascade, so removing an artist removes their URLs, release groups,
//! genres, prose and edges without this code naming a single one of them.
//! Only `label` stands outside that graph, and it is dropped outright.

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::PgPool;
#[cfg(test)]
use sqlx::postgres::PgPoolOptions;

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

    // Ranked by graph weight rather than by anything resembling popularity:
    // connectivity is what the layout is built from, so keeping the most
    // connected artists is what keeps the shape of the sky (ADR 0004).
    let removed = sqlx::query(
        "DELETE FROM artist WHERE id NOT IN (
             SELECT p.artist_id
             FROM artist_position p
             LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id
             ORDER BY COALESCE(pr.weight, 0) DESC, p.artist_id
             LIMIT $1
         )",
    )
    .bind(args.keep)
    .execute(&mut *tx)
    .await
    .context("failed to remove artists outside the slice")?
    .rows_affected();

    // `label` is the one large table that does not reference an artist at all,
    // so no cascade reaches it. Nothing the map or the card serves reads a
    // label, and it is the third largest table in the canon.
    let labels = sqlx::query("DELETE FROM label")
        .execute(&mut *tx)
        .await
        .context("failed to remove labels")?
        .rows_affected();

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
}
