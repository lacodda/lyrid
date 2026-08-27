//! What the sky asks about a star.
//!
//! The map itself is static tiles and never touches the database, so these
//! endpoints serve only what a click or a search box needs. For the card that
//! means the whole canon meeting on one screen: the name and years from
//! `MusicBrainz`, genres from Discogs with a release count behind each, origin
//! and influence from Wikidata, the lead paragraphs from Wikipedia, and the
//! neighbours from co-listening.
//!
//! One rule here is not a matter of taste. Wikipedia prose arrives under
//! CC BY-SA, and its attribution is stored in the same row as the text; this
//! module keeps them together all the way to the wire, so a client cannot
//! receive the words without the credit they require.

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
    /// Where the act comes from, as Wikidata records it.
    origin: Option<Origin>,
    /// Labels the act has been signed to.
    labels: Vec<String>,
    /// Who shaped this artist, and who they went on to shape. Directed, so
    /// the two directions are different facts and are kept apart.
    influenced_by: Vec<Neighbour>,
    influenced: Vec<Neighbour>,
    /// The lead of the artist's Wikipedia article, with the credit its licence
    /// requires travelling in the same value.
    prose: Option<Prose>,
    /// Release groups, newest first.
    releases: Vec<Release>,
}

/// Where an act comes from, and which question that answers.
#[derive(Serialize)]
struct Origin {
    /// The place itself: "Seattle", "Liverpool".
    place: Option<String>,
    /// The country, when Wikidata records one separately.
    country: Option<String>,
    /// True when this is a person's birthplace rather than a group's place of
    /// formation. "Formed in Seattle" and "born in Seattle" are different
    /// claims and the card says which one it is showing.
    is_birth: bool,
    /// Wikidata's inception year, kept beside `MusicBrainz`'s own `begin_year`
    /// rather than replacing it: the two can disagree, and a curated value
    /// should not be silently overwritten by a crowdsourced one.
    inception_year: Option<i16>,
}

/// A Wikipedia lead, inseparable from its attribution.
///
/// Every field the licence requires is here because the row it came from
/// stores them together. Serialising the extract without them would need a
/// deliberate act, not an oversight.
#[derive(Serialize)]
struct Prose {
    extract: String,
    source_title: String,
    source_url: String,
    licence: String,
}

/// One release group, as a discography line.
#[derive(Serialize)]
struct Release {
    name: String,
    /// "Album", "Single", "EP"... from `MusicBrainz`'s primary type.
    primary_type: Option<String>,
    year: Option<i16>,
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
    //
    // The metric is pinned for the same reason as in `influences`: an edge is
    // keyed by (metric_id, source_id, target_id), and a second metric would
    // otherwise put the same neighbour in the list once per metric, on scores
    // that are not comparable across metrics anyway.
    let similar = sqlx::query_as::<_, (i32, String, f32)>(
        "SELECT other.id, other.name, e.score
         FROM artist_similarity e
         JOIN artist other ON other.id = CASE WHEN e.source_id = $1 THEN e.target_id ELSE e.source_id END
         WHERE (e.source_id = $1 OR e.target_id = $1)
           AND e.metric_id = (SELECT max(id) FROM similarity_metric)
         ORDER BY e.score DESC
         LIMIT 10",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, score)| Neighbour { id, name, score })
    .collect();

    let origin = origin(pool, id).await?;
    let labels = labels(pool, id).await?;
    let prose = prose(pool, id).await?;
    let releases = releases(pool, id).await?;

    // Influence is directed, so each direction is its own query rather than
    // one query over both columns: "shaped by" and "went on to shape" are
    // different claims, and collapsing them would invent symmetry Wikidata
    // never asserted. Ordered by brightness so the better-known names lead.
    let influenced_by = influences(pool, id, Direction::Sources).await?;
    let influenced = influences(pool, id, Direction::Targets).await?;

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
        origin,
        labels,
        influenced_by,
        influenced,
        prose,
        releases,
    }))
}

