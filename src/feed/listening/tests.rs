//! The listening feed end to end: a dump server that answers ranges over real
//! HTTP, Parquet files written the way ListenBrainz writes them, and the
//! counting in a real database.
//!
//! The fixture files mirror the dumps as measured on 2026-10-16: INT96 times,
//! the matched artists as a list of strings, and columns lyrid never reads
//! sitting between the ones it does -- so a reader that strays off its columns
//! fails here, against a server that only has what it was asked for.
//!
//! The database half needs `LYRID_TEST_DATABASE_URL` and says so when it is
//! missing; every test that writes does it inside a transaction it rolls back,
//! in a schema of its own, because the test database is a working one whose
//! own days of listening must not leak into what is asserted.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use parquet::basic::{Compression, ZstdLevel};
use parquet::data_type::{ByteArray, ByteArrayType, Int64Type, Int96, Int96Type};
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgConnection, PgPool};
use tower_http::services::ServeDir;
use uuid::Uuid;

use super::*;

/// The dump's own schema, the columns lyrid does not read included.
const SCHEMA: &str = "
message schema {
  required int96 listened_at;
  required int96 created;
  required int64 user_id;
  required binary recording_name (STRING);
  optional group artist_credit_mbids (LIST) {
    repeated group list {
      optional binary element (STRING);
    }
  }
}";

/// A listen as a fixture states it.
struct Row {
    created: i64,
    listened: i64,
    user: i64,
    artists: Vec<Uuid>,
}

fn int96(seconds: i64) -> Int96 {
    // Julian day, then nanoseconds into it in two halves: Spark's layout.
    let day = seconds.div_euclid(86_400) + 2_440_588;
    let nanos = seconds.rem_euclid(86_400) * 1_000_000_000;
    let mut value = Int96::new();
    value.set_data(
        u32::try_from(nanos & 0xffff_ffff).unwrap(),
        u32::try_from(nanos >> 32).unwrap(),
        u32::try_from(day).unwrap(),
    );
    value
}

/// A Parquet file holding `groups`, one row group each.
fn parquet(groups: &[&[Row]]) -> Vec<u8> {
    let schema = Arc::new(parse_message_type(SCHEMA).unwrap());
    let properties = Arc::new(WriterProperties::builder().set_compression(Compression::ZSTD(ZstdLevel::default())).build());
    let mut writer = SerializedFileWriter::new(Vec::new(), schema, properties).unwrap();
    for rows in groups {
        let mut group = writer.next_row_group().unwrap();
        let mut index = 0;
        while let Some(mut column) = group.next_column().unwrap() {
            match index {
                0 => {
                    let values: Vec<Int96> = rows.iter().map(|row| int96(row.listened)).collect();
                    column.typed::<Int96Type>().write_batch(&values, None, None).unwrap();
                }
                1 => {
                    let values: Vec<Int96> = rows.iter().map(|row| int96(row.created)).collect();
                    column.typed::<Int96Type>().write_batch(&values, None, None).unwrap();
                }
                2 => {
                    let values: Vec<i64> = rows.iter().map(|row| row.user).collect();
                    column.typed::<Int64Type>().write_batch(&values, None, None).unwrap();
                }
                3 => {
                    let values: Vec<ByteArray> = rows.iter().map(|_| ByteArray::from("a track nobody reads")).collect();
                    column.typed::<ByteArrayType>().write_batch(&values, None, None).unwrap();
                }
                _ => {
                    // An optional list of optional strings: a listen with no
                    // match is an empty list, as ListenBrainz writes it.
                    let mut values = Vec::new();
                    let mut definitions = Vec::new();
                    let mut repetitions = Vec::new();
                    for row in *rows {
                        if row.artists.is_empty() {
                            definitions.push(1);
                            repetitions.push(0);
                        }
                        for (at, artist) in row.artists.iter().enumerate() {
                            values.push(ByteArray::from(artist.to_string().as_str()));
                            definitions.push(3);
                            repetitions.push(i16::from(at > 0));
                        }
                    }
                    column
                        .typed::<ByteArrayType>()
                        .write_batch(&values, Some(&definitions), Some(&repetitions))
                        .unwrap();
                }
            }
            column.close().unwrap();
            index += 1;
        }
        group.close().unwrap();
    }
    writer.into_inner().unwrap()
}

