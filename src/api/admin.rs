//! The stand's own state, for whoever runs it: which canon it holds, how its
//! feeds are doing, and how its accounts and mechanics are used.
//!
//! Counts only. The charter promises that nothing is kept which could become
//! a history of what a person did, and an admin page listing people would be
//! the first place to break that -- so this page is built from the same
//! aggregates anyone running the stand could reach with SQL, and names nobody.
//!
//! An admin is an account with the flag set from the server's command line
//! (`lyrid admin grant`). Anyone else is told no without being told why.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::Serialize;
use sqlx::PgPool;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

use super::accounts::server_error;
use super::scrobbling::{Refused, signed_in};
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/admin", get(overview))
}

/// How many runs of each feed are listed.
const RUNS: i64 = 12;

/// How many days of the listening feed's coverage are shown.
const DAYS: i32 = 35;

/// How many days of mechanic counts are shown.
const USE_DAYS: i32 = 14;

#[derive(Serialize)]
struct Overview {
    version: &'static str,
    canon: Canon,
    feeds: Feeds,
    accounts: Accounts,
    mechanics: Vec<MechanicUse>,
}

#[derive(Serialize)]
struct Canon {
    artists: i64,
    stars: i64,
    layout: Option<String>,
    laid_out_at: Option<String>,
    /// Whether this database holds fewer artists than the edition it carries:
    /// a slice of it, as a small stand is.
    slice: bool,
    editions: Vec<Edition>,
    imports: Vec<Import>,
}

#[derive(Serialize)]
struct Edition {
    number: i16,
    sealed_at: String,
    layout: String,
    artists: i32,
    stars: i32,
    new: i64,
    merged: i64,
    gone: i64,
}

#[derive(Serialize)]
struct Import {
    source: String,
    version: String,
    started_at: String,
    finished_at: Option<String>,
    rows: Option<i64>,
}

#[derive(Serialize)]
struct Feeds {
    listening: Listening,
    releases: ReleasesFeed,
}

#[derive(Serialize)]
struct Health {
    last_done: Option<String>,
    last_failed: Option<String>,
    failed_this_week: i64,
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct Run {
    source: String,
    started_at: String,
    finished_at: Option<String>,
    span_from: Option<String>,
    span_to: Option<String>,
    rows_read: Option<i64>,
    rows_kept: Option<i64>,
    error: Option<String>,
}

#[derive(Serialize)]
struct Listening {
    #[serde(flatten)]
    health: Health,
    days: Vec<Coverage>,
    /// Pairs waiting for their day to close.
    waiting: i64,
    /// Rows of counted listeners, and over how many days.
    counted: i64,
    rising_through: Option<String>,
    risers: i64,
}

#[derive(Serialize)]
struct Coverage {
    day: String,
    covered_seconds: i32,
    listeners: Option<i32>,
    closed: bool,
}

#[derive(Serialize)]
struct ReleasesFeed {
    #[serde(flatten)]
    health: Health,
    known: i64,
    out_this_week: i64,
}

#[derive(Serialize)]
struct Accounts {
    total: i64,
    confirmed: i64,
    joined_this_week: i64,
    joined_this_month: i64,
    admins: i64,
    sessions: i64,
    listenbrainz: ListenBrainz,
    lastfm: i64,
    letters: Letters,
}

#[derive(Serialize)]
struct ListenBrainz {
    linked: i64,
    reading_history: i64,
    failing: Vec<Failing>,
    last_read: Option<String>,
    listens: i64,
}

#[derive(Serialize)]
struct Failing {
    failure: String,
    links: i64,
}

#[derive(Serialize)]
struct Letters {
    subscribed: i64,
    last_week: Option<String>,
}

#[derive(Serialize)]
struct MechanicUse {
    mechanic: String,
    total: i64,
    days: Vec<DayUse>,
}

#[derive(Serialize)]
struct DayUse {
    day: String,
    uses: i64,
}

async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    match is_admin(&state.pool, user).await {
        Ok(true) => {}
        Ok(false) => return forbidden(),
        Err(error) => {
            tracing::error!(%error, "failed to read whether an account is an admin");
            return Refused::SessionUnreadable.into_response();
        }
    }
    match read(&state.pool).await {
        Ok(overview) => Json(overview).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read the stand's state");
            server_error("the stand's state could not be read")
        }
    }
}

