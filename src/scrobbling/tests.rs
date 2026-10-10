//! Reading listening end to end: a ListenBrainz that answers over real HTTP,
//! and the ledger in a real database.
//!
//! The ListenBrainz here is a small axum server with the one behaviour that
//! matters copied from the real one as measured on 2026-10-10: asked for
//! listens after `min_ts`, it returns the *oldest* `count` of them, newest
//! first. Paging, the overlap and the pending rule are all checked against
//! that, not against a description of it.
//!
//! The database half needs `LYRID_TEST_DATABASE_URL` and says so when it is
//! missing; every test that writes does it inside a transaction it rolls back.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use super::*;

const STAR: &str = "1c70a3fc-fa3c-4be1-8b55-c3192db8a884";
const GUEST: &str = "78b8347b-a1e2-4e36-a917-92d632a62c18";

#[derive(Clone, Default)]
struct Fake {
    listens: Arc<Mutex<HashMap<String, Vec<serde_json::Value>>>>,
    /// Requests to answer with 429 before answering properly, and the wait
    /// each of them names.
    throttle: Arc<Mutex<Vec<u64>>>,
    /// Every window asked about.
    asked: Arc<Mutex<Vec<Asked>>>,
    /// A moment after which the fake answers 503, as ListenBrainz's edge does
    /// when a request runs too long.
    broken_after: Arc<Mutex<Option<i64>>>,
}

impl Fake {
    fn add(&self, name: &str, listen: serde_json::Value) {
        self.listens.lock().unwrap().entry(name.to_string()).or_default().push(listen);
    }
}

/// A window as a request named it: `min_ts`, and `max_ts` if it had one.
type Asked = (i64, Option<i64>);

#[derive(serde::Deserialize)]
struct Window {
    min_ts: i64,
    max_ts: Option<i64>,
    count: usize,
}

async fn fake_listens(State(fake): State<Fake>, Path(name): Path<String>, Query(window): Query<Window>) -> Response {
    if let Some(wait) = fake.throttle.lock().unwrap().pop() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("x-ratelimit-remaining", "0".to_string()), ("x-ratelimit-reset-in", wait.to_string())],
            "slow down",
        )
            .into_response();
    }
    fake.asked.lock().unwrap().push((window.min_ts, window.max_ts));
    if let Some(broken) = *fake.broken_after.lock().unwrap()
        && window.min_ts >= broken
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "upstream timed out").into_response();
    }
    let listens = fake.listens.lock().unwrap();
    let Some(all) = listens.get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"code": 404, "error": format!("Cannot find user: {name}")})),
        )
            .into_response();
    };
    let mut after: Vec<serde_json::Value> = all
        .iter()
        .filter(|listen| {
            let at = listen["listened_at"].as_i64().unwrap();
            at > window.min_ts && window.max_ts.is_none_or(|max| at < max)
        })
        .cloned()
        .collect();
    // The oldest `count` in the window, then newest first: what the real
    // service does, with `max_ts` or without it.
    after.sort_by_key(|listen| listen["listened_at"].as_i64().unwrap());
    after.truncate(window.count);
    after.reverse();
    Json(serde_json::json!({ "payload": { "count": after.len(), "user_id": name, "listens": after } })).into_response()
}

async fn fake_validate(headers: HeaderMap) -> Json<serde_json::Value> {
    let token = headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
    if token == "Token good-token" {
        Json(serde_json::json!({"code": 200, "message": "Token valid.", "valid": true, "user_name": "Fixture"}))
    } else {
        Json(serde_json::json!({"code": 200, "message": "Token invalid.", "valid": false}))
    }
}

/// Starts the fake and returns a client pointed at it.
async fn listenbrainz(fake: Fake) -> Client {
    let app = Router::new()
        .route("/1/validate-token", get(fake_validate))
        .route("/1/user/{name}/listens", get(fake_listens))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Client::new(&format!("http://{address}"))
}

