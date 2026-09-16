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
//!   `curl`. What changed in v0.11 is only *when* the choice happens: nothing
//!   is asked at the door, every account starts creative, and the choice
//!   arrives with the fog (v0.22) offered once. "Once and never" is unchanged;
//!   it simply has not started yet.
//! - **Everything an account holds can be taken back or destroyed.** The
//!   charter is not a page of prose with an email address at the bottom: the
//!   export and the deletion are routes, they are one press each, and the
//!   deletion is immediate and total rather than a flag on a row.
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
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::app::AppState;
use crate::auth;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/confirm", post(confirm))
        .route("/api/auth/confirm/resend", post(resend_confirmation))
        .route("/api/auth/forgot", post(forgot_password))
        .route("/api/auth/reset", post(reset_password))
        .route("/api/me", get(me).patch(update_profile).delete(delete_account))
        .route("/api/me/export", get(export_account))
}

/// What a client sends to create an account.
///
/// No mode: it is not asked for at the door any more (decision of
/// 2026-09-11), and leaving the field out of the struct is what makes that
/// true -- a client that sends one is sending something nothing will read.
#[derive(Deserialize)]
struct Registration {
    email: String,
    password: String,
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
    /// Whether the address has been confirmed.
    ///
    /// Shown rather than enforced: an unconfirmed account works in every way
    /// except being able to reset its password, and the interface says so
    /// where that matters instead of barring the door.
    email_confirmed: bool,
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

    let Ok(hash) = auth::hash_password(&body.password) else {
        return server_error("the account could not be created");
    };

