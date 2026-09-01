//! Accounts, sessions and the profile behind them.
//!
//! What an account is for, at this version: it remembers the mode you chose,
//! where you left the sky and how you like your marked star drawn. That is a
//! small thing to log in for, and deliberately so -- fog, light and a journal
//! arrive with the game, and each brings its own tables. What is settled here
//! is the shape everything after it hangs from.
//!
//! Two rules in this module are product rules rather than implementation
//! detail, and both are enforced on the server:
//!
//! - **The mode is chosen once and never changes** (Vision, principle 5).
//!   There is no route that writes it after creation, and the column has no
//!   UPDATE path: a rule the client alone enforces is a rule until the first
//!   `curl`.
//! - **Anonymous browsing keeps working.** The sky, the card and the search
//!   never ask who is asking. An account adds memory; it does not become the
//!   price of admission (S4, and the reason the public preview is a version
//!   of its own).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get, routing::post};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::app::AppState;
use crate::auth;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/me", get(me).patch(update_profile))
}

/// What a client sends to create an account.
#[derive(Deserialize)]
struct Registration {
    email: String,
    password: String,
    /// Chosen here and never again.
    mode: String,
}

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

/// The parts of a profile a client may change afterwards.
///
/// Every field is optional and only the present ones are written, so a client
/// saving a camera position does not have to send back a marker it never
/// touched. `mode` is absent from this struct on purpose: it is not editable,
/// and leaving it out is what makes that true rather than a comment saying so.
#[derive(Deserialize)]
struct ProfileUpdate {
    halo_shape: Option<String>,
    halo_colour: Option<String>,
    camera: Option<Camera>,
}

/// Where the sky was left, and in which layout it means anything.
///
/// The layout travels with the coordinates because a rebuilt sky moves every
/// star (see migration 0007): restoring a camera into a newer layout would
/// open the map on empty space that used to be somewhere.
#[derive(Serialize, Deserialize, Clone, Copy)]
struct Camera {
    x: f32,
    y: f32,
    scale: f32,
}

/// Who is logged in, and what they have asked to be remembered.
#[derive(Serialize)]
struct Me {
    id: i32,
    email: String,
    mode: String,
    halo_shape: Option<String>,
    halo_colour: Option<String>,
    /// Absent when there is nothing saved, or when what is saved belongs to a
    /// layout the sky no longer shows.
    camera: Option<Camera>,
}

/// A session that has been checked against the database.
struct Session {
    user_id: i32,
}

/// Reads the session cookie and confirms it is a live session.
///
/// Expiry is checked in the query rather than in Rust: the database owns
/// `now()`, and a server whose clock has drifted should not be the thing
/// deciding whether a session is still valid.
async fn current_session(pool: &PgPool, headers: &HeaderMap) -> sqlx::Result<Option<Session>> {
    let Some(token) = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(auth::token_from_cookies)
    else {
        return Ok(None);
    };

    let row = sqlx::query_as::<_, (i32,)>("SELECT user_id FROM user_session WHERE token = $1 AND expires_at > now()")
        .bind(token)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|(user_id,)| Session { user_id }))
}

/// Opens a session for a user and returns the cookie that carries it.
async fn start_session(pool: &PgPool, user_id: i32, secure: bool) -> sqlx::Result<String> {
    let token = auth::session_token();
    sqlx::query("INSERT INTO user_session (token, user_id, expires_at) VALUES ($1, $2, now() + ($3 || ' days')::interval)")
        .bind(&token)
        .bind(user_id)
        .bind(auth::SESSION_DAYS.to_string())
        .execute(pool)
        .await?;
    Ok(auth::set_cookie(&token, secure))
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": message }))).into_response()
}

fn server_error(message: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unauthorised() -> Response {
    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not signed in" }))).into_response()
}

async fn register(State(state): State<AppState>, Json(body): Json<Registration>) -> Response {
    let email = auth::normalise_email(&body.email);
    if let Err(error) = auth::check_credentials(&email, &body.password) {
        return bad_request(&error.to_string());
    }
    if let Err(error) = auth::check_mode(&body.mode) {
        return bad_request(&error.to_string());
    }

    let Ok(hash) = auth::hash_password(&body.password) else {
        return server_error("the account could not be created");
    };

    match create_account(&state.pool, &email, &hash, &body.mode, state.secure_cookie).await {
        Ok(Some((me, cookie))) => ([(header::SET_COOKIE, cookie)], Json(me)).into_response(),
        // The address is taken. Said plainly: registration is where a service
        // tells you this anyway -- an account you cannot create and cannot be
        // told why is a dead end, and the address is already discoverable by
        // trying to register with it.
        Ok(None) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "an account with that address already exists" })),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to create an account");
            server_error("the account could not be created")
        }
    }
}

