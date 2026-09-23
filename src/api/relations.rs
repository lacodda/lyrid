//! What two stars have to do with each other.
//!
//! The sky puts two artists side by side because people listen to them
//! together, and that is all a line between two stars says. It is a number, and
//! a number is not knowledge. This module turns an edge into the three reasons
//! the canon can actually give for it:
//!
//! - **genres they share**, from Discogs, weighted by releases;
//! - **co-listening**, the edge itself, from `ListenBrainz`;
//! - **influence**, from Wikidata, with its direction kept.
//!
//! The same reasons serve two screens -- the neighbours on a card, and the
//! comparison of any two stars -- and are computed in one place so the two can
//! never explain the same pair differently.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};

use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/compare", get(compare))
}

/// Why two stars are near each other, in the canon's own words.
#[derive(Serialize, Default, Clone, PartialEq, Debug)]
pub struct Why {
    /// Genres and styles both carry, the most shared first. At most three: a
    /// reason is read at a glance, and the fourth shared style is never the one
    /// that explains anything.
    pub genres: Vec<String>,
    /// The co-listening score of the edge between them, when there is one.
    pub co_listening: Option<f32>,
    /// Whether one shaped the other, read from the first star's side.
    pub influence: Option<Influence>,
}

/// An influence between two stars, from the first one's point of view.
///
/// Three values rather than two booleans: "each shaped the other" is a fact
/// Wikidata does record, and it is a different sentence from either direction.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Influence {
    /// The other star shaped this one.
    ShapedBy,
    /// This star shaped the other.
    WentOnToShape,
    /// Both, which happens between contemporaries.
    Mutual,
}

/// How many shared genres a reason names.
const SHARED_GENRES: usize = 3;

/// The least share of *each* discography a genre must hold to count as shared.
///
/// A tenth: enough that the genre is part of what the artist is, low enough
/// that an act spread over five styles still shares its real ones.
const SHARED_FLOOR: f32 = 0.1;

/// The reasons `artist` is near each of `others`, keyed by the other's id.
///
/// Two queries for the whole list rather than two per neighbour: a card names
/// ten neighbours, and twenty round trips to open one card would be the
/// slowest thing on the screen.
///
/// Takes a connection rather than the pool so a test can run it inside a
/// transaction over rows of its own and roll them back.
pub async fn why_many(conn: &mut PgConnection, artist: i32, others: &[i32]) -> sqlx::Result<HashMap<i32, Why>> {
    if others.is_empty() {
        return Ok(HashMap::new());
    }

    // Styles before genres, because a shared style says more: "both Soul" tells
    // two acts apart from the rest of the sky, "both Funk / Soul" barely does.
    // Within each, shares of each artist's own discography, not release
    // counts, ranked by the smaller of the two: a genre is shared only as much as the one who
    // carries it less. Below a floor it is not shared at all. Found on the
    // card: Marvin Gaye and Nirvana came out as "both Electronic, Stage &
    // Screen, Electro" -- true of a remix and a soundtrack each, and a false
    // account of what the two have in common.
    let genres: Vec<(i32, String, f32)> = sqlx::query_as(
        "WITH shares AS (
             SELECT ag.artist_id, ag.genre_id,
                    (ag.releases::real / nullif(sum(ag.releases) OVER (PARTITION BY ag.artist_id, g.is_style), 0))::real AS share
             FROM artist_genre ag
             JOIN genre g ON g.id = ag.genre_id
             WHERE ag.artist_id = $1 OR ag.artist_id = ANY($2)
         )
         SELECT other.artist_id, g.name, least(me.share, other.share)
         FROM shares me
         JOIN shares other ON other.genre_id = me.genre_id AND other.artist_id = ANY($2)
         JOIN genre g ON g.id = me.genre_id
         WHERE me.artist_id = $1 AND least(me.share, other.share) >= $3
         ORDER BY other.artist_id, g.is_style DESC, least(me.share, other.share) DESC, g.name",
    )
    .bind(artist)
    .bind(others)
    .bind(SHARED_FLOOR)
    .fetch_all(&mut *conn)
    .await?;

    let arrows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT artist_id, influence_id FROM artist_influence
         WHERE (artist_id = $1 AND influence_id = ANY($2))
            OR (influence_id = $1 AND artist_id = ANY($2))",
    )
    .bind(artist)
    .bind(others)
    .fetch_all(&mut *conn)
    .await?;

    let mut out: HashMap<i32, Why> = others.iter().map(|&id| (id, Why::default())).collect();
    for (other, name, _) in genres {
        if let Some(why) = out.get_mut(&other) {
            push_genre(&mut why.genres, name);
        }
    }
    for (id, why) in &mut out {
        why.influence = influence_between(artist, *id, &arrows);
    }
    Ok(out)
}