/// A listen at `at`, by `artists`, three minutes long, with an msid of its own.
fn listen(at: OffsetDateTime, artists: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "listened_at": at.unix_timestamp(),
        "inserted_at": at.unix_timestamp() + 20,
        "recording_msid": fresh().to_string(),
        "user_name": "Fixture",
        "track_metadata": {
            "artist_name": "Fixture",
            "track_name": "Fixture",
            "additional_info": { "duration_ms": 180_000 },
            "mbid_mapping": { "artist_mbids": artists }
        }
    })
}

/// An id nobody else is using. The crate builds uuids from data rather than
/// generating them, so the generator is not compiled in; a random number is.
fn fresh() -> Uuid {
    Uuid::from_u128(rand::random())
}

fn ago(minutes: i64) -> OffsetDateTime {
    // Whole seconds: ListenBrainz times are, and a fraction here would make a
    // listen compare unequal to itself after one round trip.
    let now = OffsetDateTime::now_utc() - time::Duration::minutes(minutes);
    now.replace_nanosecond(0).unwrap()
}

async fn database() -> Option<PgPool> {
    let _ = dotenvy::dotenv();
    let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
        eprintln!("LYRID_TEST_DATABASE_URL is not set: reading listening into the ledger was NOT checked against a database");
        return None;
    };
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.expect("test database");
    sqlx::migrate!().run(&pool).await.expect("migrations apply");
    Some(pool)
}

/// An account linked to `name` at `linked_at`, inside the test's transaction.
async fn linked(tx: &mut Transaction<'_, Postgres>, name: &str, linked_at: OffsetDateTime) -> i32 {
    let user: i32 = sqlx::query_scalar("INSERT INTO app_user (email, password_hash) VALUES ($1, 'x') RETURNING id")
        .bind(format!("{}@scrobbling.test", fresh()))
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO listenbrainz_link (user_id, name, linked_at) VALUES ($1, $2, $3)")
        .bind(user)
        .bind(name)
        .bind(linked_at)
        .execute(&mut **tx)
        .await
        .unwrap();
    user
}

async fn claim(tx: &mut Transaction<'_, Postgres>, user: i32) -> Claim {
    let row: (i32, String, OffsetDateTime, Option<OffsetDateTime>) =
        sqlx::query_as("SELECT user_id, name, linked_at, read_through FROM listenbrainz_link WHERE user_id = $1")
            .bind(user)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
    claim_of(row)
}

/// Reads a link as it stands in the database.
async fn read_link(tx: &mut Transaction<'_, Postgres>, client: &Client, user: i32) -> Result<Read, ReadError> {
    let claimed = claim(tx, user).await;
    read(tx, client, &claimed).await
}