pub(crate) async fn is_admin(pool: &PgPool, user: i32) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar("SELECT admin FROM app_user WHERE id = $1")
        .bind(user)
        .fetch_optional(pool)
        .await?
        .unwrap_or(false))
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "not an admin" }))).into_response()
}

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_else(|_| at.to_string())
}

async fn read(pool: &PgPool) -> sqlx::Result<Overview> {
    Ok(Overview {
        version: env!("CARGO_PKG_VERSION"),
        canon: canon(pool).await?,
        feeds: Feeds {
            listening: listening(pool).await?,
            releases: releases(pool).await?,
        },
        accounts: accounts(pool).await?,
        mechanics: mechanics(pool).await?,
    })
}

async fn canon(pool: &PgPool) -> sqlx::Result<Canon> {
    let artists: i64 = sqlx::query_scalar("SELECT count(*) FROM artist").fetch_one(pool).await?;
    let layout = sqlx::query_as::<_, (i16, String, OffsetDateTime)>("SELECT id, key, created_at FROM sky_layout ORDER BY created_at DESC LIMIT 1")
        .fetch_optional(pool)
        .await?;
    let stars: i64 = match &layout {
        Some((id, _, _)) => {
            sqlx::query_scalar("SELECT count(*) FROM artist_position WHERE layout_id = $1")
                .bind(id)
                .fetch_one(pool)
                .await?
        }
        None => 0,
    };

    let editions: Vec<Edition> = sqlx::query_as::<_, (i16, OffsetDateTime, String, i32, i32, i64, i64, i64)>(
        "SELECT e.id, e.sealed_at, e.layout_key, e.artists, e.stars,
                count(c.mbid) FILTER (WHERE c.change = 'new'),
                count(c.mbid) FILTER (WHERE c.change = 'merged'),
                count(c.mbid) FILTER (WHERE c.change = 'gone')
         FROM canon_edition e LEFT JOIN canon_change c ON c.edition_id = e.id
         GROUP BY e.id ORDER BY e.id DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(number, sealed_at, layout, artists, stars, new, merged, gone)| Edition {
        number,
        sealed_at: stamp(sealed_at),
        layout,
        artists,
        stars,
        new,
        merged,
        gone,
    })
    .collect();

    let imports = sqlx::query_as::<_, (String, String, OffsetDateTime, Option<OffsetDateTime>, Option<i64>)>(
        "SELECT source, version, started_at, finished_at, rows_imported FROM dump_import ORDER BY started_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(source, version, started_at, finished_at, rows)| Import {
        source,
        version,
        started_at: stamp(started_at),
        finished_at: finished_at.map(stamp),
        rows,
    })
    .collect();

    let slice = editions.first().is_some_and(|edition| i64::from(edition.artists) > artists);
    Ok(Canon {
        artists,
        stars,
        layout: layout.as_ref().map(|(_, key, _)| key.clone()),
        laid_out_at: layout.map(|(_, _, at)| stamp(at)),
        slice,
        editions,
        imports,
    })
}