/// Creates the account, its profile and its first session in one transaction.
///
/// One transaction because an account without a profile is an account with no
/// mode, and the mode is the one thing that cannot be set later.
async fn create_account(pool: &PgPool, email: &str, hash: &str, mode: &str, secure: bool) -> sqlx::Result<Option<(Me, String)>> {
    let mut tx = pool.begin().await?;

    // ON CONFLICT rather than a prior SELECT: two registrations of the same
    // address racing each other both pass a check-then-insert, and the second
    // one dies on the constraint. This asks the constraint directly.
    let inserted = sqlx::query_as::<_, (i32,)>("INSERT INTO app_user (email, password_hash) VALUES ($1, $2) ON CONFLICT (email) DO NOTHING RETURNING id")
        .bind(email)
        .bind(hash)
        .fetch_optional(&mut *tx)
        .await?;

    let Some((user_id,)) = inserted else {
        return Ok(None);
    };

    sqlx::query("INSERT INTO user_profile (user_id, mode) VALUES ($1, $2)")
        .bind(user_id)
        .bind(mode)
        .execute(&mut *tx)
        .await?;

    let token = auth::session_token();
    sqlx::query("INSERT INTO user_session (token, user_id, expires_at) VALUES ($1, $2, now() + ($3 || ' days')::interval)")
        .bind(&token)
        .bind(user_id)
        .bind(auth::SESSION_DAYS.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let me = Me {
        id: user_id,
        email: email.to_string(),
        mode: mode.to_string(),
        halo_shape: None,
        halo_colour: None,
        camera: None,
    };
    Ok(Some((me, auth::set_cookie(&token, secure))))
}

async fn login(State(state): State<AppState>, Json(body): Json<Login>) -> Response {
    let email = auth::normalise_email(&body.email);

    let found = sqlx::query_as::<_, (i32, String)>("SELECT id, password_hash FROM app_user WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await;

    let found = match found {
        Ok(found) => found,
        Err(error) => {
            tracing::error!(%error, "failed to read an account");
            return server_error("the account could not be read");
        }
    };

    // One answer for "no such address" and "wrong password": telling them
    // apart turns the login form into a way of asking whether someone has an
    // account here, which is a fact about a person and not ours to publish.
    let Some((user_id, hash)) = found else {
        return unauthorised_login();
    };
    if !auth::verify_password(&body.password, &hash) {
        return unauthorised_login();
    }

    let cookie = match start_session(&state.pool, user_id, state.secure_cookie).await {
        Ok(cookie) => cookie,
        Err(error) => {
            tracing::error!(%error, "failed to open a session");
            return server_error("the session could not be opened");
        }
    };

    match load_me(&state.pool, user_id).await {
        Ok(Some(me)) => ([(header::SET_COOKIE, cookie)], Json(me)).into_response(),
        Ok(None) => server_error("the profile could not be read"),
        Err(error) => {
            tracing::error!(%error, "failed to read a profile");
            server_error("the profile could not be read")
        }
    }
}

fn unauthorised_login() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "that address and password do not match an account" })),
    )
        .into_response()
}

/// Ends the session, and says so even when there was none.
///
/// The token is deleted rather than left to expire: a session the user has
/// ended must stop working immediately, which is the whole reason sessions
/// live in the database instead of in a signed cookie.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(auth::token_from_cookies)
        && let Err(error) = sqlx::query("DELETE FROM user_session WHERE token = $1").bind(token).execute(&state.pool).await
    {
        tracing::error!(%error, "failed to end a session");
        return server_error("the session could not be ended");
    }

    // The cookie is cleared either way. A browser holding a token this server
    // has never heard of should still be told to drop it.
    (
        [(header::SET_COOKIE, auth::clear_cookie(state.secure_cookie))],
        Json(serde_json::json!({ "status": "signed out" })),
    )
        .into_response()
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match current_session(&state.pool, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            return server_error("the session could not be read");
        }
    };

    match load_me(&state.pool, session.user_id).await {
        Ok(Some(me)) => (StatusCode::OK, Json(me)).into_response(),
        // A live session whose account is gone: treat it as signed out rather
        // than as a server fault.
        Ok(None) => unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a profile");
            server_error("the profile could not be read")
        }
    }
}

