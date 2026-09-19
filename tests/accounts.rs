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
//! reports success for code nobody executed. CI sets the variable, and `.env`
//! sets it locally (`.env.example` carries the line); a developer who wants
//! the fast path runs `cargo test --bins`.
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
    // `.env` is read as well as the environment, so the local gate checks the
    // same suites CI does without anyone remembering to export a variable
    // first. Before this, `rigger gate lyrid` could only ever be red here --
    // and a gate that cannot be green is a gate nobody reads.
    //
    // The variable is still its own name and not `DATABASE_URL`: what this
    // avoids is inheriting a development database by accident, not reading a
    // file. An absent `.env` is the normal case in CI and not an error.
    let _ = dotenvy::dotenv();

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

/// A layout for a camera to be saved in, with a metric of its own.
///
/// Everything it needs is created here rather than looked up. An earlier
/// version read `SELECT id FROM similarity_metric LIMIT 1`, which passed on a
/// development database holding an imported canon and failed on CI's empty
/// one — a test that depended on data nobody had promised it.
async fn make_layout(tx: &mut Transaction<'_, Postgres>, key: &str) -> i16 {
    let (metric,): (i16,) = sqlx::query_as("INSERT INTO similarity_metric (key, description) VALUES ($1, 'test') RETURNING id")
        .bind(format!("metric-for-{key}"))
        .fetch_one(&mut **tx)
        .await
        .expect("a similarity metric should be creatable");
    let (layout,): (i16,) = sqlx::query_as(
        "INSERT INTO sky_layout (key, metric_id, description, seed, stars)
         VALUES ($1, $2, 'test', 1, 0) RETURNING id",
    )
    .bind(key)
    .bind(metric)
    .fetch_one(&mut **tx)
    .await
    .expect("a layout should be creatable");
    layout
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

/// The same expression against a sky with no layouts in it.
///
/// The empty set is produced by a `WHERE false` rather than by emptying the
/// table: `DELETE FROM sky_layout` needs the foreign key from
/// `artist_position` dropped first, and an `ALTER TABLE` takes a lock over
/// the whole canon -- which deadlocked against the other tests in this file
/// the moment they ran in parallel. A test that has to lock the canon to
/// check one expression is testing the wrong thing.
const CAMERA_IS_CURRENT_WITH_NO_SKY: &str = "SELECT p.layout_id IS NOT NULL
        AND p.layout_id = (SELECT id FROM sky_layout WHERE false ORDER BY created_at DESC LIMIT 1)
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

    // And with no layouts at all -- the state that exposed the defect, and
    // the state a fresh stand is in before the sky is built.
    let (keep,): (Option<bool>,) = sqlx::query_as(CAMERA_IS_CURRENT_WITH_NO_SKY).bind(id).fetch_one(&mut *tx).await.unwrap();
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
    let layout = make_layout(&mut tx, "newest-layout").await;

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
    let layout = make_layout(&mut tx, "test-layout").await;

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

/// A token of one kind for one account, inside the caller's transaction.
async fn make_token(tx: &mut Transaction<'_, Postgres>, user_id: i32, purpose: &str, hours: i64) -> String {
    let token = format!("{purpose}-{user_id}-{hours}-{}", rand_suffix());
    sqlx::query(
        "INSERT INTO user_token (token, user_id, purpose, expires_at)
         VALUES ($1, $2, $3, now() + ($4 || ' hours')::interval)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(purpose)
    .bind(hours.to_string())
    .execute(&mut **tx)
    .await
    .expect("a token should be creatable");
    token
}

/// Enough uniqueness for a primary key inside one transaction.
fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed).to_string()
}

