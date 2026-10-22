//! The listeners import end to end: a statistics dump written the way
//! ListenBrainz writes it -- PAX headers before every file, the file of people
//! first and a great deal after it that must never be read -- served over real
//! HTTP, and the writing in a real database.
//!
//! The database half needs `LYRID_TEST_DATABASE_URL` and says so when it is
//! missing; every test that writes does it inside a transaction it rolls back,
//! in a schema of its own, because the test database is a working canon.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use tower_http::services::ServeDir;
use uuid::Uuid;

use super::*;

const STAMP: &str = "listenbrainz-statistics-dump-20261015-000002";

const A: Uuid = Uuid::from_u128(0xa);
const B: Uuid = Uuid::from_u128(0xb);
/// An id MusicBrainz merged into B.
const B_BEFORE: Uuid = Uuid::from_u128(0xb0);
const C: Uuid = Uuid::from_u128(0xc);
const D: Uuid = Uuid::from_u128(0xd);
/// An artist this canon does not hold.
const ELSEWHERE: Uuid = Uuid::from_u128(0xe1);

/// A PAX header naming `path`, as Python's tarfile writes one before every
/// file of the dump: the record's length counts its own digits.
fn pax(tar: &mut tar::Builder<Vec<u8>>, path: &str) {
    let body = format!(" path={path}\n");
    let mut len = body.len() + 1;
    while len.to_string().len() + body.len() != len {
        len += 1;
    }
    let record = format!("{len}{body}");
    let mut header = tar::Header::new_ustar();
    header.set_entry_type(tar::EntryType::XHeader);
    header.set_size(record.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "././@PaxHeader", record.as_bytes()).unwrap();
}

/// The statistics dump: its small files, the people, and `after` after them.
fn statistics(people: &str, after: &[u8]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let files: [(&str, &[u8]); 5] = [
        ("SCHEMA_SEQUENCE", b"1"),
        ("TIMESTAMP", b"2026-10-15 00:00:02.588811+00:00"),
        ("COPYING", b"CC0 1.0 Universal"),
        ("lbdump/statistics/artists_all_time.jsonl", people.as_bytes()),
        ("lbdump/statistics/artists_this_year.jsonl", after),
    ];
    for (path, bytes) in files {
        pax(&mut tar, &format!("{STAMP}/{path}"));
        // The ustar name is not the path: only the PAX record says where the
        // file is, so a reader that ignored it would find nothing.
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "truncated", bytes).unwrap();
    }
    zstd::encode_all(tar.into_inner().unwrap().as_slice(), 3).unwrap()
}

/// Bytes nothing compresses, so what comes after the people weighs what it
/// says it weighs.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn line(user: u32, played: &[(u64, Option<Uuid>)]) -> String {
    let data: Vec<String> = played
        .iter()
        .map(|(count, mbid)| {
            let mbid = mbid.map_or("null".to_string(), |mbid| format!("\"{mbid}\""));
            format!(r#"{{"listen_count":{count},"artist_name":"Someone","artist_mbid":{mbid}}}"#)
        })
        .collect();
    format!(r#"{{"user_id":{user},"data":[{}]}}"#, data.join(","))
}

fn canon() -> HashMap<Uuid, i32> {
    HashMap::from([(A, 1), (B, 2), (B_BEFORE, 2), (C, 3)])
}

/// A reader that counts what is taken from it.
struct Counting<R> {
    inner: R,
    taken: Arc<AtomicUsize>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.taken.fetch_add(read, Ordering::Relaxed);
        Ok(read)
    }
}

