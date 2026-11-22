//! The weekly letter, asked for and stopped.
//!
//! Asking is behind the session and needs a confirmed address: a letter sent
//! every week to an address nobody has shown is theirs is how a service ends
//! up mailing strangers. Stopping needs neither -- the link at the bottom of a
//! letter, or the mail client's own button, stops it with one request.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post, routing::put};
use serde::Deserialize;
use time::OffsetDateTime;

use super::refusal::Refusal;
use super::scrobbling::signed_in;
use crate::app::AppState;
use crate::letter;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/me/letter", put(ask).delete(stop_from_account))
        .route("/api/letter/stop", post(stop_by_link))
}

#[derive(Deserialize)]
struct Asking {
    /// The language the person is reading lyrid in now.
    language: String,
}

#[derive(Deserialize)]
struct Stopping {
    token: String,
}

/// What the account says about the letter after a change.
fn letter_state(language: Option<&str>) -> Response {
    Json(serde_json::json!({ "letter": language.map(|language| serde_json::json!({ "language": language })) })).into_response()
}

async fn ask(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<Asking>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    if !letter::LANGUAGES.contains(&body.language.as_str()) {
        return Refusal::LetterLanguageUnknown.into_response();
    }

    let confirmed: Result<Option<bool>, _> = sqlx::query_scalar("SELECT email_confirmed_at IS NOT NULL FROM app_user WHERE id = $1")
        .bind(user)
        .fetch_optional(&state.pool)
        .await;
    match confirmed {
        Ok(Some(true)) => {}
        Ok(Some(false) | None) => return Refusal::AddressUnconfirmed.into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read an account");
            return Refusal::LetterNotAskedFor.into_response();
        }
    }

    match letter::subscribe(&state.pool, user, &body.language, OffsetDateTime::now_utc()).await {
        Ok(()) => letter_state(Some(&body.language)),
        Err(error) => {
            tracing::error!(%error, "failed to ask for the weekly letter");
            Refusal::LetterNotAskedFor.into_response()
        }
    }
}

async fn stop_from_account(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    match letter::unsubscribe(&state.pool, user).await {
        Ok(()) => letter_state(None),
        Err(error) => {
            tracing::error!(%error, "failed to stop the weekly letter");
            Refusal::LetterNotStopped.into_response()
        }
    }
}

/// Stops a letter by the token in its link.
///
/// The token travels in the address, as RFC 8058 has it: a mail client's
/// one-click POST carries `List-Unsubscribe=One-Click` as its body and the
/// token only in the URL. Answered the same whether a letter was stopped or
/// there was none: either way none will come.
async fn stop_by_link(State(state): State<AppState>, Query(stopping): Query<Stopping>) -> Response {
    if stopping.token.is_empty() || stopping.token.len() > 128 {
        return Refusal::StopLinkCut.into_response();
    }
    match letter::stop(&state.pool, &stopping.token).await {
        Ok(()) => Json(serde_json::json!({ "status": "stopped" })).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to stop a weekly letter by its link");
            Refusal::LetterNotStopped.into_response()
        }
    }
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
    async fn asking_for_the_letter_wants_a_session_and_stopping_it_does_not() {
        for (method, path) in [("PUT", "/api/me/letter"), ("DELETE", "/api/me/letter")] {
            let response = app()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"language":"en"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
        }

        // A mail client's one-click stop: no cookie, the token in the URL, a
        // form body. It reaches the database -- which in this test is not
        // there, so the answer is a server error and not a refusal.
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/letter/stop?token=abc")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("List-Unsubscribe=One-Click"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_link_cut_short_is_told_so() {
        let response = app()
            .oneshot(Request::builder().method("POST").uri("/api/letter/stop?token=").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
