//! The expedition, as the browser asks for it: setting out from an artist
//! someone loves, and how the contracts stand.
//!
//! Everything here needs a session, like the rest of a person's progress. See
//! `crate::expedition` for what an expedition is and ADR 0028 for why.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use super::accounts::{bad_request, server_error};
use super::scrobbling::signed_in;
use crate::app::AppState;
use crate::expedition::{self, Contract, Port, Refusal};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/me/expedition", get(show).put(begin))
}

/// The home port and the contracts: everything the expedition's panel shows.
#[derive(Serialize)]
struct Expedition {
    /// `null` until the expedition has begun.
    home: Option<Home>,
    contracts: Vec<ContractView>,
}

/// The home port, with what the player needs to land on it.
#[derive(Serialize)]
struct Home {
    #[serde(flatten)]
    star: Port,
    /// The uploads playlist of its channel, when it has one that embeds.
    uploads: Option<String>,
}

#[derive(Serialize)]
struct ContractView {
    id: i32,
    kind: String,
    star: Port,
    goal: i16,
    progress: i16,
    issued_at: String,
    done_at: Option<String>,
}

impl From<Contract> for ContractView {
    fn from(contract: Contract) -> Self {
        Self {
            id: contract.id,
            kind: contract.kind,
            star: contract.star,
            goal: contract.goal,
            progress: contract.progress,
            issued_at: stamp(contract.issued_at),
            done_at: contract.done_at.map(stamp),
        }
    }
}

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_else(|_| at.to_string())
}

async fn show(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    match read(&state, user).await {
        Ok(expedition) => Json(expedition).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to read an expedition");
            server_error("the expedition could not be read")
        }
    }
}

async fn read(state: &AppState, user: i32) -> sqlx::Result<Expedition> {
    let mut conn = state.pool.acquire().await?;
    let home = expedition::home_port(&mut conn, user).await?;
    let contracts = expedition::contracts(&mut conn, user).await?;
    let home = match home {
        Some(star) => {
            let uploads = match star.id {
                Some(id) => crate::api::artists::uploads_of(&state.pool, id).await?,
                None => None,
            };
            Some(Home { star, uploads })
        }
        None => None,
    };
    Ok(Expedition {
        home,
        contracts: contracts.into_iter().map(ContractView::from).collect(),
    })
}

#[derive(Deserialize)]
struct Setting {
    /// The MusicBrainz id of the artist someone loves.
    mbid: Uuid,
}

/// Sets out from the artist named, the one time it can be done. Answered with
/// the whole expedition, so the interface moves on from what the server holds.
async fn begin(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<Setting>) -> Response {
    let user = match signed_in(&state.pool, &headers).await {
        Ok(user) => user,
        Err(refused) => return refused.into_response(),
    };
    let begun = match state.pool.acquire().await {
        Ok(mut conn) => expedition::begin(&mut conn, user, body.mbid).await,
        Err(error) => Err(error),
    };
    match begun {
        Ok(Ok(_)) => match read(&state, user).await {
            Ok(expedition) => Json(expedition).into_response(),
            Err(error) => {
                tracing::error!(%error, "failed to read an expedition just begun");
                server_error("the expedition began, and could not be read back")
            }
        },
        Ok(Err(Refusal::NotOnSky)) => bad_request("that artist is not a star on this sky"),
        Ok(Err(Refusal::NotExploring)) => conflict("only the exploration mode sets out on an expedition"),
        Ok(Err(Refusal::Begun)) => conflict("the expedition has begun, from the star it began at"),
        Err(error) => {
            tracing::error!(%error, "failed to begin an expedition");
            server_error("the expedition could not begin")
        }
    }
}

fn conflict(message: &str) -> Response {
    (StatusCode::CONFLICT, Json(serde_json::json!({ "error": message }))).into_response()
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

    async fn status(method: &str, body: &str) -> StatusCode {
        let request = Request::builder()
            .method(method)
            .uri("/api/me/expedition")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        app().oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn an_expedition_is_nobody_elses_to_read_or_begin() {
        // No session cookie: answered before the database is ever asked, which
        // the dead pool proves.
        assert_eq!(status("GET", "").await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            status("PUT", r#"{"mbid":"00000000-0000-0000-0000-000000000001"}"#).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn an_expedition_sets_out_from_a_real_id() {
        assert!(status("PUT", r#"{"mbid":"not-a-uuid"}"#).await.is_client_error());
        assert!(status("PUT", r"{}").await.is_client_error());
    }
}
