//! What changed in the sky: the edition it is and what that edition changed,
//! the stars rising this week, and the releases out around now.
//!
//! One answer for one panel. Everything in it is about stars this sky can fly
//! to: a new star the slice left out, a riser with no place in the layout, a
//! release by somebody who is not a star here -- all of them are counted where
//! counts are given, and none of them is listed.

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::Serialize;
use sqlx::PgConnection;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

use super::PlacedStar;
use super::accounts::server_error;
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/sky/changes", get(changes))
}

/// How many stars of each kind are named. The panel is a glance, not a
/// catalogue: the counts say how many more there are.
const NAMED: i64 = 24;

/// How many risers are named.
const RISERS: i64 = 12;

/// How many releases are named.
const RELEASES: i64 = 24;

/// The days either side of today a release counts as out "now".
const RELEASE_DAYS: i32 = 7;

/// The newest layout: the sky the server shows.
const CURRENT_LAYOUT: &str = "(SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)";

#[derive(Serialize)]
struct Changes {
    edition: Option<Edition>,
    rising: Rising,
    releases: ReleaseWeek,
}

#[derive(Serialize)]
struct Edition {
    number: i16,
    sealed_at: String,
    stars: i32,
    /// Whether the sky on screen is this edition's. False when the sky was
    /// laid out again after the last seal: a state the canon pipeline should
    /// never leave behind, said rather than hidden.
    current: bool,
    sources: Vec<EditionSource>,
    /// The edition before, which the changes are counted against. `None` for
    /// the first edition, which changed nothing because nothing came before.
    previous: Option<Previous>,
    changed: Option<Changed>,
}

#[derive(Serialize)]
struct EditionSource {
    source: String,
    version: String,
    imported_at: String,
}

#[derive(Serialize)]
struct Previous {
    number: i16,
    sealed_at: String,
}

#[derive(Serialize)]
struct Changed {
    new: i64,
    merged: i64,
    gone: i64,
    /// The brightest of the new stars this sky holds.
    new_stars: Vec<PlacedStar>,
    /// Stars merged into another, by the name they had, and the star they are now.
    merges: Vec<Merge>,
    /// Stars gone dark, by the name they had.
    gone_names: Vec<String>,
}

#[derive(Serialize)]
struct Merge {
    from: String,
    into: PlacedStar,
}

#[derive(Serialize)]
struct Rising {
    /// The last day of the week the stars rose in; `None` until enough days
    /// are counted to call anything rising.
    through: Option<String>,
    /// How many days of listening have been counted, so a sky that is only
    /// starting to count can say so instead of saying nothing is rising.
    days_counted: i64,
    stars: Vec<Riser>,
}

#[derive(Serialize)]
struct Riser {
    #[serde(flatten)]
    star: PlacedStar,
    /// Listeners a day this week, and over the four weeks before.
    recent: f32,
    earlier: f32,
}

#[derive(Serialize)]
struct ReleaseWeek {
    from: String,
    to: String,
    releases: Vec<Release>,
}

#[derive(Serialize)]
struct Release {
    title: String,
    credit: String,
    kind: Option<String>,
    out_on: String,
    /// The brightest star the release credits.
    star: PlacedStar,
}

async fn changes(State(state): State<AppState>) -> Response {
    let mut conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "failed to reach the database for what changed in the sky");
            return server_error("what changed in the sky could not be read");
        }
    };
    match read(&mut conn).await {
        Ok(changes) => (
            // The feeds change it twice an hour at most, and the canon far
            // more rarely.
            [(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"))],
            Json(changes),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read what changed in the sky");
            server_error("what changed in the sky could not be read")
        }
    }
}

async fn read(conn: &mut PgConnection) -> sqlx::Result<Changes> {
    Ok(Changes {
        edition: edition(conn).await?,
        rising: rising(conn).await?,
        releases: releases(conn).await?,
    })
}

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_else(|_| at.to_string())
}

type PlacedRow = (i32, String, f32, f32);

fn placed((id, name, x, y): PlacedRow) -> PlacedStar {
    PlacedStar { id, name, x, y }
}