async fn ledger(tx: &mut Transaction<'_, Postgres>, user: i32) -> (i64, i64) {
    sqlx::query_as("SELECT count(*), COALESCE(sum(light), 0) FROM listen WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_is_checked_and_its_owner_named() {
    let client = listenbrainz(Fake::default()).await;
    let check = |token: &'static str| {
        let client = client.clone();
        tokio::task::spawn_blocking(move || client.whose(token))
    };
    assert_eq!(check("good-token").await.unwrap(), Ok(Some("Fixture".to_string())));
    assert_eq!(check("someone-elses").await.unwrap(), Ok(None));
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_answering_is_unreachable_rather_than_a_refusal() {
    // A port nothing listens on: the difference between "ListenBrainz said
    // no" and "ListenBrainz said nothing" is what the person is told.
    let client = Client::new("http://127.0.0.1:1");
    let result = tokio::task::spawn_blocking(move || client.whose("good-token")).await.unwrap();
    assert_eq!(result, Err(Failure::Unreachable));
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_is_read_and_a_listen_on_two_pages_is_one_listen() {
    let fake = Fake::default();
    // Five listens, two to a page, and two of them in the same second so that
    // the first page ends between them. `min_ts` is exclusive: a next page
    // starting at the first page's newest second would never see the second.
    let base = ago(100);
    for offset in [0, 60, 60, 120, 180] {
        fake.add("Fixture", listen(base + time::Duration::seconds(offset), &[STAR]));
    }
    let client = listenbrainz(fake).await.with_page(2);
    let fetched = tokio::task::spawn_blocking(move || client.listens("Fixture", base)).await.unwrap().unwrap();
    assert_eq!(fetched.listens.len(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_stretch_is_read_a_bounded_window_at_a_time() {
    // Twelve days, listens scattered across them. Every request names both
    // ends and spans no more than a window: asked with a start alone,
    // ListenBrainz scans to the present and is cut off at forty seconds.
    let fake = Fake::default();
    for days in [11, 7, 3, 0] {
        fake.add("Fixture", listen(ago(days * 24 * 60 + 30), &[STAR]));
    }
    let client = listenbrainz(fake.clone()).await;
    let fetched = tokio::task::spawn_blocking(move || client.listens("Fixture", ago(12 * 24 * 60)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.listens.len(), 4);
    assert_eq!(fetched.stopped, None);

    let asked = fake.asked.lock().unwrap().clone();
    assert!(asked.len() >= 4, "twelve days in three-day windows: {asked:?}");
    for (after, before) in asked {
        let before = before.expect("a request without an end scans to the present");
        assert!(before - after <= client::WINDOW + 1, "a window of {} seconds", before - after);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_that_breaks_halfway_keeps_what_it_read() {
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();

    let user = linked(&mut tx, "Fixture", ago(12 * 24 * 60)).await;
    for days in [11, 10, 2] {
        fake.add("Fixture", listen(ago(days * 24 * 60), &[STAR]));
    }
    // ListenBrainz stops answering for anything from six days ago on.
    *fake.broken_after.lock().unwrap() = Some(ago(6 * 24 * 60).unix_timestamp());

    let partial = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(partial.listens, 2, "the two listens before the break are kept");
    let (failure, through): (Option<String>, Option<OffsetDateTime>) = sqlx::query_as("SELECT failure, read_through FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(failure.as_deref(), Some("unreachable"));
    // As far as the read got, and never past the listen it did not read.
    let through = through.expect("the cursor moved as far as the read got");
    assert!(through > ago(10 * 24 * 60) && through < ago(2 * 24 * 60), "{through}");

    // ListenBrainz recovers; the next read carries on and pays only the rest.
    *fake.broken_after.lock().unwrap() = None;
    let rest = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(rest.listens, 1);
    assert_eq!(ledger(&mut tx, user).await.0, 3);

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_wait_for_the_rate_limit_is_waited_and_a_long_one_is_not() {
    let fake = Fake::default();
    fake.add("Fixture", listen(ago(10), &[STAR]));
    fake.throttle.lock().unwrap().push(1);
    let client = listenbrainz(fake.clone()).await;

    let reader = client.clone();
    let fetched = tokio::task::spawn_blocking(move || reader.listens("Fixture", ago(60))).await.unwrap();
    assert_eq!(fetched.unwrap().listens.len(), 1);

    fake.throttle.lock().unwrap().push(3600);
    let fetched = tokio::task::spawn_blocking(move || client.listens("Fixture", ago(60))).await.unwrap();
    assert_eq!(fetched.unwrap_err(), Failure::Throttled);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_listen_is_paid_once_however_often_it_is_read() {
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();

    let user = linked(&mut tx, "Fixture", ago(120)).await;
    // Before the link: not this link's to read.
    fake.add("Fixture", listen(ago(180), &[STAR]));
    fake.add("Fixture", listen(ago(90), &[STAR]));
    fake.add("Fixture", listen(ago(60), &[STAR, GUEST]));
    fake.add("Fixture", listen(ago(30), &[GUEST]));

    let first = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(first.listens, 3);
    assert_eq!(first.opened, 2, "the star and its guest");
    // Three minutes each, every star still new.
    assert_eq!(first.light, 3 * 3 * i64::from(light::NEW_RATE));

    // The same window again: nothing new, nothing paid.
    let again = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(again, Read::default());
    assert_eq!(ledger(&mut tx, user).await, (3, first.light));

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn listens_read_again_do_not_move_the_price_of_the_new_one_beside_them() {
    // Nine listens of a star, then a tenth: it is the tenth listen, so still
    // new. The window brings the nine back with it, and if they were priced
    // again they would count twice and push the tenth past the threshold.
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();

    let user = linked(&mut tx, "Fixture", ago(600)).await;
    for minutes in 0..9 {
        fake.add("Fixture", listen(ago(300 - minutes * 5), &[STAR]));
    }
    assert_eq!(read_link(&mut tx, &client, user).await.unwrap().listens, 9);

    fake.add("Fixture", listen(ago(10), &[STAR]));
    let tenth = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(tenth.listens, 1);
    assert_eq!(tenth.light, 3 * i64::from(light::NEW_RATE));

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_listen_submitted_late_is_caught_by_the_overlap() {
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();

    let user = linked(&mut tx, "Fixture", ago(600)).await;
    fake.add("Fixture", listen(ago(300), &[STAR]));
    fake.add("Fixture", listen(ago(10), &[STAR]));
    assert_eq!(read_link(&mut tx, &client, user).await.unwrap().listens, 2);

    // A phone back online submits what it played hours ago, between the two.
    fake.add("Fixture", listen(ago(200), &[GUEST]));
    let late = read_link(&mut tx, &client, user).await.unwrap();
    assert_eq!(late.listens, 1);
    assert_eq!(late.opened, 1);

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unnamed_listen_waits_a_day_for_its_artist_and_is_then_counted_without_one() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Fixture", ago(3 * 24 * 60)).await;
    let claimed = claim(&mut tx, user).await;

    let unnamed = |minutes: i64| Listen {
        listened_at: ago(minutes),
        recording_msid: fresh(),
        artists: vec![],
        seconds: None,
    };
    let fetched = Fetched {
        listens: vec![unnamed(60), unnamed(30 * 60)],
        through: ago(0),
        stopped: None,
    };
    let read = apply(&mut tx, &claimed, fetched, OffsetDateTime::now_utc()).await.unwrap();
    assert_eq!(read.listens, 1, "only the one older than a day");
    assert_eq!(read.opened, 0);
    assert_eq!(read.light, i64::from(light::UNKNOWN_MINUTES * light::FAMILIAR_RATE));

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_read_leaves_its_code_and_the_next_good_one_clears_it() {
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Renamed", ago(60)).await;

    let failed = read_link(&mut tx, &client, user).await;
    assert!(matches!(failed, Err(ReadError::ListenBrainz(Failure::UnknownUser))));
    let failure: Option<String> = sqlx::query_scalar("SELECT failure FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(failure.as_deref(), Some("unknown_user"));

    fake.add("Renamed", listen(ago(5), &[STAR]));
    read_link(&mut tx, &client, user).await.unwrap();
    let (failure, read_at): (Option<String>, Option<OffsetDateTime>) = sqlx::query_as("SELECT failure, read_at FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(failure, None);
    assert!(read_at.is_some());

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_for_a_link_replaced_meanwhile_writes_nothing() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Before", ago(60)).await;
    let claimed = claim(&mut tx, user).await;

    // Relinked to another ListenBrainz account while the read was out.
    sqlx::query("UPDATE listenbrainz_link SET name = 'After' WHERE user_id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();

    let fetched = Fetched {
        listens: vec![Listen {
            listened_at: ago(5),
            recording_msid: fresh(),
            artists: vec![Uuid::parse_str(STAR).unwrap()],
            seconds: Some(200),
        }],
        through: ago(0),
        stopped: None,
    };
    assert_eq!(apply(&mut tx, &claimed, fetched, OffsetDateTime::now_utc()).await.unwrap(), Read::default());
    assert_eq!(ledger(&mut tx, user).await, (0, 0));

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_by_hand_waits_a_minute_after_the_last_one() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Fixture", ago(60)).await;

    assert!(matches!(claim_now(&mut tx, user).await.unwrap(), ByHand::Claimed(_)));
    // `now()` stands still inside a transaction, so this is "a moment later".
    assert!(matches!(claim_now(&mut tx, user).await.unwrap(), ByHand::TooSoon));

    sqlx::query("DELETE FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(matches!(claim_now(&mut tx, user).await.unwrap(), ByHand::NotLinked));

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_background_takes_each_due_link_once_and_then_rests() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Fixture", ago(60)).await;

    // Other links may exist in a development database; every due one is
    // taken, and ours is among them exactly once.
    let mut taken = Vec::new();
    while let Some(claimed) = claim_due(&mut tx).await.unwrap() {
        taken.push(claimed.user_id);
        assert!(taken.len() < 10_000, "claim_due keeps returning links it has just stamped");
    }
    assert_eq!(taken.iter().filter(|id| **id == user).count(), 1);

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn one_listenbrainz_account_feeds_one_lyrid_account() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    linked(&mut tx, "Shared", ago(60)).await;

    let other: i32 = sqlx::query_scalar("INSERT INTO app_user (email, password_hash) VALUES ($1, 'x') RETURNING id")
        .bind(format!("{}@scrobbling.test", fresh()))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    // The same account, typed in another case.
    let again = sqlx::query("INSERT INTO listenbrainz_link (user_id, name) VALUES ($1, 'shared')")
        .bind(other)
        .execute(&mut *tx)
        .await;
    assert!(again.is_err(), "two lyrid accounts were fed by one ListenBrainz account");

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unlinking_keeps_the_ledger_and_deleting_the_account_takes_it() {
    let Some(pool) = database().await else { return };
    let fake = Fake::default();
    let client = listenbrainz(fake.clone()).await;
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Fixture", ago(60)).await;
    fake.add("Fixture", listen(ago(5), &[STAR]));
    read_link(&mut tx, &client, user).await.unwrap();

    sqlx::query("DELETE FROM listenbrainz_link WHERE user_id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(ledger(&mut tx, user).await.0, 1, "unlinking stops reading; what was gathered stays");

    sqlx::query("DELETE FROM app_user WHERE id = $1").bind(user).execute(&mut *tx).await.unwrap();
    assert_eq!(ledger(&mut tx, user).await.0, 0, "the account took its listening with it");

    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ledger_refuses_a_listen_worth_nothing_and_a_listen_twice() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    let user = linked(&mut tx, "Fixture", ago(60)).await;
    let insert = |light: i32, msid: Uuid, at: OffsetDateTime| {
        sqlx::query("INSERT INTO listen (user_id, listened_at, recording_msid, artist_mbids, seconds, light) VALUES ($1, $2, $3, '{}', NULL, $4)")
            .bind(user)
            .bind(at)
            .bind(msid)
            .bind(light)
    };

    let mut savepoint = tx.begin().await.unwrap();
    assert!(insert(0, fresh(), ago(5)).execute(&mut *savepoint).await.is_err());
    savepoint.rollback().await.unwrap();

    let msid = fresh();
    let at = ago(5);
    insert(3, msid, at).execute(&mut *tx).await.unwrap();
    let mut savepoint = tx.begin().await.unwrap();
    assert!(insert(3, msid, at).execute(&mut *savepoint).await.is_err(), "one listen was written twice");
    savepoint.rollback().await.unwrap();

    tx.rollback().await.unwrap();
}
