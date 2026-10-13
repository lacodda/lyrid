//! Scrobbling, as the signed-in person sees it: linking ListenBrainz, a
//! Last.fm history brought in a file, what the listening has opened and
//! gathered, where it is at home, and the stars it has lit.
//!
//! Every route here is behind the session. The sky and the card stay
//! anonymous and cacheable; what is personal about a star is asked for
//! separately, by the person it is personal to.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get, routing::post, routing::put};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::accounts::{bad_request, current_session, server_error, unauthorised};
use crate::app::AppState;
use crate::scrobbling::{self, ByHand, Failure, ReadError, home, lastfm};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/me/listening", get(listening))
        .route("/api/me/listenbrainz", put(link).delete(unlink))
        .route("/api/me/listenbrainz/read", post(read_now))
        .route(
            "/api/me/lastfm",
            post(import_lastfm).delete(forget_lastfm).layer(DefaultBodyLimit::max(LASTFM_FILE_LIMIT)),
        )
        .route("/api/me/heard", get(heard_stars))
        .route("/api/me/heard/{id}", get(heard_star))
}

/// How large a Last.fm file may be: about half a million scrobbles, twenty
/// years of someone who listens all day. The body is read whole, so the limit
/// is also what one upload can cost the server in memory.
const LASTFM_FILE_LIMIT: usize = 64 * 1024 * 1024;

/// How many of the latest openings the summary names.
const RECENT: usize = 5;

/// The current layout, as every query of the sky's coordinates picks it.
const CURRENT_LAYOUT: &str = "(SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)";

/// What a person's listening has come to.
#[derive(Serialize)]
struct Listening {
    /// The ListenBrainz account being read, or `None` before one is linked.
    link: Option<Link>,
    /// All the light the ledger holds for this account.
    light: i64,
    /// Listens counted, from every source.
    listens: i64,
    /// Of those, the ones a Last.fm file brought: what "forget the file"
    /// would take back.
    from_file: i64,
    /// Stars on the current sky that a listen has named.
    stars: usize,
    /// The latest of those, newest first.
    recent: Vec<Opening>,
    /// Where the listening is at home on the sky, once anything is heard.
    home: Option<Star>,
}

#[derive(Serialize)]
struct Link {
    name: String,
    linked_at: String,
    /// When the last read that worked finished; `None` before the first.
    read_at: Option<String>,
    /// Why the last read failed, as a code the interface puts into words, or
    /// `None` when it did not.
    failure: Option<String>,
    /// Where reading the history from before the link stands, while it is
    /// still being read; `None` once the reading has caught up with the link.
    history: Option<History>,
}

#[derive(Serialize)]
struct History {
    /// Where the person's listening on ListenBrainz begins, once known.
    from: Option<String>,
    /// How far it has been read, once the first read has finished.
    through: Option<String>,
}

#[derive(Serialize)]
struct Opening {
    id: i32,
    name: String,
    /// The first listen that named this star.
    opened_at: String,
}

/// A star, and where the sky puts it.
#[derive(Serialize)]
struct Star {
    id: i32,
    name: String,
    x: f32,
    y: f32,
}

/// What a read by hand added, beside the summary it leaves.
#[derive(Serialize)]
struct ReadNow {
    listens: usize,
    light: i64,
    opened: usize,
    listening: Listening,
}

/// What a Last.fm file brought, beside the summary it leaves.
#[derive(Serialize)]
struct Brought {
    /// Scrobbles read from the file.
    scrobbles: usize,
    /// Rows that were not a scrobble.
    unreadable: usize,
    /// Listens new to the ledger.
    listens: usize,
    /// Of those, the ones whose artist lyrid could name.
    named: usize,
    /// Artists heard for the first time.
    opened: usize,
    listening: Listening,
}

/// What a person's listening says about one star.
#[derive(Serialize)]
struct Heard {
    listens: i64,
    first_at: Option<String>,
    last_at: Option<String>,
}

#[derive(Deserialize)]
struct LinkBody {
    token: String,
}

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_else(|_| at.to_string())
}