#[test]
fn people_are_read_against_the_canon() {
    let people = [
        line(
            1,
            &[(40, Some(A)), (3, Some(B_BEFORE)), (2, Some(B)), (5, None), (1, Some(ELSEWHERE)), (0, Some(C))],
        ),
        "not a person".to_string(),
        line(2, &[(7, Some(A))]),
        line(3, &[]),
    ]
    .join("\n");
    let audience = read(statistics(&people, b"").as_slice(), &canon()).unwrap();

    assert_eq!(
        audience.stats,
        Stats {
            people: 3,
            unmatched: 1,
            unknown: 1,
            unreadable: 1,
        }
    );
    let of = |star: i32| {
        let id = audience.artists.iter().position(|&s| s == star).expect("the star was read");
        (audience.listeners[id], audience.listens[id])
    };
    assert_eq!(of(1), (2, 47), "two people, their listens summed");
    assert_eq!(of(2), (1, 5), "an id merged into B is B, and one listener of it");
    assert!(!audience.artists.contains(&3), "a count of nothing is not a listener");
    assert_eq!(audience.people.len(), 2, "a person with no star of this canon is no one to compare");
    assert_eq!(audience.people[0].len(), 2, "B once, though listed under two ids");
}

#[test]
fn the_stream_is_left_where_the_people_end() {
    // Four megabytes after the people, as nineteen gigabytes follow them in
    // the real dump: reading them would be most of the download.
    let dump = statistics(&line(1, &[(4, Some(A))]), &noise(4 << 20));
    let taken = Arc::new(AtomicUsize::new(0));
    let stream = Counting {
        inner: dump.as_slice(),
        taken: Arc::clone(&taken),
    };
    let audience = read(stream, &canon()).unwrap();
    assert_eq!(audience.stats.people, 1);
    let taken = taken.load(Ordering::Relaxed);
    assert!(taken < dump.len() / 4, "took {taken} of {} bytes", dump.len());
}

#[test]
fn a_dump_without_the_people_is_refused() {
    let mut tar = tar::Builder::new(Vec::new());
    pax(&mut tar, &format!("{STAMP}/COPYING"));
    let mut header = tar::Header::new_ustar();
    header.set_size(3);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "COPYING", b"CC0".as_slice()).unwrap();
    let dump = zstd::encode_all(tar.into_inner().unwrap().as_slice(), 3).unwrap();

    let error = read(dump.as_slice(), &canon()).unwrap_err();
    assert!(format!("{error:#}").contains(MEMBER), "{error:#}");
}

// ------------------------------------------------------------ the dump server

const OLDER: &str = "listenbrainz-dump-2722-20261015-000002-full";
const NEWER: &str = "listenbrainz-dump-2723-20261015-000002-full";

/// Two full exports the way the dump server lays them out, index pages
/// included: the server writes its listings, a static directory does not.
fn publish(root: &Path, older: &[u8], newer: &[u8]) {
    let shelf = root.join("fullexport");
    std::fs::create_dir_all(&shelf).unwrap();
    std::fs::write(
        shelf.join("index.html"),
        format!(r#"<a href="../">../</a><a href="{NEWER}/">{NEWER}/</a><a href="{OLDER}/">{OLDER}/</a>"#),
    )
    .unwrap();
    for (name, dump) in [(OLDER, older), (NEWER, newer)] {
        let directory = shelf.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        let stats = format!("{STAMP}.tar.zst");
        std::fs::write(
            directory.join("index.html"),
            format!(
                r#"<a href="../">../</a><a href="listenbrainz-listens-dump.tar.zst">listens</a><a href="{stats}">statistics</a><a href="{stats}.md5">md5</a>"#
            ),
        )
        .unwrap();
        std::fs::write(directory.join(stats), dump).unwrap();
    }
}

async fn serve(root: &Path) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback_service(ServeDir::new(root));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work).await.unwrap()
}

#[tokio::test]
async fn the_newest_export_is_read_unless_one_is_named() {
    let root = tempfile::tempdir().unwrap();
    publish(
        root.path(),
        &statistics(&line(1, &[(1, Some(A))]), b""),
        &statistics(&[line(1, &[(1, Some(A))]), line(2, &[(1, Some(B))])].join("\n"), b""),
    );
    let dumps = Dumps::new(&serve(root.path()).await);

    let people = |wanted: Option<&'static str>| {
        let dumps = dumps.clone();
        blocking(move || {
            let dump = choose(&dumps, wanted)?;
            let audience = read(dumps.statistics(&dump)?, &canon())?;
            Ok((dump.name, audience.stats.people))
        })
    };
    assert_eq!(people(None).await.unwrap(), (NEWER.to_string(), 2), "the newest by number");
    assert_eq!(people(Some(OLDER)).await.unwrap(), (OLDER.to_string(), 1));

    let error = people(Some("listenbrainz-dump-1-20200101-000000-full")).await.unwrap_err();
    assert!(format!("{error:#}").contains(NEWER), "the refusal names what is on offer: {error:#}");
}