/// The spark tar of one dump, written where the dump server keeps it.
fn publish(root: &Path, sequence: u32, from: &str, to: &str, files: &[Vec<u8>]) -> Dump {
    let name = format!("listenbrainz-dump-{sequence}-20261016-000002-incremental");
    let stem = format!("listenbrainz-spark-dump-{sequence}-20261016-000002-incremental");
    let directory = root.join("incremental").join(&name);
    std::fs::create_dir_all(&directory).unwrap();

    let mut tar = tar::Builder::new(Vec::new());
    let mut add = |path: String, bytes: &[u8]| {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, bytes).unwrap();
    };
    add(format!("{stem}/SCHEMA_SEQUENCE"), b"1");
    add(format!("{stem}/START_TIMESTAMP"), from.as_bytes());
    add(format!("{stem}/END_TIMESTAMP"), to.as_bytes());
    add(format!("{stem}/COPYING"), b"CC0 1.0 Universal");
    for (index, file) in files.iter().enumerate() {
        add(format!("{stem}/{index}.parquet"), file);
    }
    std::fs::write(directory.join(format!("{stem}.tar")), tar.into_inner().unwrap()).unwrap();
    Dump { sequence, name }
}

/// Serves `root` the way the dump server does: files with ranges, and the
/// directory's index page.
async fn serve(root: &Path) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback_service(ServeDir::new(root));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

const A: Uuid = Uuid::from_u128(0xa);
const B: Uuid = Uuid::from_u128(0xb);
const C: Uuid = Uuid::from_u128(0xc);

/// 2026-10-15 00:00:00 UTC.
const DAY: i64 = 1_792_022_400;

fn days_since_epoch(seconds: i64) -> i32 {
    i32::try_from(seconds.div_euclid(86_400)).unwrap()
}

#[tokio::test]
async fn a_dump_comes_down_to_who_heard_which_star_that_day() {
    let root = tempfile::tempdir().unwrap();
    let first: &[Row] = &[
        // Played and sent at once.
        Row {
            created: DAY + 36_000,
            listened: DAY + 35_880,
            user: 1,
            artists: vec![A],
        },
        // The same listener and star again: one pair.
        Row {
            created: DAY + 39_600,
            listened: DAY + 39_500,
            user: 1,
            artists: vec![A],
        },
        // A credit of two artists names both.
        Row {
            created: DAY + 40_000,
            listened: DAY + 39_900,
            user: 2,
            artists: vec![A, B],
        },
        // A history imported years later: not this day's listening.
        Row {
            created: DAY + 41_000,
            listened: 1_577_836_800,
            user: 3,
            artists: vec![A],
        },
    ];
    let second: &[Row] = &[
        // Two hours ahead of its arrival: a clock wrong by more than any
        // player's was.
        Row {
            created: DAY + 42_000,
            listened: DAY + 49_200,
            user: 4,
            artists: vec![A],
        },
        // Half an hour ahead: within it.
        Row {
            created: DAY + 43_000,
            listened: DAY + 44_800,
            user: 5,
            artists: vec![C],
        },
        // Not matched to anyone.
        Row {
            created: DAY + 44_000,
            listened: DAY + 43_900,
            user: 6,
            artists: vec![],
        },
        // Played early in the day, sent late: still within a day.
        Row {
            created: DAY + 82_800,
            listened: DAY + 1_800,
            user: 7,
            artists: vec![B],
        },
    ];
    let dump = publish(
        root.path(),
        2724,
        "2026-10-15 00:00:02.697000+00:00",
        "2026-10-16 00:00:02.657000+00:00",
        &[parquet(&[first, second])],
    );
    let base = serve(root.path()).await;

    let gathered = blocking(move || gather(&Dumps::new(&base), &dump)).await.expect("the dump reads");
    assert_eq!(gathered.from, "2026-10-15 00:00:02.697000+00:00");
    assert_eq!(gathered.to, "2026-10-16 00:00:02.657000+00:00");
    assert_eq!(gathered.listens, 8, "every row is read, counted or not");

    let mut pairs = gathered.pairs;
    pairs.sort();
    let day = days_since_epoch(DAY);
    let mut expected = vec![(day, A, 1), (day, A, 2), (day, B, 2), (day, C, 5), (day, B, 7)];
    expected.sort();
    assert_eq!(pairs, expected);
}