/// Why a request was turned away before it began.
pub(crate) enum Refused {
    NotSignedIn,
    SessionUnreadable,
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Self::NotSignedIn => unauthorised(),
            Self::SessionUnreadable => server_error("the session could not be read"),
        }
    }
}

/// The session's account.
pub(crate) async fn signed_in(pool: &PgPool, headers: &HeaderMap) -> Result<i32, Refused> {
    match current_session(pool, headers).await {
        Ok(Some(session)) => Ok(session.user_id),
        Ok(None) => Err(Refused::NotSignedIn),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            Err(Refused::SessionUnreadable)
        }
    }
}

async fn listening(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    answer(&state.pool, user, |listening| listening).await
}

/// The summary after whatever a route did, wrapped as the route needs it.
async fn answer<T: Serialize>(pool: &PgPool, user: i32, wrap: impl FnOnce(Listening) -> T) -> Response {
    match summary(pool, user).await {
        Ok(listening) => Json(wrap(listening)).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read a listening summary");
            server_error("the listening could not be read")
        }
    }
}

type LinkRow = (
    String,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<String>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
);

async fn summary(pool: &PgPool, user: i32) -> sqlx::Result<Listening> {
    let link = sqlx::query_as::<_, LinkRow>("SELECT name, linked_at, read_at, failure, read_through, history_from FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .fetch_optional(pool)
        .await?
        .map(|(name, linked_at, read_at, failure, read_through, history_from)| Link {
            name,
            linked_at: stamp(linked_at),
            read_at: read_at.map(stamp),
            failure,
            history: read_through.is_none_or(|through| through < linked_at).then(|| History {
                from: history_from.map(stamp),
                through: read_through.map(stamp),
            }),
        });

    let (listens, light, from_file): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(light), 0)::bigint, count(*) FILTER (WHERE source = 'lastfm')
         FROM listen WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;

    let mut placed = scrobbling::placed(&mut *pool.acquire().await?, user, None).await?;
    let home = home::home(&placed.iter().map(scrobbling::Placed::heard).collect::<Vec<_>>())
        .and_then(|home| placed.iter().find(|star| star.id == home.id))
        .map(|star| Star {
            id: star.id,
            name: star.name.clone(),
            x: star.x,
            y: star.y,
        });

    let stars = placed.len();
    placed.sort_by(|a, b| b.first_at.cmp(&a.first_at).then(a.id.cmp(&b.id)));
    let recent = placed
        .into_iter()
        .take(RECENT)
        .map(|star| Opening {
            id: star.id,
            name: star.name,
            opened_at: stamp(star.first_at),
        })
        .collect();

    Ok(Listening {
        link,
        light,
        listens,
        from_file,
        stars,
        recent,
        home,
    })
}

/// A token as ListenBrainz issues them: a short run of visible characters.
///
/// Checked before it goes anywhere, so a pasted paragraph or a token with a
/// line break in it is refused here rather than turned into a header.
fn looks_like_a_token(token: &str) -> bool {
    (8..=128).contains(&token.len()) && token.bytes().all(|byte| byte.is_ascii_graphic())
}

/// Links a ListenBrainz account, proven by its token.
///
/// The token is checked with ListenBrainz and dropped: nothing writes it to
/// the database or the log. Linking the account already linked is a no-op,
/// so pressing the button twice does not reset where reading stands.
async fn link(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<LinkBody>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };

    let token = body.token.trim().to_string();
    if !looks_like_a_token(&token) {
        return bad_request("that does not look like a ListenBrainz token");
    }

    let client = state.listenbrainz.clone();
    let whose = tokio::task::spawn_blocking(move || client.whose(&token))
        .await
        .unwrap_or(Err(Failure::Unreachable));

    let name = match whose {
        Ok(Some(name)) => name,
        Ok(None) => return bad_request("ListenBrainz does not know that token"),
        Err(Failure::Throttled) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "ListenBrainz asked us to wait - try again in a minute" })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "ListenBrainz did not answer - try again in a minute" })),
            )
                .into_response();
        }
    };

    // Linking another account starts over: reading begins again from the
    // start of the new account's history. The ledger is the lyrid account's
    // and is not touched.
    let linked = sqlx::query(
        "INSERT INTO listenbrainz_link (user_id, name) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE
             SET name = EXCLUDED.name, linked_at = now(), read_through = NULL, read_at = NULL, tried_at = NULL,
                 failure = NULL, history_from = NULL
             WHERE lower(listenbrainz_link.name) <> lower(EXCLUDED.name)",
    )
    .bind(user)
    .bind(&name)
    .execute(&state.pool)
    .await;

    match linked {
        Ok(_) => {}
        // The unique index on the name: the account feeds someone else.
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "that ListenBrainz account is already linked to another lyrid account" })),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to link a ListenBrainz account");
            return server_error("the account could not be linked");
        }
    }

    answer(&state.pool, user, |listening| listening).await
}

