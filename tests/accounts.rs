//! Accounts against a real database.
//!
//! Everything in this file is a rule written in SQL — a constraint, a
//! cascade, a `CASE` deciding whether a saved camera still means anything.
//! None of it can be checked against the lazy pool the unit tests use, and
//! the first defect this suite was written for was found by hand rather than
//! by a test: a camera orphaned by a deleted layout came back as valid in a
//! database that had no layout at all.
//!
//! These need a database. Without `LYRID_TEST_DATABASE_URL` they cannot run,
//! and rather than passing quietly they **fail** — a suite that skips itself
//! reports success for code nobody executed. CI sets the variable; a
//! developer who wants the fast path runs `cargo test --lib`.
//!
//! Each test works inside a transaction that is rolled back, so a run leaves
//! the database exactly as it found it.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};

/// The database to test against, or a failure explaining what is missing.
///
/// Deliberately not `DATABASE_URL`: pointing this suite at a development
/// database by inheriting the variable already in `.env` is how a test suite
/// ends up writing to something someone cares about.
fn database_url() -> String {
    std::env::var("LYRID_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "LYRID_TEST_DATABASE_URL is not set, so the account rules were not checked.\n\
             These tests exercise constraints and queries that only exist in the database.\n\
             Set it to a database you do not mind writing to (every test rolls back), e.g.\n\
             \x20 LYRID_TEST_DATABASE_URL=postgres://lyrid:lyrid@localhost:5432/lyrid\n\
             or run `cargo test --bins` to skip the suites that need one."
        )
    })
}

async fn pool() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("the test database should be reachable");
    sqlx::migrate!().run(&pool).await.expect("migrations should apply to the test database");
    pool
}

/// One account with a profile, inside the caller's transaction.
async fn make_user(tx: &mut Transaction<'_, Postgres>, email: &str, mode: &str) -> i32 {
    let (id,): (i32,) = sqlx::query_as("INSERT INTO app_user (email, password_hash) VALUES ($1, 'x') RETURNING id")
        .bind(email)
        .fetch_one(&mut **tx)
        .await
        .expect("an account should be creatable");
    sqlx::query("INSERT INTO user_profile (user_id, mode) VALUES ($1, $2)")
        .bind(id)
        .bind(mode)
        .execute(&mut **tx)
        .await
        .expect("a profile should be creatable");
    id
}