#[tokio::test]
async fn a_server_that_ignores_ranges_is_refused() {
    // Answers every request with the whole file: what a misconfigured mirror
    // does. Read as a range, its first bytes would be taken for a header.
    async fn whole() -> impl IntoResponse {
        (StatusCode::OK, vec![0u8; 4096])
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, Router::new().route("/{*path}", get(whole))).await.unwrap() });

    let dump = Dump {
        sequence: 1,
        name: "listenbrainz-dump-1-20261016-000002-incremental".to_string(),
    };
    let error = blocking(move || gather(&Dumps::new(&format!("http://{address}")), &dump))
        .await
        .err()
        .expect("a server that ignores ranges cannot be read");
    assert!(format!("{error:#}").contains("range"), "{error:#}");
}

// ------------------------------------------------------------ the database

async fn database() -> Option<PgPool> {
    let _ = dotenvy::dotenv();
    let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
        eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the database half of the listening feed tests");
        return None;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("the test database is reachable");
    sqlx::migrate!().run(&pool).await.expect("migrations apply");
    Some(pool)
}

/// The feed's tables, empty, in a schema of the transaction's own.
async fn world(conn: &mut PgConnection) {
    let schema = format!("listening_{:016x}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *conn)
        .await
        .unwrap();
    for table in ["artist", "feed_run", "listening_day", "listener_seen", "artist_day", "star_rising"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)"
        )))
        .execute(&mut *conn)
        .await
        .unwrap();
    }
    sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL search_path TO {schema}")))
        .execute(&mut *conn)
        .await
        .unwrap();
}

async fn canon(conn: &mut PgConnection, stars: &[Uuid]) {
    for (key, mbid) in (1..).zip(stars) {
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, 'Star', 'Star')")
            .bind(key)
            .bind(mbid)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
}

fn gathered(from: &str, to: &str, pairs: Vec<(i32, Uuid, i64)>) -> Gathered {
    Gathered {
        from: from.to_string(),
        to: to.to_string(),
        listens: 0,
        files: 1,
        pairs,
    }
}