/// Stops reading. What the listening gathered stays with the account.
async fn unlink(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    if let Err(error) = sqlx::query("DELETE FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .execute(&state.pool)
        .await
    {
        tracing::error!(%error, "failed to unlink a ListenBrainz account");
        return server_error("the account could not be unlinked");
    }
    answer(&state.pool, user, |listening| listening).await
}

/// Reads now rather than at the next quarter hour.
async fn read_now(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };

    let mut conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "failed to reach the database");
            return server_error("the listening could not be read");
        }
    };

    let claim = match scrobbling::claim_now(&mut conn, user).await {
        Ok(ByHand::Claimed(claim)) => claim,
        Ok(ByHand::TooSoon) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "read a moment ago - try again in a minute" })),
            )
                .into_response();
        }
        Ok(ByHand::ReadingHistory) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "your history is still being read - it carries on by itself" })),
            )
                .into_response();
        }
        Ok(ByHand::NotLinked) => return bad_request("no ListenBrainz account is linked"),
        Err(error) => {
            tracing::error!(%error, "failed to take a link for reading");
            return server_error("the listening could not be read");
        }
    };

    // A failure from ListenBrainz is not an error of this request: it is
    // stored on the link, and the summary below carries its code to the
    // interface like any other state of the link.
    let read = match scrobbling::read(&mut conn, &state.listenbrainz, &claim).await {
        Ok(read) => read,
        Err(ReadError::ListenBrainz(_)) => scrobbling::Read::default(),
        Err(ReadError::Database(error)) => {
            tracing::error!(%error, "failed to write a read");
            return server_error("the listening could not be read");
        }
    };
    drop(conn);

    answer(&state.pool, user, |listening| ReadNow {
        listens: read.listens,
        light: read.light,
        opened: read.opened,
        listening,
    })
    .await
}

/// Brings in a Last.fm history: the file's text as the body.
///
/// Read as text with anything that is not UTF-8 replaced: a stray byte costs
/// one letter of one name, not the file.
async fn import_lastfm(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };

    let text = String::from_utf8_lossy(&body);
    let parsed = match lastfm::parse(&text) {
        Ok(parsed) => parsed,
        Err(lastfm::Refusal::NotCsv) => return bad_request("that file is not a CSV export"),
        Err(lastfm::Refusal::NoColumns) => return bad_request("that file has no column with a time or none with an artist"),
    };
    drop(text);

    let mut conn = match state.pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "failed to reach the database");
            return server_error("the file could not be read in");
        }
    };
    let imported = match lastfm::import(&mut conn, user, parsed).await {
        Ok(imported) => imported,
        Err(error) => {
            tracing::error!(%error, "failed to write a Last.fm file");
            return server_error("the file could not be read in");
        }
    };
    drop(conn);
    tracing::info!(
        user,
        scrobbles = imported.scrobbles,
        listens = imported.listens,
        named = imported.named,
        "Last.fm file read in"
    );

    answer(&state.pool, user, |listening| Brought {
        scrobbles: imported.scrobbles,
        unreadable: imported.unreadable,
        listens: imported.listens,
        named: imported.named,
        opened: imported.opened,
        listening,
    })
    .await
}

