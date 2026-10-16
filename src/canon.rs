//! Editions of the canon: the sky, sealed once it is built and checked.
//!
//! The canon is rebuilt from dumps every few stages, and a rebuild empties
//! every table it owns before filling them again (`TRUNCATE ... CASCADE`).
//! Before this module nothing remembered the sky that was there before, so
//! "what changed" had no answer and "which sky is this" was a matter of
//! reading import timestamps.
//!
//! An edition is declared, not inferred: `lyrid canon seal` at the end of the
//! canon pipeline, once the layout is drawn and looked at. Sealing copies what
//! the edition is made of -- which import of each source, which layout -- and
//! the list of its stars with their names, and compares that list with the
//! edition before: the stars that are new, the ones MusicBrainz merged into
//! another (its redirects say so), and the ones that simply went dark. Nothing
//! here references the canon's tables, so the history outlives the canon it
//! describes. See ADR 0023.
//!
//! A slice is cut after sealing, from the sealed canon, so a stand carries the
//! edition of the canon it was cut from and can tell it holds part of it.

use anyhow::{Context, Result, bail};
use sqlx::{Connection, PgConnection, PgPool};

/// What sealing wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub number: i16,
    pub stars: i64,
    /// `None` for the first edition, which has nothing to be compared with.
    pub changes: Option<Changes>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    pub new: i64,
    pub merged: i64,
    pub gone: i64,
}

pub async fn run(pool: &PgPool) -> Result<()> {
    let mut conn = pool.acquire().await.context("failed to reach the database")?;
    let sealed = seal(&mut conn).await?;
    if let Some(changes) = sealed.changes {
        tracing::info!(
            edition = sealed.number,
            stars = sealed.stars,
            new = changes.new,
            merged = changes.merged,
            gone = changes.gone,
            "the sky was sealed as a new edition of the canon"
        );
    } else {
        tracing::info!(
            edition = sealed.number,
            stars = sealed.stars,
            "the sky was sealed as the first edition of the canon"
        );
    }
    Ok(())
}

