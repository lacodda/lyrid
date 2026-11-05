//! The fog, as the browser asks for it: which stars are this person's, and
//! telling the server when another one opens.
//!
//! Everything here needs a session. The sky itself stays public and the same
//! for everybody; this is the layer one person's progress adds over it, and the
//! tiles never know about it.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get, routing::post, routing::put};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use super::accounts::{bad_request, server_error};
use super::scrobbling::signed_in;
use crate::app::AppState;
use crate::fog::{self, Brought, How};
use crate::layout::tiles::brightness;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/me/sky", get(sky))
        .route("/api/me/openings/brought", post(bring))
        .route("/api/me/openings/{mbid}", put(open))
}

/// The most stars a browser may bring at once: far past what a person listens to
/// without an account, and a floor under what one request can make the server do.
const MOST_BROUGHT: usize = 5000;

/// A person's stars. Arrays and not objects: a long history is tens of thousands
/// of them, and the names of the fields would be most of the answer.
#[derive(Serialize)]
struct PersonalSky {
    /// `[id, x, y, brightness, year, name, opened_at]`
    opened: Vec<(i32, f32, f32, f32, i16, String, String)>,
    /// `[id, x, y, name, [the open stars that reflect it]]`
    echoes: Vec<(i32, f32, f32, String, Vec<i32>)>,
}

/// What the sky looks like to this person: the stars they have opened, and the
/// echoes those reflect.
async fn sky(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    match personal_sky(&state, user).await {
        Ok(sky) => Json(sky).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read a personal sky");
            server_error("your sky could not be read")
        }
    }
}

async fn personal_sky(state: &AppState, user: i32) -> sqlx::Result<PersonalSky> {
    let mut conn = state.pool.acquire().await?;
    let opened = fog::opened(&mut conn, user).await?;
    let ids: Vec<i32> = opened.iter().map(|star| star.id).collect();
    let echoes = fog::echoes(&mut conn, &ids).await?;
    // The one formula the tiles and the card share: a star is as bright in
    // someone's sky as it is in everyone's.
    let brightest = state.dial.sky(&state.pool).await?.map_or(0.0, |sky| sky.brightest);
    Ok(PersonalSky {
        opened: opened
            .into_iter()
            .map(|star| {
                let at = star.at.format(&Rfc3339).unwrap_or_else(|_| star.at.to_string());
                (star.id, star.x, star.y, brightness(star.listeners, brightest), star.year, star.name, at)
            })
            .collect(),
        echoes: echoes.into_iter().map(|echo| (echo.id, echo.x, echo.y, echo.name, echo.of)).collect(),
    })
}

#[derive(Deserialize)]
struct Opening {
    how: How,
}

/// A star was listened to here. Idempotent: the star is open however many times
/// someone says so, and it stays open from the first time.
async fn open(State(state): State<AppState>, headers: HeaderMap, Path(mbid): Path<Uuid>, Json(body): Json<Opening>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    let done = match state.pool.acquire().await {
        Ok(mut conn) => fog::open(&mut conn, user, mbid, body.how).await,
        Err(error) => Err(error),
    };
    match done {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to open a star");
            server_error("the star could not be opened")
        }
    }
}

#[derive(Deserialize)]
struct Bringing {
    stars: Vec<Brought>,
}

/// What a browser opened before anyone signed in, moved into the account.
///
/// Asked of every sign-in and not only of a registration: someone who listens
/// for a while on a new machine and then signs in to the account they already
/// have has not lost those stars either. Safe to send twice, so a browser that
/// never heard the answer simply sends it again next time.
async fn bring(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<Bringing>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    if body.stars.len() > MOST_BROUGHT {
        return bad_request("too many stars at once");
    }
    let done = match state.pool.acquire().await {
        Ok(mut conn) => fog::bring(&mut conn, user, &body.stars).await,
        Err(error) => Err(error),
    };
    match done {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to bring stars into an account");
            server_error("the stars could not be brought")
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
            listenbrainz: crate::scrobbling::Client::new("http://127.0.0.1:1"),
        })
    }

    async fn status(method: &str, uri: &str, body: &str) -> StatusCode {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        app().oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn a_personal_sky_is_nobody_elses_to_read_or_write() {
        // No session cookie: answered before the database is ever asked, which
        // the dead pool proves.
        for (method, uri, body) in [
            ("GET", "/api/me/sky", ""),
            ("PUT", "/api/me/openings/00000000-0000-0000-0000-000000000001", r#"{"how":"played"}"#),
            ("POST", "/api/me/openings/brought", r#"{"stars":[]}"#),
        ] {
            assert_eq!(status(method, uri, body).await, StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn an_opening_names_a_real_id_and_a_way_of_hearing_it() {
        let uuid = "/api/me/openings/00000000-0000-0000-0000-000000000001";
        assert_eq!(
            status("PUT", "/api/me/openings/not-a-uuid", r#"{"how":"played"}"#).await,
            StatusCode::BAD_REQUEST
        );
        assert!(status("PUT", uuid, r#"{"how":"brought"}"#).await.is_client_error());
    }
}