    match create_account(&state.pool, &email, &hash, state.secure_cookie).await {
        Ok(Some((me, cookie, token))) => {
            // Sent after the transaction has committed, and its failure does
            // not undo the account. A letter that could not be sent is a
            // letter to send again -- there is a button for it -- while an
            // account rolled back because of a mail server leaves someone
            // who typed a password correctly with nothing at all.
            let link = format!("{}/confirm?token={token}", state.public_url);
            let (subject, letter) = crate::mail::confirmation(&link);
            if let Err(error) = state.mailer.send(&email, subject, &letter).await {
                tracing::error!(%error, "the confirmation letter could not be sent");
            }
            ([(header::SET_COOKIE, cookie)], Json(me)).into_response()
        }
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

/// Creates the account, its profile, its first session and its confirmation
/// token in one transaction.
///
/// One transaction because an account without a profile is an account with no
/// mode, and the mode is the one thing that cannot be set later. The
/// confirmation token joins it for the same reason: an account whose token
/// insert failed is an account that can never be confirmed and can never be
/// told why.
///
/// Returns the confirmation token so the caller can put it in a letter --
/// *after* the commit. Sending inside the transaction would hold a database
/// transaction open across a network round trip to a mail server, which is
/// how one slow SMTP host becomes a connection pool with nothing left in it.
async fn create_account(pool: &PgPool, email: &str, hash: &str, secure: bool) -> sqlx::Result<Option<(Me, String, String)>> {
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

    // The mode is not passed in: nothing asks for one any more, and the
    // column's default is what decides it. Naming the column here would put a
    // second answer beside the migration's.
    sqlx::query("INSERT INTO user_profile (user_id) VALUES ($1)")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let session = auth::session_token();
    sqlx::query("INSERT INTO user_session (token, user_id, expires_at) VALUES ($1, $2, now() + ($3 || ' days')::interval)")
        .bind(&session)
        .bind(user_id)
        .bind(auth::SESSION_DAYS.to_string())
        .execute(&mut *tx)
        .await?;

    let confirmation = auth::link_token();
    sqlx::query(
        "INSERT INTO user_token (token, user_id, purpose, expires_at)
         VALUES ($1, $2, 'confirm', now() + ($3 || ' hours')::interval)",
    )
    .bind(&confirmation)
    .bind(user_id)
    .bind(auth::CONFIRM_HOURS.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let me = Me {
        id: user_id,
        email: email.to_string(),
        mode: auth::DEFAULT_MODE.to_string(),
        halo_shape: None,
        halo_colour: None,
        camera: None,
        email_confirmed: false,
    };
    Ok(Some((me, auth::set_cookie(&session, secure), confirmation)))
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
    let Some((id, email, mode, halo_shape, halo_colour, x, y, scale, confirmed)) =
        sqlx::query_as::<_, (i32, String, String, Option<String>, Option<String>, Option<f32>, Option<f32>, Option<f32>, bool)>(
            "SELECT u.id, u.email, p.mode, p.halo_shape, p.halo_colour,
                    CASE WHEN current.keep THEN p.camera_x END,
                    CASE WHEN current.keep THEN p.camera_y END,
                    CASE WHEN current.keep THEN p.camera_scale END,
                    u.email_confirmed_at IS NOT NULL
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
        email_confirmed: confirmed,
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

/// A one-time token, as it arrives back from a link in a letter.
#[derive(Deserialize)]
struct TokenBody {
    token: String,
}

/// An address, for the forms that only have one.
#[derive(Deserialize)]
struct EmailBody {
    email: String,
}

/// A new password, and the token that earns the right to set it.
#[derive(Deserialize)]
struct ResetBody {
    token: String,
    password: String,
}

/// Spends a one-time token and says which account it belonged to.
///
/// The whole check is one statement on purpose. A read followed by a write
/// lets two clicks of the same link both pass the read, and "the link works
/// once" becomes "the link works once, usually". `used_at IS NULL` inside the
/// UPDATE makes the database decide, and it can only decide once.
async fn spend_token(pool: &PgPool, token: &str, purpose: &str) -> sqlx::Result<Option<i32>> {
    let row = sqlx::query_as::<_, (i32,)>(
        "UPDATE user_token SET used_at = now()
         WHERE token = $1 AND purpose = $2 AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(token)
    .bind(purpose)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(user_id,)| user_id))
}

/// Issues a fresh token, retiring the account's older ones of the same kind.
///
/// Retiring the old ones is what makes "ask again" mean something: two live
/// reset links for one account is two ways in, and the one the person did not
/// ask for is the one still sitting in an old letter.
async fn issue_token(pool: &PgPool, user_id: i32, purpose: &str, hours: i64) -> sqlx::Result<String> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE user_token SET used_at = now() WHERE user_id = $1 AND purpose = $2 AND used_at IS NULL")
        .bind(user_id)
        .bind(purpose)
        .execute(&mut *tx)
        .await?;

    let token = auth::link_token();
    sqlx::query(
        "INSERT INTO user_token (token, user_id, purpose, expires_at)
         VALUES ($1, $2, $3, now() + ($4 || ' hours')::interval)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(purpose)
    .bind(hours.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(token)
}

/// Confirms an address from the link in the letter.
async fn confirm(State(state): State<AppState>, Json(body): Json<TokenBody>) -> Response {
    let spent = match spend_token(&state.pool, &body.token, "confirm").await {
        Ok(spent) => spent,
        Err(error) => {
            tracing::error!(%error, "failed to spend a confirmation token");
            return server_error("the link could not be checked");
        }
    };

    let Some(user_id) = spent else {
        // One answer for expired, already used and never existed. They are
        // different facts, and none of them is the visitor's business: a link
        // that says "this token was already used" is a link that confirms a
        // guess.
        return bad_request("that link is not valid any more - ask for a new one");
    };

    // Written unconditionally rather than only when NULL: confirming an
    // already-confirmed address is not an error worth a branch, and the token
    // was already spent above.
    if let Err(error) = sqlx::query("UPDATE app_user SET email_confirmed_at = now() WHERE id = $1 AND email_confirmed_at IS NULL")
        .bind(user_id)
        .execute(&state.pool)
        .await
    {
        tracing::error!(%error, "failed to confirm an address");
        return server_error("the address could not be confirmed");
    }

    (StatusCode::OK, Json(serde_json::json!({ "status": "confirmed" }))).into_response()
}

/// Sends the confirmation letter again, to whoever is signed in.
///
/// Behind the session rather than behind an address in a form: the address is
/// already known, and a form taking one would let anyone post letters to
/// anyone from this server.
async fn resend_confirmation(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match current_session(&state.pool, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            return server_error("the session could not be read");
        }
    };

    let account = sqlx::query_as::<_, (String, bool)>("SELECT email, email_confirmed_at IS NOT NULL FROM app_user WHERE id = $1")
        .bind(session.user_id)
        .fetch_optional(&state.pool)
        .await;

    let (email, already_confirmed) = match account {
        Ok(Some(account)) => account,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read an account");
            return server_error("the account could not be read");
        }
    };

    if already_confirmed {
        return (StatusCode::OK, Json(serde_json::json!({ "status": "already confirmed" }))).into_response();
    }

    let token = match issue_token(&state.pool, session.user_id, "confirm", auth::CONFIRM_HOURS).await {
        Ok(token) => token,
        Err(error) => {
            tracing::error!(%error, "failed to issue a confirmation token");
            return server_error("the letter could not be sent");
        }
    };

    let link = format!("{}/confirm?token={token}", state.public_url);
    let (subject, letter) = crate::mail::confirmation(&link);
    if let Err(error) = state.mailer.send(&email, subject, &letter).await {
        // Said out loud here, unlike at registration: this request *is* the
        // letter. Reporting success would leave someone waiting for mail that
        // was never sent.
        tracing::error!(%error, "the confirmation letter could not be sent");
        return server_error("the letter could not be sent");
    }

    (StatusCode::OK, Json(serde_json::json!({ "status": "sent" }))).into_response()
}

/// Starts a password reset.
///
/// **Answers the same way whether or not the address is known.** The form is
/// open to anyone, so an answer that differed would turn it into a way of
/// asking whether a given person has an account here.
async fn forgot_password(State(state): State<AppState>, Json(body): Json<EmailBody>) -> Response {
    let sent = || (StatusCode::OK, Json(serde_json::json!({ "status": "sent if we know the address" }))).into_response();

    let email = auth::normalise_email(&body.email);
    if !auth::looks_like_email(&email) {
        return sent();
    }

    // Only a confirmed address gets a reset letter. An unconfirmed one was
    // never proved to belong to whoever typed it, and mailing a way into an
    // account to an address nobody verified is the hole confirmation exists to
    // close.
    let found = sqlx::query_as::<_, (i32,)>("SELECT id FROM app_user WHERE email = $1 AND email_confirmed_at IS NOT NULL")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await;

    let found = match found {
        Ok(found) => found,
        Err(error) => {
            tracing::error!(%error, "failed to read an account for a reset");
            return server_error("the request could not be handled");
        }
    };

    let Some((user_id,)) = found else {
        return sent();
    };

    match issue_token(&state.pool, user_id, "reset", auth::RESET_HOURS).await {
        Ok(token) => {
            let link = format!("{}/reset?token={token}", state.public_url);
            let (subject, letter) = crate::mail::reset(&link);
            if let Err(error) = state.mailer.send(&email, subject, &letter).await {
                // Logged, not reported: the answer cannot depend on whether
                // the address exists, and "we could not send it" is an answer
                // only an existing address could get.
                tracing::error!(%error, "the reset letter could not be sent");
            }
        }
        Err(error) => tracing::error!(%error, "failed to issue a reset token"),
    }

    sent()
}

/// Finishes a password reset: sets the new password and ends every session.
async fn reset_password(State(state): State<AppState>, Json(body): Json<ResetBody>) -> Response {
    // The password is checked before the token is spent. A token burnt on a
    // password the server was going to refuse anyway leaves the person with a
    // dead link and a typo.
    if let Err(error) = auth::check_password(&body.password) {
        return bad_request(&error.to_string());
    }

    let Ok(hash) = auth::hash_password(&body.password) else {
        return server_error("the password could not be changed");
    };

    let spent = match spend_token(&state.pool, &body.token, "reset").await {
        Ok(spent) => spent,
        Err(error) => {
            tracing::error!(%error, "failed to spend a reset token");
            return server_error("the link could not be checked");
        }
    };

    let Some(user_id) = spent else {
        return bad_request("that link is not valid any more - ask for a new one");
    };

    if let Err(error) = change_password(&state.pool, user_id, &hash).await {
        tracing::error!(%error, "failed to change a password");
        return server_error("the password could not be changed");
    }

    // The cookie is cleared as well. Whoever is at this browser has just
    // proved they can read the account's mail, but they have not signed in --
    // and the next screen is the login form, which is where a reset should
    // leave someone.
    (
        [(header::SET_COOKIE, auth::clear_cookie(state.secure_cookie))],
        Json(serde_json::json!({ "status": "changed" })),
    )
        .into_response()
}

/// Sets a new password and ends every session the account has open.
///
/// Both, in one transaction. A reset is what someone does when they think
/// their password is known to somebody else, and leaving that somebody else's
/// session alive is answering the wrong half of the problem.
async fn change_password(pool: &PgPool, user_id: i32, hash: &str) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE app_user SET password_hash = $2 WHERE id = $1")
        .bind(user_id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_session WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    // A reset also retires any other live reset links: asking twice and
    // clicking the older letter should not be a second way in.
    sqlx::query("UPDATE user_token SET used_at = now() WHERE user_id = $1 AND purpose = 'reset' AND used_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Everything this service holds about the account, as one JSON document.
///
/// The charter's first promise made answerable: not a description of what is
/// held, but the rows themselves.
///
/// Sessions are listed without their tokens. A token is a live credential, and
/// an export is a file that gets mailed around and left in a downloads folder:
/// handing over working keys to the account is not part of handing over the
/// data about it.
async fn export_account(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match current_session(&state.pool, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            return server_error("the session could not be read");
        }
    };

    match gather_export(&state.pool, session.user_id).await {
        Ok(Some(document)) => (
            StatusCode::OK,
            // Named so a browser saves it as a file rather than showing it:
            // the point is to hand it over, not to display it.
            [(header::CONTENT_DISPOSITION, "attachment; filename=\"lyrid-account.json\"")],
            Json(document),
        )
            .into_response(),
        Ok(None) => unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to export an account");
            server_error("the account could not be exported")
        }
    }
}

async fn gather_export(pool: &PgPool, user_id: i32) -> sqlx::Result<Option<serde_json::Value>> {
    let account = sqlx::query_as::<_, (i32, String, Option<OffsetDateTime>, OffsetDateTime)>(
        "SELECT id, email, email_confirmed_at, created_at FROM app_user WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some((id, email, confirmed_at, created_at)) = account else {
        return Ok(None);
    };

    let profile = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<f32>, Option<f32>, Option<f32>, Option<String>)>(
        "SELECT p.mode, p.halo_shape, p.halo_colour, p.camera_x, p.camera_y, p.camera_scale, l.key
         FROM user_profile p LEFT JOIN sky_layout l ON l.id = p.layout_id
         WHERE p.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let sessions =
        sqlx::query_as::<_, (OffsetDateTime, OffsetDateTime)>("SELECT created_at, expires_at FROM user_session WHERE user_id = $1 ORDER BY created_at")
            .bind(user_id)
            .fetch_all(pool)
            .await?;

    Ok(Some(serde_json::json!({
        "exported_at": stamp(OffsetDateTime::now_utc()),
        "note": "Everything lyrid holds about this account. The sky itself is built from public data and belongs to nobody.",
        "account": {
            "id": id,
            "email": email,
            "email_confirmed_at": confirmed_at.map(stamp),
            "created_at": stamp(created_at),
        },
        "profile": profile.map(|(mode, halo_shape, halo_colour, x, y, scale, layout)| serde_json::json!({
            "mode": mode,
            "halo_shape": halo_shape,
            "halo_colour": halo_colour,
            "camera": match (x, y, scale) {
                (Some(x), Some(y), Some(scale)) => serde_json::json!({ "x": x, "y": y, "scale": scale, "layout": layout }),
                _ => serde_json::Value::Null,
            },
        })),
        "sessions": sessions
            .into_iter()
            .map(|(created, expires)| serde_json::json!({ "created_at": stamp(created), "expires_at": stamp(expires) }))
            .collect::<Vec<_>>(),
    })))
}

/// A timestamp as RFC 3339, which is what a person's other tools can read.
///
/// A formatting failure falls back to the type's own rendering rather than to
/// null: a moment printed oddly is still the moment, and an export that
/// silently drops a date is worse than one that prints it unusually.
fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_else(|_| at.to_string())
}

/// Destroys the account and everything hanging off it.
///
/// One DELETE, because the schema was built for this: every personal table
/// references `app_user` with `ON DELETE CASCADE` (migrations 0008 and 0009),
/// so the row going away takes the profile, the sessions and the tokens with
/// it. Not a flag, not a queue, not a "we will remove it within 30 days" --
/// the charter promises one press, and a promise kept by a background job is
/// a promise the user cannot check.
///
/// The usage counters are untouched and that is correct: they hold no row
/// about this person to delete, which is the property the aggregate shape was
/// chosen for.
async fn delete_account(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match current_session(&state.pool, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return unauthorised(),
        Err(error) => {
            tracing::error!(%error, "failed to read a session");
            return server_error("the session could not be read");
        }
    };

    match sqlx::query("DELETE FROM app_user WHERE id = $1")
        .bind(session.user_id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => (
            [(header::SET_COOKIE, auth::clear_cookie(state.secure_cookie))],
            Json(serde_json::json!({ "status": "deleted" })),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to delete an account");
            server_error("the account could not be deleted")
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
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
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
                    .body(json(&serde_json::json!({"email": "not an address", "password": "a long enough passphrase"})))
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
                    .body(json(&serde_json::json!({"email": "ada@example.com", "password": "short"})))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(error_of(response).await.contains("password"));
    }

    #[test]
    fn a_registration_carrying_a_mode_carries_nothing() {
        // The door does not ask for a mode any more (decision of 2026-09-11),
        // and the struct is what makes that true: a client sending one --
        // including an old build of this SPA, or someone with curl hoping to
        // pick the mode that is not built yet -- is sending a field nothing
        // reads. The column's default decides.
        let registration: Registration =
            serde_json::from_value(serde_json::json!({"email": "ada@example.com", "password": "a long enough passphrase", "mode": "explore"})).unwrap();
        assert_eq!(registration.email, "ada@example.com");
        assert_eq!(registration.password, "a long enough passphrase");
        // And a registration with no mode at all parses just as well, which is
        // what the SPA now sends.
        let plain: Registration = serde_json::from_value(serde_json::json!({"email": "ada@example.com", "password": "a long enough passphrase"})).unwrap();
        assert_eq!(plain.email, registration.email);
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