/// Seals the current sky as the next edition.
///
/// One transaction: an edition is either whole -- sources, stars and changes
/// -- or not there at all.
pub async fn seal(conn: &mut PgConnection) -> Result<Sealed> {
    let mut tx = conn.begin().await.context("failed to open the sealing transaction")?;

    // The sky the server shows is the newest layout; that is what is sealed.
    let Some((layout_id, layout_key)) = sqlx::query_as::<_, (i16, String)>("SELECT id, key FROM sky_layout ORDER BY created_at DESC LIMIT 1")
        .fetch_optional(&mut *tx)
        .await
        .context("failed to read the current layout")?
    else {
        bail!("there is no layout to seal: run `lyrid layout` first");
    };

    // An edition is one sky. Sealing the same layout twice would make a
    // second edition with nothing changed, and progress keyed to either of
    // them would disagree about which one it is.
    if let Some(number) = sqlx::query_scalar::<_, i16>("SELECT id FROM canon_edition WHERE layout_key = $1")
        .bind(&layout_key)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to look for an edition of this layout")?
    {
        bail!("this sky ({layout_key}) is already edition {number}: lay the sky out again before sealing a new one");
    }

    let artists: i64 = sqlx::query_scalar("SELECT count(*) FROM artist")
        .fetch_one(&mut *tx)
        .await
        .context("failed to count the canon's artists")?;
    let stars: i64 = sqlx::query_scalar("SELECT count(*) FROM artist_position WHERE layout_id = $1")
        .bind(layout_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to count the sky's stars")?;
    if stars == 0 {
        bail!("the layout {layout_key} placed no stars: there is no sky to seal");
    }

    let previous: Option<i16> = sqlx::query_scalar("SELECT max(id) FROM canon_edition")
        .fetch_one(&mut *tx)
        .await
        .context("failed to find the edition before")?;

    let number: i16 = sqlx::query_scalar("INSERT INTO canon_edition (layout_key, artists, stars) VALUES ($1, $2, $3) RETURNING id")
        .bind(&layout_key)
        .bind(i32::try_from(artists).context("more artists than an edition can count")?)
        .bind(i32::try_from(stars).context("more stars than an edition can count")?)
        .fetch_one(&mut *tx)
        .await
        .context("failed to record the edition")?;

    // The newest finished import of each source. A source imported twice has
    // two records -- probes and reruns among them -- and only the last one is
    // what the canon now holds.
    sqlx::query(
        "INSERT INTO canon_edition_source (edition_id, source, version, imported_at, rows_imported)
         SELECT DISTINCT ON (source) $1, source, version, finished_at, rows_imported
         FROM dump_import
         WHERE finished_at IS NOT NULL
         ORDER BY source, finished_at DESC, id DESC",
    )
    .bind(number)
    .execute(&mut *tx)
    .await
    .context("failed to record the edition's sources")?;

    sqlx::query(
        "INSERT INTO canon_edition_star (edition_id, mbid, name)
         SELECT $1, a.mbid, a.name
         FROM artist_position p JOIN artist a ON a.id = p.artist_id
         WHERE p.layout_id = $2",
    )
    .bind(number)
    .bind(layout_id)
    .execute(&mut *tx)
    .await
    .context("failed to record the edition's stars")?;

    let changes = match previous {
        Some(previous) => Some(compare(&mut tx, previous, number).await?),
        None => None,
    };

    tx.commit().await.context("failed to commit the edition")?;
    Ok(Sealed { number, stars, changes })
}

/// Writes what changed between two editions, and counts it.
async fn compare(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, previous: i16, current: i16) -> Result<Changes> {
    let new = sqlx::query(
        "INSERT INTO canon_change (edition_id, mbid, change, name)
         SELECT $2, s.mbid, 'new', s.name
         FROM canon_edition_star s
         WHERE s.edition_id = $2
           AND NOT EXISTS (SELECT 1 FROM canon_edition_star o WHERE o.edition_id = $1 AND o.mbid = s.mbid)",
    )
    .bind(previous)
    .bind(current)
    .execute(&mut **tx)
    .await
    .context("failed to record the new stars")?
    .rows_affected();

    // A star of the edition before that is not a star now either merged into
    // another artist -- MusicBrainz leaves a redirect from its id -- or went
    // dark: deleted, or left without the similarity that placed it.
    sqlx::query(
        "INSERT INTO canon_change (edition_id, mbid, change, into_mbid, name)
         SELECT $2, o.mbid,
                CASE WHEN target.mbid IS NULL THEN 'gone' ELSE 'merged' END,
                target.mbid, o.name
         FROM canon_edition_star o
         LEFT JOIN LATERAL (
             SELECT a.mbid FROM artist_redirect r JOIN artist a ON a.id = r.artist_id WHERE r.mbid = o.mbid
         ) AS target ON true
         WHERE o.edition_id = $1
           AND NOT EXISTS (SELECT 1 FROM canon_edition_star s WHERE s.edition_id = $2 AND s.mbid = o.mbid)",
    )
    .bind(previous)
    .bind(current)
    .execute(&mut **tx)
    .await
    .context("failed to record the stars that left")?;

    let (merged, gone): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE change = 'merged'), count(*) FILTER (WHERE change = 'gone')
         FROM canon_change WHERE edition_id = $1",
    )
    .bind(current)
    .fetch_one(&mut **tx)
    .await
    .context("failed to count the stars that left")?;

    Ok(Changes {
        new: i64::try_from(new).unwrap_or(i64::MAX),
        merged,
        gone,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    use super::*;

    /// A pool on the test database, or `None` with a note when there is none.
    async fn database() -> Option<PgPool> {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the database half of the canon tests");
            return None;
        };
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("the test database is reachable");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        Some(pool)
    }

    /// A canon of its own, inside the test's transaction.
    ///
    /// Sealing reads the newest layout, the whole canon and the edition
    /// before, and the test database is a working canon with editions of its
    /// own. So every table the sealing touches is made again, empty, in a
    /// schema the transaction puts first on its search path -- and all of it
    /// rolls back with everything else.
    async fn world(conn: &mut PgConnection) -> sqlx::Result<()> {
        let schema = format!("seal_{:016x}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}"))).execute(&mut *conn).await?;
        for table in [
            "artist",
            "artist_redirect",
            "similarity_metric",
            "sky_layout",
            "artist_position",
            "dump_import",
            "canon_edition",
            "canon_edition_source",
            "canon_edition_star",
            "canon_change",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)"
            )))
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL search_path TO {schema}")))
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    async fn star(conn: &mut PgConnection, layout: i16, key: i32, mbid: Uuid, name: &str) {
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, $3, $3)")
            .bind(key)
            .bind(mbid)
            .bind(name)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES ($1, $2, 0, 0)")
            .bind(layout)
            .bind(key)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    async fn run(conn: &mut PgConnection, sql: &'static str) {
        sqlx::query(sql).execute(&mut *conn).await.unwrap();
    }

    #[tokio::test]
    async fn the_second_edition_says_which_stars_are_new_merged_and_gone() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        world(&mut tx).await.expect("a canon of the test's own");

        // Edition one: three stars.
        run(&mut tx, "INSERT INTO similarity_metric (id, key, description) VALUES (1, 'test', 'test')").await;
        run(
            &mut tx,
            "INSERT INTO sky_layout (id, key, metric_id, description, seed, created_at) VALUES (1, 'first', 1, 'test', 1, now() - interval '1 day')",
        )
        .await;
        star(&mut tx, 1, 1, id(1), "Kept").await;
        star(&mut tx, 1, 2, id(2), "Merged Away").await;
        star(&mut tx, 1, 3, id(3), "Gone Dark").await;
        run(
            &mut tx,
            "INSERT INTO dump_import (source, version, finished_at, rows_imported) VALUES ('musicbrainz', 'one', now() - interval '2 days', 3)",
        )
        .await;

        let first = seal(&mut tx).await.expect("the first sky seals");
        assert_eq!(first.stars, 3);
        assert_eq!(first.changes, None, "the first edition has nothing to differ from");

        // The same sky again is refused: one layout, one edition.
        let again = seal(&mut tx).await.unwrap_err();
        assert!(again.to_string().contains(&format!("already edition {}", first.number)), "{again}");

        // The canon is rebuilt: the second star is merged into the first, the
        // third is deleted, and a fourth arrives. A rebuild empties the
        // canon's tables, as the MusicBrainz import does.
        run(&mut tx, "TRUNCATE artist, artist_redirect, artist_position").await;
        run(
            &mut tx,
            "INSERT INTO sky_layout (id, key, metric_id, description, seed) VALUES (2, 'second', 1, 'test', 2)",
        )
        .await;
        star(&mut tx, 2, 1, id(1), "Kept").await;
        star(&mut tx, 2, 4, id(4), "Newcomer").await;
        sqlx::query("INSERT INTO artist_redirect (mbid, artist_id) VALUES ($1, 1)")
            .bind(id(2))
            .execute(&mut *tx)
            .await
            .unwrap();
        run(
            &mut tx,
            "INSERT INTO dump_import (source, version, finished_at, rows_imported) VALUES ('musicbrainz', 'two', now(), 2)",
        )
        .await;

        let second = seal(&mut tx).await.expect("the rebuilt sky seals");
        assert_eq!(second.number, first.number + 1);
        assert_eq!(second.changes, Some(Changes { new: 1, merged: 1, gone: 1 }));

        let changes: Vec<(Uuid, String, Option<Uuid>, String)> =
            sqlx::query_as("SELECT mbid, change, into_mbid, name FROM canon_change WHERE edition_id = $1 ORDER BY mbid")
                .bind(second.number)
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            changes,
            vec![
                (id(2), "merged".to_string(), Some(id(1)), "Merged Away".to_string()),
                (id(3), "gone".to_string(), None, "Gone Dark".to_string()),
                (id(4), "new".to_string(), None, "Newcomer".to_string()),
            ]
        );

        // The edition records the newest import of the source, not the first.
        let version: String = sqlx::query_scalar("SELECT version FROM canon_edition_source WHERE edition_id = $1 AND source = 'musicbrainz'")
            .bind(second.number)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(version, "two");

        tx.rollback().await.unwrap();
    }
}