/// Where the act comes from, resolved from Wikidata item ids into words.
///
/// Wikidata stores a place as an item id, and the labels for those items were
/// captured during the same dump pass precisely so a card does not have to
/// reach back into a hundred gigabytes to say "Seattle".
async fn origin(pool: &PgPool, id: i32) -> sqlx::Result<Option<Origin>> {
    Ok(sqlx::query_as::<_, (Option<String>, Option<String>, Option<bool>, Option<i16>)>(
        "SELECT place.label, country.label, f.origin_is_birth, f.inception_year
         FROM artist_fact f
         LEFT JOIN wikidata_item place ON place.qid = f.origin_qid
         LEFT JOIN wikidata_item country ON country.qid = f.country_qid
         WHERE f.artist_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(|(place, country, is_birth, inception_year)| Origin {
        place,
        country,
        is_birth: is_birth.unwrap_or(false),
        inception_year,
    }))
}

async fn labels(pool: &PgPool, id: i32) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT item.label
         FROM artist_wikidata_label l
         JOIN wikidata_item item ON item.qid = l.label_qid
         WHERE l.artist_id = $1 AND item.label IS NOT NULL
         ORDER BY item.label
         LIMIT 8",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// The Wikipedia lead, selected together with the credit its licence requires.
///
/// The extract and the attribution come out of the row that stores them
/// together. There is no query in this codebase that can hand back the words
/// alone.
async fn prose(pool: &PgPool, id: i32) -> sqlx::Result<Option<Prose>> {
    Ok(
        sqlx::query_as::<_, (String, String, String, String)>("SELECT extract, source_title, source_url, licence FROM artist_prose WHERE artist_id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|(extract, source_title, source_url, licence)| Prose {
                extract,
                source_title,
                source_url,
                licence,
            }),
    )
}

/// The discography, ordered albums first and then oldest first.
///
/// That approximates "the records this artist is known for" from the only
/// fields the canon holds, and neither half is arbitrary. Newest-first looks
/// obvious and is wrong: the Beatles carry 696 release groups typed Album,
/// almost all reissues and compilations, so the newest twelve are 2025-2026
/// repackagings and Abbey Road is nowhere. Oldest-first surfaces the debut,
/// because compilations of a body of work can only come after it.
///
/// The honest limit: `MusicBrainz` separates a studio album from a live one
/// through *secondary* types, which the v0.2.0 import does not read, so
/// concert recordings typed Album still appear among the studio records.
/// Fixing that means re-importing the canon, which is a stage of its own.
async fn releases(pool: &PgPool, id: i32) -> sqlx::Result<Vec<Release>> {
    Ok(sqlx::query_as::<_, (String, Option<String>, Option<i16>)>(
        "SELECT name, primary_type, year
         FROM release_group
         WHERE artist_id = $1
         ORDER BY CASE primary_type
                    WHEN 'Album' THEN 0
                    WHEN 'EP' THEN 1
                    WHEN 'Single' THEN 2
                    ELSE 3
                  END,
                  year IS NULL, year, name
         LIMIT 12",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, primary_type, year)| Release { name, primary_type, year })
    .collect())
}

/// Which end of the influence arrow to follow.
#[derive(Clone, Copy)]
enum Direction {
    /// Who shaped this artist.
    Sources,
    /// Whom this artist shaped.
    Targets,
}

async fn influences(pool: &PgPool, id: i32, direction: Direction) -> sqlx::Result<Vec<Neighbour>> {
    // Two fixed statements rather than one with a swapped column pair: the
    // planner sees each one whole, and each uses the index built for its own
    // direction (the reverse one exists for exactly this query).
    //
    // Prominence is keyed by (metric_id, artist_id), so joining it without
    // naming a metric would return one row per metric and repeat every
    // influence in the list. Today there is a single metric and the bug would
    // be invisible; v0.17.1 adds a second one, at which point every card would
    // quietly show its influences twice. The subquery pins one metric now.
    let sql = match direction {
        Direction::Sources => {
            "SELECT a.id, a.name, coalesce(p.weight, 0)
             FROM artist_influence i
             JOIN artist a ON a.id = i.influence_id
             LEFT JOIN artist_prominence p
               ON p.artist_id = a.id
              AND p.metric_id = (SELECT max(id) FROM similarity_metric)
             WHERE i.artist_id = $1
             ORDER BY coalesce(p.weight, 0) DESC, a.name
             LIMIT 8"
        }
        Direction::Targets => {
            "SELECT a.id, a.name, coalesce(p.weight, 0)
             FROM artist_influence i
             JOIN artist a ON a.id = i.artist_id
             LEFT JOIN artist_prominence p
               ON p.artist_id = a.id
              AND p.metric_id = (SELECT max(id) FROM similarity_metric)
             WHERE i.influence_id = $1
             ORDER BY coalesce(p.weight, 0) DESC, a.name
             LIMIT 8"
        }
    };

    Ok(sqlx::query_as::<_, (i32, String, f32)>(sql)
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, name, score)| Neighbour { id, name, score })
        .collect())
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