type RunRow = (
    String,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

async fn health(pool: &PgPool, feed: &str) -> sqlx::Result<Health> {
    let (last_done, last_failed, failed_this_week): (Option<OffsetDateTime>, Option<OffsetDateTime>, i64) = sqlx::query_as(
        "SELECT max(finished_at) FILTER (WHERE error IS NULL),
                max(finished_at) FILTER (WHERE error IS NOT NULL),
                count(*) FILTER (WHERE error IS NOT NULL AND started_at > now() - interval '7 days')
         FROM feed_run WHERE feed = $1",
    )
    .bind(feed)
    .fetch_one(pool)
    .await?;
    let runs = sqlx::query_as::<_, RunRow>(
        "SELECT source, started_at, finished_at, span_from, span_to, rows_read, rows_kept, error
         FROM feed_run WHERE feed = $1 ORDER BY started_at DESC, id DESC LIMIT $2",
    )
    .bind(feed)
    .bind(RUNS)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(source, started_at, finished_at, span_from, span_to, rows_read, rows_kept, error)| Run {
        source,
        started_at: stamp(started_at),
        finished_at: finished_at.map(stamp),
        span_from: span_from.map(stamp),
        span_to: span_to.map(stamp),
        rows_read,
        rows_kept,
        error,
    })
    .collect();
    Ok(Health {
        last_done: last_done.map(stamp),
        last_failed: last_failed.map(stamp),
        failed_this_week,
        runs,
    })
}

async fn listening(pool: &PgPool) -> sqlx::Result<Listening> {
    let days = sqlx::query_as::<_, (Date, i32, Option<i32>, bool)>(
        "SELECT day, covered_seconds, listeners, closed_at IS NOT NULL
         FROM listening_day ORDER BY day DESC LIMIT $1",
    )
    .bind(i64::from(DAYS))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(day, covered_seconds, listeners, closed)| Coverage {
        day: day.to_string(),
        covered_seconds,
        listeners,
        closed,
    })
    .collect();
    let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM listener_seen").fetch_one(pool).await?;
    let counted: i64 = sqlx::query_scalar("SELECT count(*) FROM artist_day").fetch_one(pool).await?;
    let (rising_through, risers): (Option<Date>, i64) = sqlx::query_as("SELECT max(through), count(*) FROM star_rising").fetch_one(pool).await?;
    Ok(Listening {
        health: health(pool, "listening").await?,
        days,
        waiting,
        counted,
        rising_through: rising_through.map(|day| day.to_string()),
        risers,
    })
}

async fn releases(pool: &PgPool) -> sqlx::Result<ReleasesFeed> {
    let (known, out_this_week): (i64, i64) = sqlx::query_as(
        "SELECT count(*),
                count(*) FILTER (WHERE out_on BETWEEN (now() AT TIME ZONE 'UTC')::date - 6 AND (now() AT TIME ZONE 'UTC')::date)
         FROM new_release",
    )
    .fetch_one(pool)
    .await?;
    Ok(ReleasesFeed {
        health: health(pool, "releases").await?,
        known,
        out_this_week,
    })
}

async fn accounts(pool: &PgPool) -> sqlx::Result<Accounts> {
    let (total, confirmed, joined_this_week, joined_this_month, admins): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),
                count(*) FILTER (WHERE email_confirmed_at IS NOT NULL),
                count(*) FILTER (WHERE created_at > now() - interval '7 days'),
                count(*) FILTER (WHERE created_at > now() - interval '30 days'),
                count(*) FILTER (WHERE admin)
         FROM app_user",
    )
    .fetch_one(pool)
    .await?;
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM user_session WHERE expires_at > now()")
        .fetch_one(pool)
        .await?;

    let (linked, reading_history, last_read): (i64, i64, Option<OffsetDateTime>) = sqlx::query_as(
        "SELECT count(*),
                count(*) FILTER (WHERE read_through IS NULL OR read_through < linked_at),
                max(read_at)
         FROM listenbrainz_link",
    )
    .fetch_one(pool)
    .await?;
    let failing =
        sqlx::query_as::<_, (String, i64)>("SELECT failure, count(*) FROM listenbrainz_link WHERE failure IS NOT NULL GROUP BY failure ORDER BY failure")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(failure, links)| Failing { failure, links })
            .collect();
    let (listens, lastfm): (i64, i64) =
        sqlx::query_as("SELECT count(*) FILTER (WHERE source = 'listenbrainz'), count(DISTINCT user_id) FILTER (WHERE source = 'lastfm') FROM listen")
            .fetch_one(pool)
            .await?;
    let (subscribed, last_week): (i64, Option<Date>) = sqlx::query_as("SELECT count(*), max(sent_week) FROM weekly_letter").fetch_one(pool).await?;

    Ok(Accounts {
        total,
        confirmed,
        joined_this_week,
        joined_this_month,
        admins,
        sessions,
        listenbrainz: ListenBrainz {
            linked,
            reading_history,
            failing,
            last_read: last_read.map(stamp),
            listens,
        },
        lastfm,
        letters: Letters {
            subscribed,
            last_week: last_week.map(|week| week.to_string()),
        },
    })
}