#[tokio::test]
async fn one_mailbox_cannot_hold_two_accounts() {
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    make_user(&mut tx, "ada@example.com", "create").await;
    let again = sqlx::query("INSERT INTO app_user (email, password_hash) VALUES ('ada@example.com', 'y')")
        .execute(&mut *tx)
        .await;

    assert!(again.is_err(), "a second account took the same address");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_mode_outside_the_two_is_refused_by_the_database() {
    // The API checks this too, but the column is where it has to hold: the
    // rule is a product rule, and the API is not the only thing that can
    // write a row.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let (id,): (i32,) = sqlx::query_as("INSERT INTO app_user (email, password_hash) VALUES ('rogue@example.com', 'x') RETURNING id")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let wrong = sqlx::query("INSERT INTO user_profile (user_id, mode) VALUES ($1, 'administrator')")
        .bind(id)
        .execute(&mut *tx)
        .await;

    assert!(wrong.is_err(), "a profile was created in a mode that does not exist");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn deleting_an_account_takes_its_sessions_and_profile_with_it() {
    // What the privacy charter will need, in place before there is anything
    // personal to delete.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let id = make_user(&mut tx, "leaving@example.com", "explore").await;
    sqlx::query("INSERT INTO user_session (token, user_id, expires_at) VALUES ('t', $1, now() + interval '1 day')")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query("DELETE FROM app_user WHERE id = $1").bind(id).execute(&mut *tx).await.unwrap();

    let (profiles,): (i64,) = sqlx::query_as("SELECT count(*) FROM user_profile WHERE user_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let (sessions,): (i64,) = sqlx::query_as("SELECT count(*) FROM user_session WHERE user_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    assert_eq!(profiles, 0, "a deleted account left its profile behind");
    assert_eq!(sessions, 0, "a deleted account left its sessions behind");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn rebuilding_the_canon_does_not_touch_accounts() {
    // `TRUNCATE artist ... CASCADE` is what re-importing MusicBrainz does,
    // and CASCADE follows foreign keys. The profile references `sky_layout`,
    // so the question "does re-importing the canon delete everyone's
    // account?" has to be answered by asking, not by reasoning.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let id = make_user(&mut tx, "keeper@example.com", "create").await;
    sqlx::query("TRUNCATE artist, release_group, artist_url, artist_credit RESTART IDENTITY CASCADE")
        .execute(&mut *tx)
        .await
        .unwrap();

    let (accounts,): (i64,) = sqlx::query_as("SELECT count(*) FROM app_user WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    assert_eq!(accounts, 1, "re-importing the canon deleted an account");
    tx.rollback().await.unwrap();
}

/// The rule `GET /api/me` applies to a saved camera, asked of the database
/// directly.
///
/// Kept in step with `load_me` by being the same expression; what is being
/// checked is the expression itself, which is where the defect lived.
const CAMERA_IS_CURRENT: &str = "SELECT p.layout_id IS NOT NULL
        AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
    FROM user_profile p WHERE p.user_id = $1";

#[tokio::test]
async fn an_orphaned_camera_is_refused_even_when_there_is_no_sky() {
    // Found by hand, not by a test: with `IS NOT DISTINCT FROM`, a camera
    // whose layout had been deleted (layout_id NULL) matched "the current
    // layout" in a database with no layouts, because NULL = NULL was called
    // a match. A fresh stand before the sky is built is exactly that state,
    // so the camera came back as valid pointing at coordinates from a sky
    // that no longer existed.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let id = make_user(&mut tx, "orphan@example.com", "create").await;
    sqlx::query("UPDATE user_profile SET camera_x = 1, camera_y = 2, camera_scale = 3, layout_id = NULL WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();

    // With layouts present, an orphaned camera is plainly not current.
    let (keep,): (Option<bool>,) = sqlx::query_as(CAMERA_IS_CURRENT).bind(id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(keep, Some(false), "an orphaned camera passed while layouts existed");

    // And with no layouts at all -- the state that exposed the defect.
    sqlx::query("ALTER TABLE artist_position DROP CONSTRAINT artist_position_layout_id_fkey")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM sky_layout").execute(&mut *tx).await.unwrap();

    let (keep,): (Option<bool>,) = sqlx::query_as(CAMERA_IS_CURRENT).bind(id).fetch_one(&mut *tx).await.unwrap();
    assert_ne!(keep, Some(true), "an orphaned camera was called current in a database with no sky at all");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_camera_saved_in_the_current_layout_is_kept() {
    // The other half of the rule: a check that only ever says "no" would
    // pass the test above and lose everybody's view.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let id = make_user(&mut tx, "settled@example.com", "create").await;
    let (metric,): (i16,) = sqlx::query_as("SELECT id FROM similarity_metric LIMIT 1").fetch_one(&mut *tx).await.unwrap();
    let (layout,): (i16,) = sqlx::query_as(
        "INSERT INTO sky_layout (key, metric_id, description, seed, stars)
         VALUES ('newest-layout', $1, 'test', 1, 0) RETURNING id",
    )
    .bind(metric)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    sqlx::query("UPDATE user_profile SET camera_x = 1, camera_y = 2, camera_scale = 3, layout_id = $2 WHERE user_id = $1")
        .bind(id)
        .bind(layout)
        .execute(&mut *tx)
        .await
        .unwrap();

    let (keep,): (Option<bool>,) = sqlx::query_as(CAMERA_IS_CURRENT).bind(id).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(keep, Some(true), "a camera saved in the newest layout was thrown away");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn losing_a_layout_costs_the_camera_and_nothing_else() {
    // The layout a camera was taken in can be deleted. That must cost the
    // camera its meaning, not cost the person their account.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let id = make_user(&mut tx, "traveller@example.com", "create").await;
    let (metric,): (i16,) = sqlx::query_as("SELECT id FROM similarity_metric LIMIT 1").fetch_one(&mut *tx).await.unwrap();
    let (layout,): (i16,) =
        sqlx::query_as("INSERT INTO sky_layout (key, metric_id, description, seed, stars) VALUES ('test-layout', $1, 'test', 1, 0) RETURNING id")
            .bind(metric)
            .fetch_one(&mut *tx)
            .await
            .unwrap();

    sqlx::query("UPDATE user_profile SET camera_x = 1, camera_y = 2, camera_scale = 3, layout_id = $2 WHERE user_id = $1")
        .bind(id)
        .bind(layout)
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query("DELETE FROM sky_layout WHERE id = $1")
        .bind(layout)
        .execute(&mut *tx)
        .await
        .unwrap();

    let (still_there, orphaned): (i64, Option<i16>) = sqlx::query_as("SELECT count(*) OVER (), layout_id FROM user_profile WHERE user_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    assert_eq!(still_there, 1, "deleting a layout deleted a profile");
    assert!(orphaned.is_none(), "the profile still points at a layout that is gone");
    tx.rollback().await.unwrap();
}