async fn edition(conn: &mut PgConnection) -> sqlx::Result<Option<Edition>> {
    let Some((number, sealed_at, stars, current)) = sqlx::query_as::<_, (i16, OffsetDateTime, i32, bool)>(sqlx::AssertSqlSafe(format!(
        "SELECT e.id, e.sealed_at, e.stars, e.layout_key IS NOT DISTINCT FROM (SELECT key FROM sky_layout WHERE id = {CURRENT_LAYOUT})
         FROM canon_edition e ORDER BY e.id DESC LIMIT 1"
    )))
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    let sources = sqlx::query_as::<_, (String, String, OffsetDateTime)>(
        "SELECT source, version, imported_at FROM canon_edition_source WHERE edition_id = $1 ORDER BY source",
    )
    .bind(number)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(source, version, imported_at)| EditionSource {
        source,
        version,
        imported_at: stamp(imported_at),
    })
    .collect();

    let previous = sqlx::query_as::<_, (i16, OffsetDateTime)>("SELECT id, sealed_at FROM canon_edition WHERE id < $1 ORDER BY id DESC LIMIT 1")
        .bind(number)
        .fetch_optional(&mut *conn)
        .await?
        .map(|(number, sealed_at)| Previous {
            number,
            sealed_at: stamp(sealed_at),
        });

    let changed = match previous {
        Some(_) => Some(changed(conn, number).await?),
        None => None,
    };

    Ok(Some(Edition {
        number,
        sealed_at: stamp(sealed_at),
        stars,
        current,
        sources,
        previous,
        changed,
    }))
}

async fn changed(conn: &mut PgConnection, edition: i16) -> sqlx::Result<Changed> {
    let (new_count, merged_count, gone_count): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE change = 'new'), count(*) FILTER (WHERE change = 'merged'), count(*) FILTER (WHERE change = 'gone')
         FROM canon_change WHERE edition_id = $1",
    )
    .bind(edition)
    .fetch_one(&mut *conn)
    .await?;

    let new_stars = sqlx::query_as::<_, PlacedRow>(sqlx::AssertSqlSafe(format!(
        "SELECT a.id, a.name, p.x, p.y
         FROM canon_change c
         JOIN artist a ON a.mbid = c.mbid
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = {CURRENT_LAYOUT}
         LEFT JOIN artist_listeners al ON al.artist_id = a.id
         WHERE c.edition_id = $1 AND c.change = 'new'
         ORDER BY coalesce(al.listeners, 0) DESC, a.id
         LIMIT $2"
    )))
    .bind(edition)
    .bind(NAMED)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(placed)
    .collect();

    let merges = sqlx::query_as::<_, (String, i32, String, f32, f32)>(sqlx::AssertSqlSafe(format!(
        "SELECT c.name, a.id, a.name, p.x, p.y
         FROM canon_change c
         JOIN artist a ON a.mbid = c.into_mbid
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = {CURRENT_LAYOUT}
         LEFT JOIN artist_listeners al ON al.artist_id = a.id
         WHERE c.edition_id = $1 AND c.change = 'merged'
         ORDER BY coalesce(al.listeners, 0) DESC, c.name
         LIMIT $2"
    )))
    .bind(edition)
    .bind(NAMED)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(from, id, name, x, y)| Merge {
        from,
        into: PlacedStar { id, name, x, y },
    })
    .collect();

    let gone_names = sqlx::query_scalar("SELECT name FROM canon_change WHERE edition_id = $1 AND change = 'gone' ORDER BY name LIMIT $2")
        .bind(edition)
        .bind(NAMED)
        .fetch_all(&mut *conn)
        .await?;

    Ok(Changed {
        new: new_count,
        merged: merged_count,
        gone: gone_count,
        new_stars,
        merges,
        gone_names,
    })
}

async fn rising(conn: &mut PgConnection) -> sqlx::Result<Rising> {
    let days_counted: i64 = sqlx::query_scalar("SELECT count(*) FROM listening_day WHERE closed_at IS NOT NULL AND covered_seconds > 0")
        .fetch_one(&mut *conn)
        .await?;
    let through: Option<Date> = sqlx::query_scalar("SELECT max(through) FROM star_rising").fetch_one(&mut *conn).await?;

    let stars = sqlx::query_as::<_, (i32, String, f32, f32, f32, f32)>(sqlx::AssertSqlSafe(format!(
        "SELECT a.id, a.name, p.x, p.y, r.recent, r.earlier
         FROM star_rising r
         JOIN artist a ON a.mbid = r.artist_mbid
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = {CURRENT_LAYOUT}
         ORDER BY r.rank
         LIMIT $1"
    )))
    .bind(RISERS)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(id, name, x, y, recent, earlier)| Riser {
        star: PlacedStar { id, name, x, y },
        recent,
        earlier,
    })
    .collect();

    Ok(Rising {
        through: through.map(|day| day.to_string()),
        days_counted,
        stars,
    })
}

