//! Counting what gets used, without counting who used it.
//!
//! The charter promises that lyrid does not build a profile of what you
//! listen to or look at. Knowing whether anyone ever opens the radio is still
//! worth knowing — a mechanic nobody finds is a mechanic to move, not to
//! polish — so the question is asked in the only form the promise allows.
//!
//! **What makes this aggregate is the schema, not the discipline of the
//! writer.** There is no row per event: a counter is incremented in place, so
//! there is nothing to join, nothing to order by time, and nothing that
//! becomes personal later when someone thinks of a clever query. A table of
//! events with the user id left out is still a table of events, and the next
//! version that adds a timestamp "just for debugging" has reinvented the log
//! this deliberately is not.
//!
//! The consequence is accepted on purpose: these numbers can answer "how many
//! times was the radio opened today" and can never answer "did the people who
//! opened the radio come back". The second question is the one the charter
//! sold.
//!
//! A second consequence, stated rather than left to be discovered: **anyone
//! can inflate a counter.** The route takes no session, because requiring one
//! would mean the counters only described people with accounts — and because
//! identifying the caller is the thing this module exists not to do. So these
//! are a rough sense of what gets used, not a figure to report or to decide
//! anything irreversible on. Rate limiting would narrow it and is not worth a
//! bucket-per-address store here; if these numbers ever need to be trusted
//! that far, the honest fix is to say so and change the design, not to quietly
//! start keeping more.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};

use crate::app::AppState;

/// The mechanics worth counting, listed here rather than accepted from the
/// client.
///
/// A free-text name would let anyone write anything into the table, including
/// something that identifies a person -- an artist id, a search term. A closed
/// list cannot carry a payload, which is the property that makes the promise
/// hold against a hostile client rather than a polite one.
const MECHANICS: [&str; 10] = [
    // The sky opened at all, once per visit.
    "sky_opened",
    // A star's card was opened.
    "card_opened",
    // Something was played through the embedded channel.
    "listen_opened",
    // The view was shared: a link copied or a poster saved.
    "view_shared",
    // The charter page was read.
    "charter_read",
    // An account was asked to hand back or delete its data.
    "data_requested",
    // The era lens was turned on.
    "lens_used",
    // The time machine was moved off the present.
    "time_travelled",
    // Two stars were compared side by side.
    "stars_compared",
    // A route was opened, from a link or by adding a first stop.
    "route_opened",
];

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/metrics/{mechanic}", post(count))
}

/// A mechanic was used. The request carries no body, and that is the design:
/// a batch endpoint taking a list of uses with timestamps would be a session,
/// and a session is the shape of thing this module exists not to keep.
async fn count(State(state): State<AppState>, axum::extract::Path(mechanic): axum::extract::Path<String>) -> Response {
    if !MECHANICS.contains(&mechanic.as_str()) {
        // 404 rather than 400: an unknown mechanic is an unknown address, and
        // saying "that is not one of the six" is a more useful answer than
        // pretending the write happened.
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not a mechanic we count" }))).into_response();
    }

    // Per day, so the counter says something about when without saying
    // anything about whom. A day is the coarsest bucket that still answers
    // "did the change on Tuesday help".
    let written = sqlx::query(
        "INSERT INTO mechanic_use (mechanic, day, uses) VALUES ($1, current_date, 1)
         ON CONFLICT (mechanic, day) DO UPDATE SET uses = mechanic_use.uses + 1",
    )
    .bind(&mechanic)
    .execute(&state.pool)
    .await;

    if let Err(error) = written {
        // A counter that cannot be written is not worth failing a page over:
        // the person is here to look at the sky. Logged so it is visible, and
        // answered with the same 204 either way so the client has no reason to
        // retry and no way to tell.
        tracing::warn!(%error, mechanic, "a usage counter could not be written");
    }

    StatusCode::NO_CONTENT.into_response()
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
        })
    }

    #[tokio::test]
    async fn a_mechanic_outside_the_list_is_not_counted() {
        // The list is what stops a client writing a search term or an artist
        // id into the table. Without this check the promise holds only for
        // clients that keep it.
        let response = app()
            .oneshot(Request::post("/api/metrics/ada@example.com").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_known_mechanic_answers_even_when_the_counter_cannot_be_written() {
        // There is no database behind this pool, so the INSERT fails -- and
        // the answer is still 204. A counter is not worth failing a page for.
        let response = app()
            .oneshot(Request::post("/api/metrics/sky_opened").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn nothing_in_the_list_can_carry_a_payload() {
        // The property that makes these aggregates: a mechanic name is a
        // constant, so there is no room in it for anything about a person.
        for mechanic in MECHANICS {
            assert!(
                mechanic.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{mechanic} is not a plain constant name"
            );
        }
    }
}