#[tokio::test]
async fn a_closed_day_counts_people_and_forgets_who_they_were() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    world(&mut tx).await;
    // C is somebody this canon does not hold.
    canon(&mut tx, &[A, B]).await;

    let fifteenth = days_since_epoch(DAY);
    let first = gathered(
        "2026-10-15 00:00:02+00",
        "2026-10-16 00:00:02+00",
        vec![
            (fifteenth, A, 1),
            (fifteenth, A, 2),
            (fifteenth, B, 1),
            (fifteenth, C, 1),
            (fifteenth, C, 2),
            // The last two seconds of the dump fell on the sixteenth.
            (fifteenth + 1, A, 3),
        ],
    );
    assert_eq!(write(&mut tx, &first).await.unwrap(), 4, "the stranger's pairs are not kept");
    assert_eq!(close(&mut tx, &first.to).await.unwrap(), 1, "the fifteenth is closed; the sixteenth is not");

    // Then ListenBrainz skips the seventeenth, and the next dump begins on the
    // eighteenth. It carries a listen of the fifteenth too, which comes too
    // late: that day is counted.
    let eighteenth = fifteenth + 3;
    let second = gathered(
        "2026-10-18 00:00:03+00",
        "2026-10-19 00:00:03+00",
        vec![
            (eighteenth, A, 4),
            (eighteenth, A, 5),
            (eighteenth, B, 4),
            (eighteenth, B, 5),
            (eighteenth, B, 6),
            (fifteenth, A, 9),
        ],
    );
    assert_eq!(write(&mut tx, &second).await.unwrap(), 5, "nothing is added to a closed day");
    assert_eq!(
        close(&mut tx, &second.to).await.unwrap(),
        3,
        "the sixteenth, the missing seventeenth and the eighteenth"
    );

    let days: Vec<(String, i32, Option<i32>, bool)> =
        sqlx::query_as("SELECT to_char(day, 'MM-DD'), covered_seconds, listeners, closed_at IS NOT NULL FROM listening_day ORDER BY day")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        days,
        vec![
            ("10-15".to_string(), 86_398, Some(2), true),
            ("10-16".to_string(), 2, Some(1), true),
            // Missing, and saying so: no coverage, nobody counted.
            ("10-17".to_string(), 0, Some(0), true),
            ("10-18".to_string(), 86_397, Some(3), true),
            ("10-19".to_string(), 3, None, false),
        ]
    );

    // People per star per day, and a star one person heard is not kept.
    let counted: Vec<(String, Uuid, i32)> = sqlx::query_as("SELECT to_char(day, 'MM-DD'), artist_mbid, listeners FROM artist_day ORDER BY day, artist_mbid")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        counted,
        vec![("10-15".to_string(), A, 2), ("10-18".to_string(), A, 2), ("10-18".to_string(), B, 3)]
    );

    // And who they were is gone with the closed days.
    let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM listener_seen").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(waiting, 0);

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn rising_is_listeners_a_day_against_the_month_before_over_the_days_covered() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    world(&mut tx).await;

    // R rises from 2 a day to 20. S is big and steady. Q rises, but to too few
    // people to call it anything. G is steady at 10 a day -- over a month in
    // which the dumps covered only nine days: counted against the whole month
    // it would look like a rise from 3.2, which it is not.
    let r = Uuid::from_u128(0x1);
    let s = Uuid::from_u128(0x2);
    let q = Uuid::from_u128(0x3);
    let g = Uuid::from_u128(0x4);
    let through = days_since_epoch(DAY);
    for back in 0..35 {
        let day = through - back;
        let recent = back < 7;
        let covered = recent || back % 3 == 0;
        sqlx::query("INSERT INTO listening_day (day, covered_seconds, listeners, closed_at) VALUES (DATE '1970-01-01' + $1, $2, 1, now())")
            .bind(day)
            .bind(if covered { 86_400 } else { 0 })
            .execute(&mut *tx)
            .await
            .unwrap();
        if !covered {
            continue;
        }
        for (star, listeners) in [(r, if recent { 20 } else { 2 }), (s, 100), (q, if recent { 4 } else { 0 }), (g, 10)] {
            if listeners < 2 {
                continue;
            }
            sqlx::query("INSERT INTO artist_day (day, artist_mbid, listeners) VALUES (DATE '1970-01-01' + $1, $2, $3)")
                .bind(day)
                .bind(star)
                .bind(listeners)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
    }

    rise(&mut tx).await.unwrap();
    let risers: Vec<(Uuid, i32, f32, f32)> = sqlx::query_as("SELECT artist_mbid, rank, recent, earlier FROM star_rising ORDER BY rank")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(risers.len(), 1, "only R rises: {risers:?}");
    let (star, rank, recent, earlier) = risers[0];
    assert_eq!((star, rank), (r, 1));
    assert!((recent - 20.0).abs() < 0.01 && (earlier - 2.0).abs() < 0.01, "{recent} a day against {earlier}");

    // With too little of the month covered, nothing is called rising at all.
    sqlx::query("UPDATE listening_day SET covered_seconds = 0 WHERE day <= DATE '1970-01-01' + $1")
        .bind(through - 7 - 12)
        .execute(&mut *tx)
        .await
        .unwrap();
    rise(&mut tx).await.unwrap();
    let risers: i64 = sqlx::query_scalar("SELECT count(*) FROM star_rising").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(risers, 0, "a month with fewer than seven covered days says nothing");

    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_dump_older_than_the_last_one_counted_is_never_read() {
    let Some(pool) = database().await else { return };
    let mut tx = pool.begin().await.unwrap();
    world(&mut tx).await;
    sqlx::query(
        "INSERT INTO feed_run (feed, source, finished_at, rows_read, rows_kept)
         VALUES ('listening', 'listenbrainz-dump-2716-20261013-000002-incremental', now(), 1, 1)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    // A failed run counts for nothing.
    sqlx::query(
        "INSERT INTO feed_run (feed, source, finished_at, error)
         VALUES ('listening', 'listenbrainz-dump-2720-20261015-000002-incremental', now(), 'it broke')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    let offered = [2709, 2716, 2720, 2724]
        .into_iter()
        .map(|sequence| Dump {
            sequence,
            name: format!("listenbrainz-dump-{sequence}-20261016-000002-incremental"),
        })
        .collect();
    let pending: Vec<u32> = pending(&mut tx, offered).await.unwrap().into_iter().map(|dump| dump.sequence).collect();
    assert_eq!(pending, vec![2720, 2724]);

    tx.rollback().await.unwrap();
}