/// Adds a shared genre unless the list is full or already names it.
///
/// A name can arrive twice: Discogs uses "Blues" as a genre and as a style,
/// and both rows join. Saying "Blues, Blues" would read as a bug.
fn push_genre(genres: &mut Vec<String>, name: String) {
    if genres.len() < SHARED_GENRES && !genres.contains(&name) {
        genres.push(name);
    }
}

/// The influence between `me` and `other`, if any, from `me`'s side.
///
/// `arrows` are `(influenced, influence)` pairs as `artist_influence` stores
/// them; pairs about anyone else are ignored.
fn influence_between(me: i32, other: i32, arrows: &[(i32, i32)]) -> Option<Influence> {
    let shaped_me = arrows.contains(&(me, other));
    let i_shaped = arrows.contains(&(other, me));
    match (shaped_me, i_shaped) {
        (true, true) => Some(Influence::Mutual),
        (true, false) => Some(Influence::ShapedBy),
        (false, true) => Some(Influence::WentOnToShape),
        (false, false) => None,
    }
}

#[derive(Deserialize)]
struct CompareQuery {
    a: i32,
    b: i32,
}

/// Two stars side by side: what joins them, and how their sounds differ.
#[derive(Serialize)]
struct Comparison {
    why: Why,
    /// Each star's genres as shares of its own discography, lined up.
    spectrum: Vec<Line>,
    /// Stars both are listened alongside, strongest for both first.
    shared_neighbours: Vec<Shared>,
}

/// One band of the spectrum: how much of each star's work carries a genre.
#[derive(Serialize, PartialEq, Debug)]
struct Line {
    name: String,
    is_style: bool,
    /// Share of the first star's releases, 0..1.
    a: f32,
    /// Share of the second star's releases, 0..1.
    b: f32,
}

#[derive(Serialize)]
struct Shared {
    id: i32,
    name: String,
    /// The weaker of the two edges: a neighbour is shared only as strongly as
    /// it is shared with the one it is less tied to.
    score: f32,
}

async fn compare(State(state): State<AppState>, Query(query): Query<CompareQuery>) -> Response {
    if query.a == query.b {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "a star compared with itself has nothing to show" })),
        )
            .into_response();
    }
    match load_comparison(&state.pool, query.a, query.b).await {
        Ok(Some(comparison)) => (StatusCode::OK, Json(comparison)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "no such artist" }))).into_response(),
        Err(error) => {
            tracing::error!(%error, a = query.a, b = query.b, "failed to compare two artists");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be read" })),
            )
                .into_response()
        }
    }
}

async fn load_comparison(pool: &PgPool, a: i32, b: i32) -> sqlx::Result<Option<Comparison>> {
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM artist WHERE id = ANY($1)")
        .bind([a, b].as_slice())
        .fetch_one(pool)
        .await?;
    if known < 2 {
        return Ok(None);
    }

    let mut why = why_many(&mut *pool.acquire().await?, a, &[b]).await?.remove(&b).unwrap_or_default();

    // The pair is stored once, smaller id first, under the metric the card
    // also reads; see `artists::load_artist` for why the metric is pinned.
    why.co_listening = sqlx::query_scalar(
        "SELECT score FROM artist_similarity
         WHERE metric_id = (SELECT max(id) FROM similarity_metric)
           AND source_id = least($1, $2) AND target_id = greatest($1, $2)",
    )
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await?;

    let genres = |id: i32| {
        sqlx::query_as::<_, (String, bool, i32)>(
            "SELECT g.name, g.is_style, ag.releases
             FROM artist_genre ag JOIN genre g ON g.id = ag.genre_id
             WHERE ag.artist_id = $1",
        )
        .bind(id)
        .fetch_all(pool)
    };
    let spectrum = spectrum(&genres(a).await?, &genres(b).await?);

    let shared_neighbours = sqlx::query_as::<_, (i32, String, f32)>(
        "WITH metric AS (SELECT max(id) AS id FROM similarity_metric),
         near_a AS (
             SELECT CASE WHEN e.source_id = $1 THEN e.target_id ELSE e.source_id END AS id, e.score
             FROM artist_similarity e, metric
             WHERE e.metric_id = metric.id AND (e.source_id = $1 OR e.target_id = $1)
         ),
         near_b AS (
             SELECT CASE WHEN e.source_id = $2 THEN e.target_id ELSE e.source_id END AS id, e.score
             FROM artist_similarity e, metric
             WHERE e.metric_id = metric.id AND (e.source_id = $2 OR e.target_id = $2)
         )
         SELECT artist.id, artist.name, least(near_a.score, near_b.score)
         FROM near_a
         JOIN near_b USING (id)
         JOIN artist ON artist.id = near_a.id
         ORDER BY least(near_a.score, near_b.score) DESC, artist.name
         LIMIT 8",
    )
    .bind(a)
    .bind(b)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, score)| Shared { id, name, score })
    .collect();

    Ok(Some(Comparison {
        why,
        spectrum,
        shared_neighbours,
    }))
}