async fn releases(conn: &mut PgConnection) -> sqlx::Result<ReleaseWeek> {
    let (from, to): (Date, Date) = sqlx::query_as("SELECT (now() AT TIME ZONE 'UTC')::date - $1, (now() AT TIME ZONE 'UTC')::date + $1")
        .bind(RELEASE_DAYS)
        .fetch_one(&mut *conn)
        .await?;

    // Each release once, under the brightest star it credits; then the
    // releases of the brightest stars first, newest first among equals.
    let releases = sqlx::query_as::<_, (String, String, Option<String>, Date, i32, String, f32, f32)>(sqlx::AssertSqlSafe(format!(
        "SELECT title, credit, primary_type, out_on, id, name, x, y FROM (
             SELECT DISTINCT ON (n.release_group_mbid)
                    n.title, n.credit, n.primary_type, n.out_on, a.id, a.name, p.x, p.y, coalesce(al.listeners, 0) AS listeners
             FROM new_release n
             JOIN artist a ON a.mbid = ANY (n.artist_mbids)
             JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = {CURRENT_LAYOUT}
             LEFT JOIN artist_listeners al ON al.artist_id = a.id
             WHERE n.out_on BETWEEN $1 AND $2
             ORDER BY n.release_group_mbid, coalesce(al.listeners, 0) DESC, a.id
         ) AS out
         ORDER BY listeners DESC, out_on DESC, title
         LIMIT $3"
    )))
    .bind(from)
    .bind(to)
    .bind(RELEASES)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(title, credit, kind, out_on, id, name, x, y)| Release {
        title,
        credit,
        kind,
        out_on: out_on.to_string(),
        star: PlacedStar { id, name, x, y },
    })
    .collect();

    Ok(ReleaseWeek {
        from: from.to_string(),
        to: to.to_string(),
        releases,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    use super::*;

    /// The tables the panel reads, empty, in a schema the test's transaction
    /// puts first on its search path: the test database is a working sky with
    /// an edition and weeks of weather of its own.
    async fn world(conn: &mut PgConnection) {
        let schema = format!("changes_{:016x}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&mut *conn)
            .await
            .unwrap();
        for table in [
            "artist",
            "similarity_metric",
            "sky_layout",
            "artist_position",
            "artist_listeners",
            "canon_edition",
            "canon_edition_source",
            "canon_change",
            "listening_day",
            "star_rising",
            "new_release",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)"
            )))
            .execute(&mut *conn)
            .await
            .unwrap();
        }
        sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL search_path TO {schema}")))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    async fn run(conn: &mut PgConnection, sql: &'static str) {
        sqlx::query(sql).execute(&mut *conn).await.unwrap();
    }

    /// A small sky and its weather: three artists, two editions, two risers,
    /// three releases. Returns Bright, Dim and Dark.
    async fn sky(tx: &mut PgConnection) -> (Uuid, Uuid, Uuid) {
        // Three artists: Bright and Dim are placed, Dark is in the canon with
        // no place in the sky.
        let bright = Uuid::from_u128(1);
        let dim = Uuid::from_u128(2);
        let dark = Uuid::from_u128(3);
        for (id, mbid, name) in [(1, bright, "Bright"), (2, dim, "Dim"), (3, dark, "Dark")] {
            sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, $3, $3)")
                .bind(id)
                .bind(mbid)
                .bind(name)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        run(&mut *tx, "INSERT INTO similarity_metric (id, key, description) VALUES (1, 'm', 'm')").await;
        run(
            &mut *tx,
            "INSERT INTO sky_layout (id, key, metric_id, description, seed) VALUES (1, 'sky', 1, 'd', 1)",
        )
        .await;
        run(
            &mut *tx,
            "INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES (1, 1, 1, 1), (1, 2, 2, 2)",
        )
        .await;
        run(
            &mut *tx,
            "INSERT INTO artist_listeners (artist_id, listeners, listens) VALUES (1, 9, 9), (2, 1, 1)",
        )
        .await;

        // Edition 2 over edition 1: Bright and Dark are new, Old merged into
        // Dim, Lost into Dark (which has no place), Faded gone dark.
        run(&mut *tx, "INSERT INTO canon_edition (id, layout_key, artists, stars, sealed_at) VALUES (1, 'old', 3, 2, now() - interval '30 days'), (2, 'sky', 3, 2, now())").await;
        run(
            &mut *tx,
            "INSERT INTO canon_edition_source (edition_id, source, version, imported_at) VALUES (2, 'musicbrainz', '20261014-150514', now())",
        )
        .await;
        sqlx::query(
            "INSERT INTO canon_change (edition_id, mbid, change, into_mbid, name) VALUES
             (2, $1, 'new', NULL, 'Bright'), (2, $3, 'new', NULL, 'Dark'),
             (2, $4, 'merged', $2, 'Old Name'), (2, $5, 'merged', $3, 'Lost Name'), (2, $6, 'gone', NULL, 'Faded')",
        )
        .bind(bright)
        .bind(dim)
        .bind(dark)
        .bind(Uuid::from_u128(4))
        .bind(Uuid::from_u128(5))
        .bind(Uuid::from_u128(6))
        .execute(&mut *tx)
        .await
        .unwrap();

        // Dark rises most, but has nowhere to be flown to.
        sqlx::query(
            "INSERT INTO star_rising (artist_mbid, rank, recent, earlier, through) VALUES
             ($1, 1, 40, 2, DATE '2026-10-15'), ($2, 2, 20, 5, DATE '2026-10-15')",
        )
        .bind(dark)
        .bind(dim)
        .execute(&mut *tx)
        .await
        .unwrap();

        // A release crediting Dim and Bright is listed once, under Bright; one
        // by Dark alone, and one a month old, are not listed.
        sqlx::query(
            "INSERT INTO new_release (release_group_mbid, title, credit, artist_mbids, primary_type, out_on) VALUES
             ($4, 'Together', 'Dim & Bright', ARRAY[$2, $1]::uuid[], 'Album', (now() AT TIME ZONE 'UTC')::date - 1),
             ($5, 'Alone', 'Dark', ARRAY[$3]::uuid[], 'Single', (now() AT TIME ZONE 'UTC')::date),
             ($6, 'Old News', 'Bright', ARRAY[$1]::uuid[], 'EP', (now() AT TIME ZONE 'UTC')::date - 30)",
        )
        .bind(bright)
        .bind(dim)
        .bind(dark)
        .bind(Uuid::from_u128(11))
        .bind(Uuid::from_u128(12))
        .bind(Uuid::from_u128(13))
        .execute(&mut *tx)
        .await
        .unwrap();

        (bright, dim, dark)
    }

    #[tokio::test]
    async fn the_panel_lists_only_what_this_sky_can_fly_to() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping what changed in the sky");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        world(&mut tx).await;

        sky(&mut tx).await;

        let changes = read(&mut tx).await.expect("the panel reads");

        let edition = changes.edition.expect("an edition is sealed");
        assert_eq!(edition.number, 2);
        assert!(edition.current, "the sky on screen is the sealed one");
        assert_eq!(edition.previous.map(|previous| previous.number), Some(1));
        let difference = edition.changed.expect("the second edition says what changed");
        assert_eq!(
            (difference.new, difference.merged, difference.gone),
            (2, 2, 1),
            "counts include what is not listed"
        );
        assert_eq!(difference.new_stars.iter().map(|star| star.name.as_str()).collect::<Vec<_>>(), ["Bright"]);
        assert_eq!(
            difference
                .merges
                .iter()
                .map(|merge| (merge.from.as_str(), merge.into.name.as_str()))
                .collect::<Vec<_>>(),
            [("Old Name", "Dim")]
        );
        assert_eq!(difference.gone_names, ["Faded"]);

        assert_eq!(changes.rising.through.as_deref(), Some("2026-10-15"));
        assert_eq!(changes.rising.stars.iter().map(|riser| riser.star.name.as_str()).collect::<Vec<_>>(), ["Dim"]);

        let releases: Vec<(&str, &str)> = changes
            .releases
            .releases
            .iter()
            .map(|release| (release.title.as_str(), release.star.name.as_str()))
            .collect();
        assert_eq!(releases, [("Together", "Bright")]);

        tx.rollback().await.unwrap();
    }
}