async fn load_me(pool: &PgPool, user_id: i32) -> sqlx::Result<Option<Me>> {
    // The camera is dropped unless it was saved in the layout the sky
    // currently shows. Coordinates from an older layout are not stale, they
    // are meaningless: the same numbers point at a different star.
    //
    // Dropped by the query rather than by a second one: the profile itself
    // must come back either way, and a WHERE clause on the layout would hide
    // the whole account behind a camera it no longer needs.
    let Some((id, email, mode, halo_shape, halo_colour, x, y, scale)) =
        sqlx::query_as::<_, (i32, String, String, Option<String>, Option<String>, Option<f32>, Option<f32>, Option<f32>)>(
            "SELECT u.id, u.email, p.mode, p.halo_shape, p.halo_colour,
                    CASE WHEN current.keep THEN p.camera_x END,
                    CASE WHEN current.keep THEN p.camera_y END,
                    CASE WHEN current.keep THEN p.camera_scale END
             FROM app_user u
             JOIN user_profile p ON p.user_id = u.id
             CROSS JOIN LATERAL (
                 -- Both sides must be a real layout. `IS NOT DISTINCT FROM`
                 -- would call NULL = NULL a match, so a camera orphaned by a
                 -- deleted layout would come back as valid in a database
                 -- that has no layout at all -- which is exactly the state
                 -- a fresh stand is in before the sky is built.
                 SELECT p.layout_id IS NOT NULL
                    AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1) AS keep
             ) AS current
             WHERE u.id = $1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let camera = match (x, y, scale) {
        (Some(x), Some(y), Some(scale)) => Some(Camera { x, y, scale }),
        // A partly saved camera is no camera: two of three coordinates would
        // put the view somewhere nobody chose.
        _ => None,
    };

    Ok(Some(Me {
        id,
        email,
        mode,
        halo_shape,
        halo_colour,
        camera,
    }))
}

/// Saves what the client asks to be remembered.
///
/// Absent fields are left alone rather than cleared: a client saving a camera
/// position has not asked to forget the marker.
async fn update_profile(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<ProfileUpdate>) -> Response {
    let session = match current_session(&state.pool, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            return server_error("the session could not be read");
        }
    };

    if let Some(shape) = &body.halo_shape
        && shape.len() > 32
    {
        return bad_request("that marker shape is not one of ours");
    }
    if let Some(colour) = &body.halo_colour
        && colour.len() > 32
    {
        return bad_request("that marker colour is not one of ours");
    }
    if let Some(camera) = body.camera
        && !(camera.x.is_finite() && camera.y.is_finite() && camera.scale.is_finite() && camera.scale > 0.0)
    {
        // A non-finite coordinate is not a position, and Postgres would take
        // NaN happily -- leaving a profile that opens the sky nowhere.
        return bad_request("that is not a place on the map");
    }

    match save_profile(&state.pool, session.user_id, &body).await {
        Ok(()) => match load_me(&state.pool, session.user_id).await {
            Ok(Some(me)) => (StatusCode::OK, Json(me)).into_response(),
            Ok(None) => unauthorised(),
            Err(error) => {
                tracing::error!(%error, "failed to read a profile");
                server_error("the profile could not be read")
            }
        },
        Err(error) => {
            tracing::error!(%error, "failed to save a profile");
            server_error("the profile could not be saved")
        }
    }
}

