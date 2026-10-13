//! The stars of someone's youth (idea 12): the music that came out while they
//! were fifteen to twenty.
//!
//! A question about the canon, not about a person: "who was putting records
//! out from 1998 to 2003" has the same answer for everyone, so it is asked by
//! the years and answered anonymously and cacheably. The birth year that turns
//! a person into those years stays in their profile, and the interface does
//! the subtraction.
//!
//! Who counts, measured on the full canon on 2026-10-13 rather than guessed:
//!
//! - an artist with an album, an EP or a single first released in those years
//!   -- compilations are a catalogue going round again, not music coming out;
//! - whose first record came no more than ten years before them. Release
//!   groups carry only a primary type here, so a reissue typed Album is
//!   indistinguishable from a new album; without this, the Beatles are the
//!   brightest star of every youth since 1970. Ten years keeps an act still
//!   making its name and lets go of one reissuing its catalogue.
//!
//! Brightest first, as the sky measures brightness.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::Serialize;
use sqlx::PgExecutor;
use time::OffsetDateTime;

use super::accounts::{bad_request, server_error};
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/youth/{from}", get(youth))
}

/// How many years a youth lasts: fifteen to twenty, both counted.
pub const YEARS: i16 = 6;

/// How long before the youth an artist's first record may be and still count.
const CAREER: i16 = 10;

/// How many stars a youth names: enough to ring a region of the sky, few
/// enough that each is someone the person might remember.
const STARS: i64 = 60;

/// The earliest youth the canon can answer for: release dates before the
/// twentieth century are a catalogue's guesses.
const EARLIEST: i16 = 1900;

#[derive(Serialize)]
struct Youth {
    from: i16,
    to: i16,
    stars: Vec<Star>,
}

#[derive(Serialize)]
struct Star {
    id: i32,
    name: String,
    x: f32,
    y: f32,
}

async fn youth(State(state): State<AppState>, Path(from): Path<i16>) -> Response {
    let now = OffsetDateTime::now_utc().year();
    if from < EARLIEST || i32::from(from) > now {
        return bad_request("that youth is not one the canon can answer for");
    }
    let to = from + YEARS - 1;

    match stars(&state.pool, from, to).await {
        Ok(stars) => (
            // The same for everyone until the canon changes, which is rarer
            // than daily: cached the way the tiles are.
            [(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"))],
            Json(Youth {
                from,
                to,
                stars: stars.into_iter().map(|(id, name, x, y)| Star { id, name, x, y }).collect(),
            }),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read the stars of a youth");
            server_error("the stars of that youth could not be read")
        }
    }
}

/// The stars of the years `from` to `to`, brightest first, as `(id, name, x,
/// y)` on the current sky.
async fn stars(db: impl PgExecutor<'_>, from: i16, to: i16) -> sqlx::Result<Vec<(i32, String, f32, f32)>> {
    sqlx::query_as::<_, (i32, String, f32, f32)>(
        "WITH layout AS (SELECT id, metric_id FROM sky_layout ORDER BY created_at DESC LIMIT 1),
         released AS (
             SELECT DISTINCT artist_id FROM release_group
             WHERE year BETWEEN $1 AND $2 AND primary_type IN ('Album', 'EP', 'Single')
         )
         SELECT a.id, a.name, p.x, p.y
         FROM released r
         JOIN artist a ON a.id = r.artist_id
         JOIN layout l ON true
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = l.id
         JOIN artist_prominence pr ON pr.artist_id = a.id AND pr.metric_id = l.metric_id
         WHERE NOT EXISTS (
             SELECT 1 FROM release_group e
             WHERE e.artist_id = a.id AND e.year < $1 - $3
               AND e.primary_type IN ('Album', 'EP', 'Single')
         )
         ORDER BY pr.weight DESC, a.id
         LIMIT $4",
    )
    .bind(from)
    .bind(to)
    .bind(CAREER)
    .bind(STARS)
    .fetch_all(db)
    .await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        routes().with_state(AppState {
            pool: PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(1))
                .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
                .expect("lazy pool creation does not touch the network"),
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
            dial: crate::api::listening::Dial::default(),
            listenbrainz: crate::scrobbling::Client::new("http://127.0.0.1:1"),
        })
    }

    #[tokio::test]
    async fn a_youth_outside_what_the_canon_can_answer_is_refused_before_the_database() {
        for from in ["1850", "3000", "nineteen"] {
            let response = app()
                .oneshot(Request::builder().uri(format!("/api/youth/{from}")).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{from}");
        }
    }

    #[tokio::test]
    async fn a_youth_the_canon_can_answer_reaches_the_database() {
        let response = app()
            .oneshot(Request::builder().uri("/api/youth/1998").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR, "the test's database is not there");
    }

    /// A name, how bright it is, and its records as (year, type).
    type Fixture<'a> = (&'a str, f32, &'a [(i16, &'a str)]);

    #[tokio::test]
    async fn a_star_of_a_youth_put_music_out_then_and_was_not_reissuing_an_old_catalogue() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the stars of a youth were NOT checked against a database");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        let mut tx = pool.begin().await.unwrap();

        let tag = rand::random::<u32>();
        let metric: i16 = sqlx::query_scalar("INSERT INTO similarity_metric (key, description) VALUES ($1, 'test') RETURNING id")
            .bind(format!("youth-{tag}"))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let layout: i16 = sqlx::query_scalar("INSERT INTO sky_layout (key, metric_id, description, seed, stars) VALUES ($1, $2, 'test', 1, 0) RETURNING id")
            .bind(format!("youth-{tag}"))
            .bind(metric)
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        let artists: [Fixture<'_>; 5] = [
            ("Arrived Then", 2.0, &[(1997, "Single"), (1999, "Album")]),
            ("Brighter Arrival", 5.0, &[(2001, "EP")]),
            ("Old Catalogue", 9.0, &[(1963, "Album"), (2000, "Album")]),
            ("Compiled", 7.0, &[(2000, "Compilation")]),
            ("Later On", 8.0, &[(2010, "Album")]),
        ];
        let mut next_group = rand::random_range(1_500_000_000..1_900_000_000);
        for (name, brightness, records) in artists {
            let id: i32 = rand::random_range(1_500_000_000..2_000_000_000);
            sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, $3, $3)")
                .bind(id)
                .bind(uuid::Uuid::from_u128(rand::random()))
                .bind(name)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES ($1, $2, 0, 0)")
                .bind(layout)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO artist_prominence (metric_id, artist_id, degree, weight) VALUES ($1, $2, 1, $3)")
                .bind(metric)
                .bind(id)
                .bind(brightness)
                .execute(&mut *tx)
                .await
                .unwrap();
            for (year, kind) in records {
                next_group += 1;
                sqlx::query("INSERT INTO release_group (id, mbid, name, primary_type, artist_id, year) VALUES ($1, $2, 'Record', $3, $4, $5)")
                    .bind(next_group)
                    .bind(uuid::Uuid::from_u128(rand::random()))
                    .bind(kind)
                    .bind(id)
                    .bind(year)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
        }

        let found = stars(&mut *tx, 1998, 2003).await.unwrap();
        let names: Vec<&str> = found.iter().map(|star| star.1.as_str()).collect();
        assert_eq!(
            names,
            vec!["Brighter Arrival", "Arrived Then"],
            "brightest first; no reissue, compilation or later act"
        );
        tx.rollback().await.unwrap();
    }
}