// ------------------------------------------------------------ the database

async fn database() -> Option<PgPool> {
    let _ = dotenvy::dotenv();
    let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
        eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the database half of the listeners import tests");
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

/// The tables the import touches, empty, in a schema the transaction puts
/// first on its search path: the test database is a working canon with
/// listeners and edges of its own.
async fn world(conn: &mut PgConnection) {
    let schema = format!("listeners_{:016x}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *conn)
        .await
        .unwrap();
    for table in [
        "artist",
        "artist_redirect",
        "dump_import",
        "similarity_metric",
        "artist_similarity",
        "artist_listeners",
    ] {
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

#[tokio::test]
async fn listeners_and_edges_are_written_and_replaced() {
    let Some(pool) = database().await else { return };
    let mut conn = pool.acquire().await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    world(&mut tx).await;

    // The canon numbers its stars against the order the dump meets them in:
    // A first in the dump, last by id. The pair has to be put in the canon's
    // order, which the table's check insists on.
    for (id, mbid) in [(30, A), (20, B), (10, C), (40, D)] {
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, 'Star', 'Star')")
            .bind(id)
            .bind(mbid)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    // Another metric's edge, which this import must leave standing.
    sqlx::query("INSERT INTO similarity_metric (id, key, description) VALUES (1, 'listenbrainz-2020', 'old')")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artist_similarity (metric_id, source_id, target_id, score) VALUES (1, 10, 40, 0.5)")
        .execute(&mut *tx)
        .await
        .unwrap();

    // Six people listen to A, B and C; four of them to D as well -- too few
    // for D to have neighbours, enough to have listeners.
    let people: Vec<String> = (1..=6)
        .map(|user| {
            let mut played = vec![(10, Some(A)), (5, Some(B)), (2, Some(C))];
            if user <= 4 {
                played.push((1, Some(D)));
            }
            line(user, &played)
        })
        .collect();
    let root = tempfile::tempdir().unwrap();
    let dump = statistics(&people.join("\n"), b"");
    publish(root.path(), &dump, &dump);
    let dumps = Dumps::new(&serve(root.path()).await);

    // Twice: the second run replaces the first rather than adding to it.
    for _ in 0..2 {
        import(&mut tx, &dumps, None).await.expect("the import runs");
    }

    let listeners: Vec<(i32, i32, i64)> = sqlx::query_as("SELECT artist_id, listeners, listens FROM artist_listeners ORDER BY artist_id")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(listeners, vec![(10, 6, 12), (20, 6, 30), (30, 6, 60), (40, 4, 4)]);

    let edges: Vec<(String, i32, i32, f32)> = sqlx::query_as(
        "SELECT m.key, e.source_id, e.target_id, e.score
         FROM artist_similarity e JOIN similarity_metric m ON m.id = e.metric_id
         ORDER BY m.key, e.source_id, e.target_id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        edges,
        vec![
            ("listenbrainz-2020".to_string(), 10, 40, 0.5),
            (METRIC_KEY.to_string(), 10, 20, 1.0),
            (METRIC_KEY.to_string(), 10, 30, 1.0),
            (METRIC_KEY.to_string(), 20, 30, 1.0),
        ],
        "everyone who listens to one of A, B and C listens to all three"
    );

    let import: (String, Option<i64>, bool) =
        sqlx::query_as("SELECT version, rows_imported, finished_at IS NOT NULL FROM dump_import WHERE source = 'listenbrainz-statistics'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(import, (NEWER.to_string(), Some(3), true), "one record, of the dump by its name");
}