/// Spending a token, exactly as `accounts::spend_token` does it.
///
/// The statement is repeated here rather than the handler being called,
/// because what is under test is the statement: whether the database can be
/// made to hand the same token out twice. A test that called the handler
/// would be testing the same SQL through more layers.
async fn spend(tx: &mut Transaction<'_, Postgres>, token: &str, purpose: &str) -> Option<i32> {
    sqlx::query_as::<_, (i32,)>(
        "UPDATE user_token SET used_at = now()
         WHERE token = $1 AND purpose = $2 AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(token)
    .bind(purpose)
    .fetch_optional(&mut **tx)
    .await
    .expect("spending a token should not fail")
    .map(|(id,)| id)
}

#[tokio::test]
async fn a_new_profile_lands_in_the_creative_mode_without_being_told() {
    // The door stopped asking (decision of 2026-09-11), so the column's
    // default is the only thing left deciding. If it were dropped, every
    // registration would fail on the NOT NULL rather than quietly pick wrong
    // -- but a default that changed value would be silent, which is what this
    // pins.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let (id,): (i32,) = sqlx::query_as("INSERT INTO app_user (email, password_hash) VALUES ('fresh@example.com', 'x') RETURNING id")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    // Exactly the insert `create_account` performs: no mode named.
    sqlx::query("INSERT INTO user_profile (user_id) VALUES ($1)")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();

    let (mode,): (String,) = sqlx::query_as("SELECT mode FROM user_profile WHERE user_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(mode, "create", "a profile made without a mode must land in the mode that is actually built");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_link_works_once_even_when_clicked_twice() {
    // The rule the whole design of `spend_token` exists for. A read followed
    // by a write would let both clicks pass the read; this asserts the
    // database refuses the second one on its own.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let user = make_user(&mut tx, "twice@example.com", "create").await;
    let token = make_token(&mut tx, user, "reset", 1).await;

    assert_eq!(spend(&mut tx, &token, "reset").await, Some(user), "the first click should work");
    assert_eq!(spend(&mut tx, &token, "reset").await, None, "the second click must not");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn an_expired_link_is_refused() {
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let user = make_user(&mut tx, "stale@example.com", "create").await;
    // Issued an hour ago with a one-hour life: expired by exactly the margin
    // the reset route promises.
    let token = make_token(&mut tx, user, "reset", -1).await;

    assert_eq!(spend(&mut tx, &token, "reset").await, None);

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_confirmation_link_cannot_be_spent_as_a_reset() {
    // Both kinds live in one table, which is only safe if the purpose is part
    // of the check. Without it, the link in a welcome letter -- the one that
    // is deliberately long-lived and harmless -- would set a password.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let user = make_user(&mut tx, "crossed@example.com", "create").await;
    let confirmation = make_token(&mut tx, user, "confirm", 24).await;

    assert_eq!(
        spend(&mut tx, &confirmation, "reset").await,
        None,
        "a confirmation link must not open a password reset"
    );
    assert_eq!(
        spend(&mut tx, &confirmation, "confirm").await,
        Some(user),
        "and it must still work as what it is"
    );

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn only_tokens_of_the_two_purposes_can_be_stored() {
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let user = make_user(&mut tx, "purpose@example.com", "create").await;
    let refused = sqlx::query("INSERT INTO user_token (token, user_id, purpose, expires_at) VALUES ('x', $1, 'admin', now() + interval '1 hour')")
        .bind(user)
        .execute(&mut *tx)
        .await;
    assert!(refused.is_err(), "the set of purposes is closed by a constraint, not by the callers");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn deleting_an_account_takes_its_tokens_with_it() {
    // The charter promises deletion is total and immediate. A live reset link
    // outliving the account it opened would be the one row that made that
    // false -- and it would still be pointing at a user id Postgres is free
    // to hand to somebody else.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let user = make_user(&mut tx, "gone@example.com", "create").await;
    make_token(&mut tx, user, "reset", 1).await;
    make_token(&mut tx, user, "confirm", 24).await;

    sqlx::query("DELETE FROM app_user WHERE id = $1").bind(user).execute(&mut *tx).await.unwrap();

    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM user_token WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(left, 0, "a deleted account left {left} live link(s) into itself");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn the_backfill_in_migration_0009_is_the_statement_it_claims_to_be() {
    // The migration has already run by the time any test connects, so the
    // rows it fixed are gone from view. What can still be checked is that the
    // statement does what the comment above it says -- run against a row in
    // the state the migration found.
    //
    // Without this, the owner's own accounts -- made on a stand with no mail
    // server -- would be left unable to reset a password: inventing a problem
    // in order to have solved it correctly.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    let (id,): (i32,) = sqlx::query_as("INSERT INTO app_user (email, password_hash) VALUES ('legacy@example.com', 'x') RETURNING id")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    // Back to the state migration 0009 found every existing row in.
    sqlx::query("UPDATE app_user SET email_confirmed_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();

    // The migration's own statement, verbatim.
    sqlx::query("UPDATE app_user SET email_confirmed_at = created_at WHERE email_confirmed_at IS NULL")
        .execute(&mut *tx)
        .await
        .unwrap();

    let (confirmed,): (bool,) = sqlx::query_as("SELECT email_confirmed_at IS NOT NULL FROM app_user WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(confirmed, "an account predating confirmation was left unable to reset its password");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_usage_counter_counts_and_never_names() {
    // The charter's shape, asserted against the schema rather than against
    // the handler: incrementing twice must leave one row with two uses, not
    // two rows. If a column were ever added that could hold a user, this
    // would still pass -- so the columns themselves are read below.
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();

    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO mechanic_use (mechanic, day, uses) VALUES ('sky_opened', current_date, 1)
             ON CONFLICT (mechanic, day) DO UPDATE SET uses = mechanic_use.uses + 1",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
    }

    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM mechanic_use WHERE mechanic = 'sky_opened' AND day = current_date")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(rows, 1, "a counter incremented twice became {rows} rows: that is a log, not an aggregate");

    // And there is nowhere in the table for a person to be recorded. This is
    // the check that survives someone adding a column "just for debugging".
    let columns: Vec<(String,)> = sqlx::query_as("SELECT column_name FROM information_schema.columns WHERE table_name = 'mechanic_use'")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let names: Vec<String> = columns.into_iter().map(|(name,)| name).collect();
    assert_eq!(
        names.iter().map(String::as_str).collect::<std::collections::BTreeSet<_>>(),
        ["day", "mechanic", "uses"].into_iter().collect::<std::collections::BTreeSet<_>>(),
        "mechanic_use grew a column: anything beyond (mechanic, day, uses) can carry something about a person"
    );

    tx.rollback().await.unwrap();
}
