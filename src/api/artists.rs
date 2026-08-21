//! What the sky asks about a star.
//!
//! Deliberately thin: the map itself is static tiles and never touches the
//! database, so these endpoints serve only what a click or a search box needs
//! — a name, where it sits, and enough of the canon to recognise it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/artists/{id}", get(artist)).route("/api/search", get(search))
}

/// One artist, as a card shows them.
#[derive(Serialize)]
struct Artist {
    id: i32,
    mbid: uuid::Uuid,
    name: String,
    /// `MusicBrainz`'s disambiguation comment, which is what tells two acts of
    /// one name apart.
    comment: Option<String>,
    kind: Option<String>,
    area: Option<String>,
    begin_year: Option<i16>,
    end_year: Option<i16>,
    /// Where the star sits in the current layout, when it has a place.
    position: Option<Position>,
    /// Genres by weight, strongest first.
    genres: Vec<Genre>,
    /// Nearest neighbours in the similarity graph.
    similar: Vec<Neighbour>,
}

#[derive(Serialize)]
struct Position {
    x: f32,
    y: f32,
    /// How brightly the star is drawn: connectivity, not popularity.
    brightness: f32,
}

#[derive(Serialize)]
struct Genre {
    name: String,
    is_style: bool,
    releases: i32,
}

#[derive(Serialize)]
struct Neighbour {
    id: i32,
    name: String,
    score: f32,
}

async fn artist(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    match load_artist(&state.pool, id).await {
        Ok(Some(artist)) => (StatusCode::OK, Json(artist)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "no such artist" }))).into_response(),
        Err(error) => {
            tracing::error!(%error, artist = id, "failed to read an artist");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be read" })),
            )
                .into_response()
        }
    }
}

async fn load_artist(pool: &PgPool, id: i32) -> sqlx::Result<Option<Artist>> {
    let Some(row) = sqlx::query_as::<
        _,
        (
            i32,
            uuid::Uuid,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i16>,
            Option<i16>,
        ),
    >("SELECT id, mbid, name, comment, kind, area, begin_year, end_year FROM artist WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    // The newest layout is the one the client is looking at; an older one
    // would place the star somewhere the map does not show it.
    let position = sqlx::query_as::<_, (f32, f32, Option<f32>)>(
        "SELECT p.x, p.y, pr.weight
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE p.artist_id = $1
         ORDER BY l.created_at DESC
         LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(|(x, y, weight)| Position {
        x,
        y,
        brightness: weight.unwrap_or(0.0),
    });

    let genres = sqlx::query_as::<_, (String, bool, i32)>(
        "SELECT g.name, g.is_style, ag.releases
         FROM artist_genre ag JOIN genre g ON g.id = ag.genre_id
         WHERE ag.artist_id = $1
         ORDER BY ag.releases DESC, g.name
         LIMIT 8",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, is_style, releases)| Genre { name, is_style, releases })
    .collect();

    // Similarity is stored once per unordered pair, so neighbours come from
    // both columns.
    let similar = sqlx::query_as::<_, (i32, String, f32)>(
        "SELECT other.id, other.name, e.score
         FROM artist_similarity e
         JOIN artist other ON other.id = CASE WHEN e.source_id = $1 THEN e.target_id ELSE e.source_id END
         WHERE e.source_id = $1 OR e.target_id = $1
         ORDER BY e.score DESC
         LIMIT 10",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, score)| Neighbour { id, name, score })
    .collect();

    let (id, mbid, name, comment, kind, area, begin_year, end_year) = row;
    Ok(Some(Artist {
        id,
        mbid,
        name,
        comment,
        kind,
        area,
        begin_year,
        end_year,
        position,
        genres,
        similar,
    }))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

/// A search hit: enough to list it and to fly to it.
#[derive(Serialize)]
struct Hit {
    id: i32,
    name: String,
    comment: Option<String>,
    x: Option<f32>,
    y: Option<f32>,
}

async fn search(State(state): State<AppState>, Query(query): Query<SearchQuery>) -> Response {
    let term = query.q.trim();
    if term.len() < 2 {
        return (StatusCode::OK, Json(Vec::<Hit>::new())).into_response();
    }

    match run_search(&state.pool, term).await {
        Ok(hits) => (StatusCode::OK, Json(hits)).into_response(),
        Err(error) => {
            tracing::error!(%error, "search failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be searched" })),
            )
                .into_response()
        }
    }
}

async fn run_search(pool: &PgPool, term: &str) -> sqlx::Result<Vec<Hit>> {
    // Substring rather than prefix: an English band as famous as "The
    // Beatles" starts with an article, and a prefix search for "beatles"
    // would find a tribute act called "Beatless" and miss them entirely.
    //
    // Only among stars that have a place -- a result the map cannot fly to is
    // a dead end -- and ranked so the obvious answer leads: an exact name
    // first, then how woven into the graph the artist is. Connectivity beats
    // where the match falls, or searching "beatles" leads with a band called
    // "Beatless" simply because the word starts its name.
    let rows = sqlx::query_as::<_, (i32, String, Option<String>, Option<f32>, Option<f32>)>(
        "SELECT a.id, a.name, a.comment, p.x, p.y
         FROM artist a
         JOIN artist_position p ON p.artist_id = a.id
         LEFT JOIN artist_prominence pr ON pr.artist_id = a.id
         WHERE a.name ILIKE '%' || $1 || '%'
         ORDER BY
             (lower(a.name) = lower($1)) DESC,
             COALESCE(pr.weight, 0) DESC,
             (a.name ILIKE $1 || '%') DESC,
             length(a.name),
             a.name
         LIMIT 12",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(id, name, comment, x, y)| Hit { id, name, comment, x, y }).collect())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    /// A pool that never reaches a database, so routing and argument handling
    /// can be exercised without one.
    fn dead_pool() -> PgPool {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network")
    }

    fn app() -> Router {
        routes().with_state(AppState { pool: dead_pool() })
    }

    #[tokio::test]
    async fn a_short_search_term_answers_empty_without_touching_the_database() {
        // One letter would match a large share of three million artists, so
        // the query is refused before it is asked -- which is also why this
        // test can pass with no database behind it.
        let response = app().oneshot(Request::get("/api/search?q=a").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");
    }

    #[tokio::test]
    async fn a_search_without_a_term_is_a_bad_request_rather_than_a_crash() {
        let response = app().oneshot(Request::get("/api/search").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_unreachable_database_is_a_server_error_not_a_hang() {
        let response = app().oneshot(Request::get("/api/artists/1").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body.get("error").is_some(), "an error response should say so: {body}");
    }

    #[tokio::test]
    async fn a_non_numeric_artist_id_does_not_reach_the_database() {
        let response = app().oneshot(Request::get("/api/artists/nirvana").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