/// Takes back everything a Last.fm file brought.
async fn forget_lastfm(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    let forgotten = match state.pool.acquire().await {
        Ok(mut conn) => lastfm::forget(&mut conn, user).await,
        Err(error) => Err(error),
    };
    if let Err(error) = forgotten {
        tracing::error!(%error, "failed to forget a Last.fm history");
        return server_error("the file's history could not be forgotten");
    }
    answer(&state.pool, user, |listening| listening).await
}

/// Every star of the current sky this person's listening has named, as
/// `[id, x, y]` -- the rings the sky draws around them.
async fn heard_stars(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    let stars = sqlx::query_as::<_, (i32, f32, f32)>(sqlx::AssertSqlSafe(format!(
        "SELECT a.id, p.x, p.y
         FROM (SELECT DISTINCT artist AS mbid FROM listen, unnest(artist_mbids) AS artist WHERE user_id = $1) h
         JOIN artist a ON a.mbid = h.mbid
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = {CURRENT_LAYOUT}
         ORDER BY a.id"
    )))
    .bind(user)
    .fetch_all(&state.pool)
    .await;
    match stars {
        Ok(stars) => Json(serde_json::json!({ "stars": stars })).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read heard stars");
            server_error("the listening could not be read")
        }
    }
}

/// What this person's listening says about one star: how often, since when.
///
/// An answer with no listens rather than a 404 for a star never heard: the
/// star exists, and "you have not heard it" is a fact about it.
async fn heard_star(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i32>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    let row = sqlx::query_as::<_, (i64, Option<OffsetDateTime>, Option<OffsetDateTime>)>(
        "SELECT count(l.*), min(l.listened_at), max(l.listened_at)
         FROM artist a
         LEFT JOIN listen l ON l.user_id = $1 AND l.artist_mbids @> ARRAY[a.mbid]
         WHERE a.id = $2",
    )
    .bind(user)
    .bind(id)
    .fetch_one(&state.pool)
    .await;
    match row {
        Ok((listens, first_at, last_at)) => Json(Heard {
            listens,
            first_at: first_at.map(stamp),
            last_at: last_at.map(stamp),
        })
        .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read what was heard of a star");
            server_error("the listening could not be read")
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
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
            listenbrainz: scrobbling::Client::new("http://127.0.0.1:1"),
        })
    }

    #[tokio::test]
    async fn every_route_wants_a_session() {
        for (method, path, body) in [
            ("GET", "/api/me/listening", ""),
            ("PUT", "/api/me/listenbrainz", r#"{"token":"0000-0000-0000"}"#),
            ("DELETE", "/api/me/listenbrainz", ""),
            ("POST", "/api/me/listenbrainz/read", ""),
            ("POST", "/api/me/lastfm", "uts,artist\n1580474040,Air\n"),
            ("DELETE", "/api/me/lastfm", ""),
            ("GET", "/api/me/heard", ""),
            ("GET", "/api/me/heard/1", ""),
        ] {
            let response = app()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
        }
    }

    #[tokio::test]
    async fn a_large_file_is_not_refused_for_its_size_below_the_limit() {
        // Axum refuses bodies over two megabytes unless a route says
        // otherwise; a year of scrobbles is larger than that. Past the size
        // check, the request meets the session check -- which is the proof
        // the size was let through.
        let file = "x".repeat(10 * 1024 * 1024);
        let response = app()
            .oneshot(Request::builder().method("POST").uri("/api/me/lastfm").body(Body::from(file)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn a_token_is_one_short_run_of_visible_characters() {
        assert!(looks_like_a_token("c1b9e2f0-1d2e-4f3a-9b8c-7d6e5f4a3b2c"));
        assert!(!looks_like_a_token("short"));
        assert!(!looks_like_a_token("c1b9e2f0-1d2e\r\nX-Injected: 1"));
        assert!(!looks_like_a_token("a token with spaces in it"));
        assert!(!looks_like_a_token(&"x".repeat(129)));
    }
}