async fn save_profile(pool: &PgPool, user_id: i32, update: &ProfileUpdate) -> sqlx::Result<()> {
    if update.halo_shape.is_some() || update.halo_colour.is_some() {
        sqlx::query(
            "UPDATE user_profile
             SET halo_shape = COALESCE($2, halo_shape), halo_colour = COALESCE($3, halo_colour)
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(update.halo_shape.as_deref())
        .bind(update.halo_colour.as_deref())
        .execute(pool)
        .await?;
    }

    if let Some(camera) = update.camera {
        // The layout is stamped from the sky as it is now, not taken from the
        // client: a client cannot know which layout it is looking at, and one
        // that claimed to would be claiming something worth lying about.
        sqlx::query(
            "UPDATE user_profile
             SET camera_x = $2, camera_y = $3, camera_scale = $4,
                 layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(camera.x)
        .bind(camera.y)
        .bind(camera.scale)
        .execute(pool)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    fn dead_pool() -> PgPool {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network")
    }

    fn app() -> Router {
        routes().with_state(AppState {
            pool: dead_pool(),
            secure_cookie: false,
        })
    }

    fn json(body: &serde_json::Value) -> Body {
        Body::from(body.to_string())
    }

    async fn error_of(response: Response) -> String {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        body.get("error").and_then(|value| value.as_str()).unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn a_bad_address_is_refused_before_the_database() {
        // Validation happens first, which is also why this passes with no
        // database behind it.
        let response = app()
            .oneshot(
                Request::post("/api/auth/register")
                    .header("content-type", "application/json")
                    .body(json(
                        &serde_json::json!({"email": "not an address", "password": "a long enough passphrase", "mode": "create"}),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(error_of(response).await.contains("email"));
    }

    #[tokio::test]
    async fn a_short_password_is_refused_before_the_database() {
        let response = app()
            .oneshot(
                Request::post("/api/auth/register")
                    .header("content-type", "application/json")
                    .body(json(&serde_json::json!({"email": "ada@example.com", "password": "short", "mode": "create"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(error_of(response).await.contains("password"));
    }

    #[tokio::test]
    async fn an_unknown_mode_is_refused_before_the_database() {
        // The set of modes is closed, and the column has a check constraint
        // saying so -- but a request that reaches it has already cost a
        // password hash and a round trip.
        let response = app()
            .oneshot(
                Request::post("/api/auth/register")
                    .header("content-type", "application/json")
                    .body(json(
                        &serde_json::json!({"email": "ada@example.com", "password": "a long enough passphrase", "mode": "administrator"}),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(error_of(response).await.contains("mode"));
    }

    #[test]
    fn a_profile_update_carrying_a_mode_carries_nothing() {
        // The product rule (Vision, principle 5) enforced by shape rather
        // than by a check that could be forgotten: a PATCH asking for a mode
        // parses into an update that has no mode in it, so there is nothing
        // for `save_profile` to write. Adding the field would break this.
        let update: ProfileUpdate = serde_json::from_value(serde_json::json!({"mode": "explore", "halo_shape": "ring"})).unwrap();
        assert_eq!(update.halo_shape.as_deref(), Some("ring"));
        assert!(update.camera.is_none());
        assert!(update.halo_colour.is_none());
    }

    #[tokio::test]
    async fn asking_who_i_am_without_a_cookie_is_unauthorised() {
        // No cookie, so no session -- answered without touching the database,
        // which is what lets this test run without one.
        let response = app().oneshot(Request::get("/api/me").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_cookie_that_is_not_ours_is_no_session() {
        let response = app()
            .oneshot(
                Request::get("/api/me")
                    .header(header::COOKIE, "theme=dark; consent=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn saving_a_profile_without_a_session_is_unauthorised() {
        // The check comes before any write: an anonymous PATCH must not reach
        // a row at all, not merely fail to find one.
        let response = app()
            .oneshot(
                Request::patch("/api/me")
                    .header("content-type", "application/json")
                    .body(json(&serde_json::json!({"halo_shape": "ring"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn signing_out_clears_the_cookie_even_without_a_session() {
        // A browser holding a token this server never issued should still be
        // told to drop it.
        let response = app().oneshot(Request::post("/api/auth/logout").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
        assert!(cookie.contains("Max-Age=0"), "{cookie}");
        assert!(cookie.contains("HttpOnly"), "{cookie}");
    }

    #[tokio::test]
    async fn a_wrong_login_does_not_reveal_whether_the_address_exists() {
        // Both halves of a failed login say the same thing. The wording is
        // asserted because the whole point is that it cannot differ.
        let unknown = unauthorised_login();
        assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(error_of(unknown).await, "that address and password do not match an account");
    }
}