/// How many bands of each kind the spectrum shows.
const SPECTRUM_GENRES: usize = 6;
const SPECTRUM_STYLES: usize = 8;

/// Lines up two discographies genre by genre.
///
/// Shares, not release counts: an act with four hundred releases and one with
/// twelve are compared by what their work *is*, and raw counts would draw the
/// prolific one as more of everything. Genres and styles are shared out
/// separately, because Discogs tags a release with both and summing across the
/// two would count one record twice.
///
/// The bands kept are the ones that matter to either star, largest first --
/// so a genre one of them is all about is shown even when the other has none
/// of it, which is exactly the difference a comparison is for.
fn spectrum(a: &[(String, bool, i32)], b: &[(String, bool, i32)]) -> Vec<Line> {
    let mut out = Vec::new();
    for (is_style, keep) in [(false, SPECTRUM_GENRES), (true, SPECTRUM_STYLES)] {
        let shares = |rows: &[(String, bool, i32)]| -> HashMap<String, f32> {
            let total: i64 = rows.iter().filter(|r| r.1 == is_style).map(|r| i64::from(r.2.max(0))).sum();
            #[expect(clippy::cast_precision_loss, reason = "release counts are far below f32's exact range")]
            rows.iter()
                .filter(|r| r.1 == is_style && total > 0)
                .map(|r| (r.0.clone(), r.2.max(0) as f32 / total as f32))
                .collect()
        };
        let (sa, sb) = (shares(a), shares(b));
        let mut names: Vec<&String> = sa.keys().chain(sb.keys()).collect();
        names.sort();
        names.dedup();

        let mut lines: Vec<Line> = names
            .into_iter()
            .map(|name| Line {
                name: name.clone(),
                is_style,
                a: sa.get(name).copied().unwrap_or(0.0),
                b: sb.get(name).copied().unwrap_or(0.0),
            })
            .collect();
        lines.sort_by(|x, y| y.a.max(y.b).total_cmp(&x.a.max(x.b)).then_with(|| x.name.cmp(&y.name)));
        lines.truncate(keep);
        out.extend(lines);
    }
    out
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "shares of small integer counts are exact in f32")]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        let pool = PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network");
        routes().with_state(AppState {
            pool,
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
        })
    }

    #[test]
    fn influence_keeps_its_direction() {
        // (influenced, influence): 2 shaped 1, and 1 shaped 3.
        let arrows = [(1, 2), (3, 1), (4, 5)];
        assert_eq!(influence_between(1, 2, &arrows), Some(Influence::ShapedBy));
        assert_eq!(influence_between(1, 3, &arrows), Some(Influence::WentOnToShape));
        // Read from the other side, the same arrow says the opposite.
        assert_eq!(influence_between(2, 1, &arrows), Some(Influence::WentOnToShape));
        // An arrow between two other artists says nothing about this pair.
        assert_eq!(influence_between(1, 4, &arrows), None);
    }

    #[test]
    fn influence_both_ways_is_its_own_answer() {
        let arrows = [(1, 2), (2, 1)];
        assert_eq!(influence_between(1, 2, &arrows), Some(Influence::Mutual));
    }

    #[test]
    fn a_shared_genre_is_named_once_and_three_at_most() {
        let mut genres = Vec::new();
        for name in ["Blues", "Blues", "Rock", "Soul", "Funk"] {
            push_genre(&mut genres, name.to_string());
        }
        assert_eq!(genres, vec!["Blues", "Rock", "Soul"]);
    }

    #[test]
    fn the_spectrum_compares_shares_not_counts() {
        // A has 400 releases, B has 4; both are three-quarters rock. Counts
        // would draw A as a hundred times more rock than B.
        let a = vec![("Rock".to_string(), false, 300), ("Pop".to_string(), false, 100)];
        let b = vec![("Rock".to_string(), false, 3), ("Jazz".to_string(), false, 1)];
        let lines = spectrum(&a, &b);
        let rock = lines.iter().find(|l| l.name == "Rock").unwrap();
        assert_eq!((rock.a, rock.b), (0.75, 0.75));
        // What one star is and the other is not is kept: that is the difference.
        let jazz = lines.iter().find(|l| l.name == "Jazz").unwrap();
        assert_eq!((jazz.a, jazz.b), (0.0, 0.25));
    }

    #[test]
    fn genres_and_styles_are_shared_out_separately() {
        // One record tagged Electronic / Techno must be all electronic and all
        // techno, not half of each.
        let a = vec![("Electronic".to_string(), false, 10), ("Techno".to_string(), true, 10)];
        let lines = spectrum(&a, &[]);
        assert!(lines.iter().all(|l| l.a == 1.0), "{lines:?}");
    }

    #[test]
    fn the_spectrum_keeps_the_bands_that_matter_most() {
        let a: Vec<(String, bool, i32)> = (0..20).map(|i| (format!("G{i:02}"), false, 20 - i)).collect();
        let lines = spectrum(&a, &[]);
        assert_eq!(lines.len(), SPECTRUM_GENRES);
        assert_eq!(lines[0].name, "G00", "the largest band leads");
    }

    /// Runs the real reasons query over rows of its own, then rolls them back.
    ///
    /// The query is where the defects live -- a share computed as a double and
    /// decoded as a float failed every card on the first live run while every
    /// test above was green -- so it is run against Postgres, not described.
    /// Without a test database it says so and does not pass silently for a
    /// reason it did not check; CI always has one.
    #[tokio::test]
    async fn shared_genres_are_shares_of_both_and_influence_keeps_its_arrow() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the reasons query was NOT checked against a database");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        let mut tx = pool.begin().await.unwrap();

        // Ids far above anything MusicBrainz issues, so the fixture cannot
        // meet a real artist in a database that holds the canon.
        let (me, soul, rock) = (2_000_000_001, 2_000_000_002, 2_000_000_003);
        for (id, name) in [(me, "Fixture Singer"), (soul, "Fixture Soul"), (rock, "Fixture Rock")] {
            sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, gen_random_uuid(), $2, $2)")
                .bind(id)
                .bind(name)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        let genre = |name: &'static str| {
            sqlx::query_scalar::<_, i32>(
                "INSERT INTO genre (name, is_style) VALUES ($1, false)
                 ON CONFLICT (name, is_style) DO UPDATE SET name = EXCLUDED.name RETURNING id",
            )
            .bind(name)
        };
        let soul_genre = genre("Fixture Soul Genre").fetch_one(&mut *tx).await.unwrap();
        let rock_genre = genre("Fixture Rock Genre").fetch_one(&mut *tx).await.unwrap();
        // `me` is 95% soul, 5% rock; `soul` is all soul; `rock` all rock. The
        // 5% must not make "rock" a thing `me` shares with anyone.
        for (artist, genre, releases) in [(me, soul_genre, 95), (me, rock_genre, 5), (soul, soul_genre, 10), (rock, rock_genre, 40)] {
            sqlx::query("INSERT INTO artist_genre (artist_id, genre_id, releases) VALUES ($1, $2, $3)")
                .bind(artist)
                .bind(genre)
                .bind(releases)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        // `soul` shaped `me`.
        sqlx::query("INSERT INTO artist_influence (artist_id, influence_id) VALUES ($1, $2)")
            .bind(me)
            .bind(soul)
            .execute(&mut *tx)
            .await
            .unwrap();

        let why = why_many(&mut tx, me, &[soul, rock]).await.expect("the query runs and decodes");
        assert_eq!(why[&soul].genres, vec!["Fixture Soul Genre"]);
        assert_eq!(why[&soul].influence, Some(Influence::ShapedBy));
        assert!(why[&rock].genres.is_empty(), "a 5% sideline is not a shared genre: {:?}", why[&rock]);
        assert_eq!(why[&rock].influence, None);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_star_compared_with_itself_is_refused_before_the_database() {
        let response = app().oneshot(Request::get("/api/compare?a=5&b=5").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_comparison_needs_both_stars() {
        let response = app().oneshot(Request::get("/api/compare?a=5").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