async fn mechanics(pool: &PgPool) -> sqlx::Result<Vec<MechanicUse>> {
    let rows = sqlx::query_as::<_, (String, Date, i64)>(
        "SELECT mechanic, day, uses FROM mechanic_use
         WHERE day > (now() AT TIME ZONE 'UTC')::date - $1
         ORDER BY mechanic, day",
    )
    .bind(USE_DAYS)
    .fetch_all(pool)
    .await?;
    let mut mechanics: Vec<MechanicUse> = Vec::new();
    for (mechanic, day, uses) in rows {
        if mechanics.last().is_none_or(|last| last.mechanic != mechanic) {
            mechanics.push(MechanicUse {
                mechanic,
                total: 0,
                days: Vec::new(),
            });
        }
        if let Some(last) = mechanics.last_mut() {
            last.total += uses;
            last.days.push(DayUse { day: day.to_string(), uses });
        }
    }
    mechanics.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.mechanic.cmp(&b.mechanic)));
    Ok(mechanics)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    fn app(pool: PgPool) -> Router {
        routes().with_state(AppState {
            pool,
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
            dial: crate::api::listening::Dial::default(),
            listenbrainz: crate::scrobbling::Client::new("http://127.0.0.1:1"),
        })
    }

    async fn ask(pool: &PgPool, token: Option<&str>) -> (StatusCode, String) {
        let mut request = Request::get("/api/admin");
        if let Some(token) = token {
            request = request.header("cookie", format!("{}={token}", crate::auth::COOKIE));
        }
        let response = app(pool.clone()).oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn only_an_admin_is_shown_the_stand_and_nobody_is_named_to_them() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the admin page's access test");
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("the test database is reachable");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");

        // The handler reads through the pool, so the account is committed --
        // under an address nobody else has -- and removed at the end, which
        // takes its session with it.
        let email = format!("admin-{:016x}@lyrid.test", rand::random::<u64>());
        let token = format!("admin-test-{:016x}", rand::random::<u64>());
        let user: i32 = sqlx::query_scalar("INSERT INTO app_user (email, password_hash) VALUES ($1, 'x') RETURNING id")
            .bind(&email)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO user_session (token, user_id, expires_at) VALUES ($1, $2, now() + interval '1 hour')")
            .bind(&token)
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();

        let unsigned = ask(&pool, None).await;
        let refused = ask(&pool, Some(&token)).await;
        sqlx::query("UPDATE app_user SET admin = true WHERE id = $1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        let shown = ask(&pool, Some(&token)).await;
        sqlx::query("DELETE FROM app_user WHERE id = $1").bind(user).execute(&pool).await.unwrap();

        assert_eq!(unsigned.0, StatusCode::UNAUTHORIZED);
        assert_eq!(refused.0, StatusCode::FORBIDDEN, "an account that is not an admin is shown nothing");
        assert!(!refused.1.contains("canon"), "a refusal carries nothing of the stand: {}", refused.1);
        assert_eq!(shown.0, StatusCode::OK, "{}", shown.1);
        let overview: serde_json::Value = serde_json::from_str(&shown.1).unwrap();
        assert!(overview["accounts"]["admins"].as_i64().unwrap() >= 1);
        // Counts only: no address, the admin's own included, is anywhere in it.
        assert!(!shown.1.contains('@'), "the overview names somebody: {}", shown.1);
    }
}
