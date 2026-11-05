//! The fog of war: what a person has opened, what that echoes, and how all of
//! it survives a new canon.
//!
//! A star is open when someone has listened to it, and there are two records
//! of that: the ledger of listens read from outside (`scrobbling`), and
//! `star_opening` for the listening that happened here. Both name stars by
//! `MusicBrainz` id. Nothing else about the fog is stored -- how far the light
//! reaches is worked out in the browser from where the open stars are in the
//! sky being shown, so a sky laid out again lights the same stars in their new
//! places without anybody writing anything. See ADR 0026.
//!
//! The one thing a new canon can take away is an id: `MusicBrainz` merges two
//! artists and only one id remains. [`carry_over`] rewrites a person's progress
//! through the canon's redirects when a new edition arrives, once per edition
//! per person, and the edition reached is kept on the profile.

use std::collections::HashMap;

use serde::Deserialize;
use sqlx::{Connection, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::scrobbling::hold_ledger;

/// The two ways of being in the sky (Vision, principle 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Explore,
    Create,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Create => "create",
        }
    }
}

/// How a person came to open a star in lyrid itself. What the client may claim:
/// `brought` is decided here, by which route the star arrived through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Played,
    Followed,
}

impl How {
    fn as_str(self) -> &'static str {
        match self {
            Self::Played => "played",
            Self::Followed => "followed",
        }
    }
}

/// Records the choice, if none has been made. `false` when one has.
///
/// Choosing the creative mode forgets what was opened on the way to choosing:
/// that mode has no progress, and the charter does not keep what nothing uses.
pub async fn choose_mode(conn: &mut PgConnection, user: i32, mode: Mode) -> sqlx::Result<bool> {
    let mut tx = conn.begin().await?;
    let chosen = sqlx::query("UPDATE user_profile SET mode = $2 WHERE user_id = $1 AND mode IS NULL")
        .bind(user)
        .bind(mode.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected()
        == 1;
    if chosen && mode == Mode::Create {
        sqlx::query("DELETE FROM star_opening WHERE user_id = $1").bind(user).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(chosen)
}

/// Opens a star, unless the person's mode has no progress to keep.
///
/// An id the canon has since merged lands on the star it became: a page left
/// open across an update, or a browser's memory of last year, may name either.
pub async fn open(conn: &mut PgConnection, user: i32, mbid: Uuid, how: How) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO star_opening (user_id, artist_mbid, how)
         SELECT $1, COALESCE((SELECT a.mbid FROM artist_redirect r JOIN artist a ON a.id = r.artist_id WHERE r.mbid = $2), $2), $3
         FROM user_profile p
         WHERE p.user_id = $1 AND p.mode IS DISTINCT FROM 'create'
         ON CONFLICT (user_id, artist_mbid) DO NOTHING",
    )
    .bind(user)
    .bind(mbid)
    .bind(how.as_str())
    .execute(conn)
    .await?;
    Ok(())
}

/// A star a browser remembers opening before anyone signed in, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Brought {
    pub mbid: Uuid,
    /// Milliseconds since the epoch, by the browser's clock.
    pub at: i64,
}

/// Takes in what a browser remembers, keeping the earliest moment of each star.
///
/// The moment is the browser's word for it, held to what cannot be untrue: not
/// before the epoch and not after now.
pub async fn bring(conn: &mut PgConnection, user: i32, stars: &[Brought]) -> sqlx::Result<()> {
    let now = i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX);
    let mbids: Vec<Uuid> = stars.iter().map(|star| star.mbid).collect();
    let moments: Vec<i64> = stars.iter().map(|star| star.at.clamp(0, now)).collect();
    sqlx::query(
        "INSERT INTO star_opening (user_id, artist_mbid, opened_at, how)
         SELECT $1, t.mbid, min(t.at), 'brought'
         FROM (
             SELECT COALESCE(a.mbid, b.mbid) AS mbid, to_timestamp(b.at / 1000.0) AS at
             FROM unnest($2::uuid[], $3::bigint[]) AS b (mbid, at)
             LEFT JOIN artist_redirect r ON r.mbid = b.mbid
             LEFT JOIN artist a ON a.id = r.artist_id
         ) t
         WHERE EXISTS (SELECT 1 FROM user_profile p WHERE p.user_id = $1 AND p.mode IS DISTINCT FROM 'create')
         GROUP BY t.mbid
         ON CONFLICT (user_id, artist_mbid) DO UPDATE SET opened_at = LEAST(star_opening.opened_at, EXCLUDED.opened_at)",
    )
    .bind(user)
    .bind(&mbids)
    .bind(&moments)
    .execute(conn)
    .await?;
    Ok(())
}

/// A star of the current sky this person has opened.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub listeners: f32,
    pub year: i16,
    /// When it was first opened, by either record.
    pub at: OffsetDateTime,
}

/// Every star of the current sky this person has opened, by whichever record.
pub async fn opened(conn: &mut PgConnection, user: i32) -> sqlx::Result<Vec<Opened>> {
    let rows = sqlx::query_as::<_, (i32, String, f32, f32, f32, Option<i16>, OffsetDateTime)>(
        "WITH opened AS (
             SELECT mbid, min(at) AS at FROM (
                 SELECT artist AS mbid, listened_at AS at FROM listen, unnest(artist_mbids) AS artist WHERE user_id = $1
                 UNION ALL
                 SELECT artist_mbid, opened_at FROM star_opening WHERE user_id = $1
             ) both_records
             GROUP BY mbid
         )
         SELECT a.id, a.name, p.x, p.y, COALESCE(al.listeners, 0)::real, COALESCE(a.begin_year, f.inception_year), o.at
         FROM opened o
         JOIN artist a ON a.mbid = o.mbid
         JOIN artist_position p ON p.artist_id = a.id AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         LEFT JOIN artist_listeners al ON al.artist_id = a.id
         LEFT JOIN artist_fact f ON f.artist_id = a.id
         ORDER BY a.id",
    )
    .bind(user)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, x, y, listeners, year, at)| Opened {
            id,
            name,
            x,
            y,
            listeners,
            year: crate::layout::cut::plausible_year(year),
            at,
        })
        .collect())
}

/// How many unheard stars one open star reflects (bank of mechanics, 17).
const ECHOES_PER_STAR: i64 = 3;

/// A star not yet heard that an open star reflects, and which ones do.
#[derive(Debug, Clone, PartialEq)]
pub struct Echo {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub of: Vec<i32>,
}

/// The closest unheard stars of each open star, by the similarity the current
/// sky was laid out from.
///
/// Set-based rather than one question per star: a person with a long history
/// holds tens of thousands of open stars, and a lateral lookup that also has to
/// ask "is this one open?" of each neighbour grows with both. The neighbours are
/// read once, the open ones dropped with a hash, and then each star keeps its
/// best.
pub async fn echoes(conn: &mut PgConnection, opened: &[i32]) -> sqlx::Result<Vec<Echo>> {
    if opened.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (i32, String, f32, f32, Vec<i32>)>(
        "WITH shown AS MATERIALIZED (
             SELECT id, metric_id FROM sky_layout ORDER BY created_at DESC LIMIT 1
         ),
         open_star AS MATERIALIZED (
             SELECT DISTINCT unnest($1::int[]) AS id
         ),
         edge AS (
             SELECT e.source_id AS star, e.target_id AS other, e.score
             FROM artist_similarity e, shown
             WHERE e.metric_id = shown.metric_id AND e.source_id = ANY($1)
             UNION ALL
             SELECT e.target_id, e.source_id, e.score
             FROM artist_similarity e, shown
             WHERE e.metric_id = shown.metric_id AND e.target_id = ANY($1)
         ),
         ranked AS (
             SELECT edge.star, edge.other, p.x, p.y,
                    row_number() OVER (PARTITION BY edge.star ORDER BY edge.score DESC, edge.other) AS place
             FROM edge
             JOIN shown ON true
             JOIN artist_position p ON p.layout_id = shown.id AND p.artist_id = edge.other
             WHERE NOT EXISTS (SELECT 1 FROM open_star s WHERE s.id = edge.other)
         )
         SELECT a.id, a.name, r.x, r.y, array_agg(r.star ORDER BY r.star)
         FROM ranked r
         JOIN artist a ON a.id = r.other
         WHERE r.place <= $2
         GROUP BY a.id, a.name, r.x, r.y
         ORDER BY a.id",
    )
    .bind(opened)
    .bind(ECHOES_PER_STAR)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|(id, name, x, y, of)| Echo { id, name, x, y, of }).collect())
}

/// What one carry into a new edition did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Carried {
    pub edition: i16,
    pub profiles: u64,
    /// Openings moved onto the star their id was merged into.
    pub openings: u64,
    /// Listens whose credited artists were rewritten the same way.
    pub listens: u64,
}

/// Carries every profile that is behind into the newest edition of the canon.
///
/// Run at the end of filling a stand, and whenever a server starts, so a stand
/// filled by hand catches up on its next restart. A person is carried in one
/// transaction under their ledger's lock and marked as they are: an interrupted
/// run leaves the rest behind and they are carried by the next. With no edition
/// sealed there is nothing to be carried into.
pub async fn carry_over(conn: &mut PgConnection) -> sqlx::Result<Option<Carried>> {
    let Some(edition) = sqlx::query_scalar::<_, Option<i16>>("SELECT max(id) FROM canon_edition")
        .fetch_one(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let behind: Vec<i32> = sqlx::query_scalar("SELECT user_id FROM user_profile WHERE canon_edition IS DISTINCT FROM $1 ORDER BY user_id")
        .bind(edition)
        .fetch_all(&mut *conn)
        .await?;

    let mut carried = Carried { edition, ..Carried::default() };
    for user in behind {
        let mut tx = conn.begin().await?;
        let (openings, listens) = carry_one(&mut tx, user, edition).await?;
        tx.commit().await?;
        carried.profiles += 1;
        carried.openings += openings;
        carried.listens += listens;
    }
    Ok(Some(carried))
}

/// Carries whoever is behind, and says so in the log: what a command and a
/// starting server both want.
pub async fn run(pool: &PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;
    match carry_over(&mut conn).await? {
        Some(carried) if carried.profiles > 0 => tracing::info!(
            edition = carried.edition,
            profiles = carried.profiles,
            openings = carried.openings,
            listens = carried.listens,
            "carried people's progress into the newest edition of the canon"
        ),
        _ => tracing::debug!("nobody's progress is behind the canon"),
    }
    Ok(())
}

/// Rewrites one person's progress through the redirects. Inside the caller's
/// transaction. Returns how many openings and how many listens it moved.
async fn carry_one(conn: &mut PgConnection, user: i32, edition: i16) -> sqlx::Result<(u64, u64)> {
    hold_ledger(conn, user).await?;

    // Two ids merged into one may both be open, or the survivor may be open
    // already: the star was first opened at the earliest of them, and by
    // whatever opened it then.
    let openings = sqlx::query_as::<_, (Uuid, OffsetDateTime, String)>(
        "DELETE FROM star_opening o
         USING artist_redirect r JOIN artist a ON a.id = r.artist_id
         WHERE o.user_id = $1 AND o.artist_mbid = r.mbid
         RETURNING a.mbid, o.opened_at, o.how",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let moved = openings.len() as u64;

    let mut earliest: HashMap<Uuid, (OffsetDateTime, String)> = HashMap::new();
    for (mbid, at, how) in openings {
        match earliest.get(&mbid) {
            Some((first, _)) if *first <= at => {}
            _ => {
                earliest.insert(mbid, (at, how));
            }
        }
    }
    for (mbid, (at, how)) in earliest {
        sqlx::query(
            "INSERT INTO star_opening (user_id, artist_mbid, opened_at, how) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, artist_mbid) DO UPDATE SET
                 how = CASE WHEN EXCLUDED.opened_at < star_opening.opened_at THEN EXCLUDED.how ELSE star_opening.how END,
                 opened_at = LEAST(star_opening.opened_at, EXCLUDED.opened_at)",
        )
        .bind(user)
        .bind(mbid)
        .bind(at)
        .bind(how)
        .execute(&mut *conn)
        .await?;
    }

    // The ledger's own key is the moment and the recording, so only who a listen
    // was by changes: credit order kept, and two credited ids that became one
    // listed once. The list of ids is worked out first so that the rows it does
    // not name are never touched.
    let merged: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT r.mbid
         FROM listen l, unnest(l.artist_mbids) AS artist
         JOIN artist_redirect r ON r.mbid = artist
         WHERE l.user_id = $1",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let listens = if merged.is_empty() {
        0
    } else {
        sqlx::query(
            "UPDATE listen l
             SET artist_mbids = ARRAY(
                 SELECT credit.mbid FROM (
                     SELECT COALESCE(a.mbid, c.mbid) AS mbid, min(c.place) AS place
                     FROM unnest(l.artist_mbids) WITH ORDINALITY AS c (mbid, place)
                     LEFT JOIN artist_redirect r ON r.mbid = c.mbid
                     LEFT JOIN artist a ON a.id = r.artist_id
                     GROUP BY 1
                 ) credit
                 ORDER BY credit.place
             )
             WHERE l.user_id = $1 AND l.artist_mbids && $2::uuid[]",
        )
        .bind(user)
        .bind(&merged)
        .execute(&mut *conn)
        .await?
        .rows_affected()
    };

    sqlx::query("UPDATE user_profile SET canon_edition = $2 WHERE user_id = $1")
        .bind(user)
        .bind(edition)
        .execute(&mut *conn)
        .await?;
    Ok((moved, listens))
}

#[cfg(test)]
mod tests {
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// A schema of its own inside a transaction that is rolled back: the tables
    /// the fog reads, empty, whatever the database under the tests holds. No
    /// foreign keys come along, which is why the tests need no accounts.
    async fn sandbox(tx: &mut PgConnection) {
        let schema = format!("fog_{:016x}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&mut *tx)
            .await
            .unwrap();
        for table in [
            "artist",
            "artist_redirect",
            "artist_fact",
            "artist_listeners",
            "artist_similarity",
            "artist_position",
            "sky_layout",
            "similarity_metric",
            "canon_edition",
            "listen",
            "star_opening",
            "user_profile",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)"
            )))
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL search_path TO {schema}")))
            .execute(&mut *tx)
            .await
            .unwrap();
        run(tx, "INSERT INTO similarity_metric (id, key, description) VALUES (1, 'm', 'd')").await;
        run(
            tx,
            "INSERT INTO sky_layout (id, key, metric_id, description, seed) VALUES (1, 'first', 1, 'test', 1)",
        )
        .await;
    }

    async fn run(tx: &mut PgConnection, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_string())).execute(tx).await.unwrap();
    }

    async fn star(tx: &mut PgConnection, artist: i32, mbid: Uuid, name: &str) {
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, $3, $3)")
            .bind(artist)
            .bind(mbid)
            .bind(name)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES (1, $1, 0, 0)")
            .bind(artist)
            .execute(tx)
            .await
            .unwrap();
    }

    async fn profile(tx: &mut PgConnection, user: i32, mode: Option<&str>) {
        sqlx::query("INSERT INTO user_profile (user_id, mode) VALUES ($1, $2)")
            .bind(user)
            .bind(mode)
            .execute(tx)
            .await
            .unwrap();
    }

    async fn listen(tx: &mut PgConnection, user: i32, at: i64, artists: &[Uuid]) {
        sqlx::query(
            "INSERT INTO listen (user_id, listened_at, recording, artist_mbids, seconds, light, source)
             VALUES ($1, to_timestamp($2), gen_random_uuid(), $3, 200, NULL, 'lastfm')",
        )
        .bind(user)
        .bind(at)
        .bind(artists)
        .execute(tx)
        .await
        .unwrap();
    }

    /// A pool on the test database, or `None` with a note when there is none:
    /// the matrix that runs on three systems has no Postgres, and the `database`
    /// job, which does, runs these.
    async fn database() -> Option<PgPool> {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the database half of the fog tests");
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

    async fn opened_ids(tx: &mut PgConnection, user: i32) -> Vec<i32> {
        opened(tx, user).await.unwrap().into_iter().map(|star| star.id).collect()
    }

    #[tokio::test]
    async fn a_star_is_open_by_either_record_and_from_the_earlier_of_them() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        star(&mut tx, 10, id(10), "Played").await;
        star(&mut tx, 20, id(20), "Scrobbled").await;
        star(&mut tx, 30, id(30), "Both").await;
        star(&mut tx, 40, id(40), "Unheard").await;

        open(&mut tx, 1, id(10), How::Played).await.unwrap();
        open(&mut tx, 1, id(30), How::Played).await.unwrap();
        listen(&mut tx, 1, 1_000_000, &[id(20)]).await;
        listen(&mut tx, 1, 2_000_000, &[id(30)]).await;

        let stars = opened(&mut tx, 1).await.unwrap();
        assert_eq!(stars.iter().map(|star| star.id).collect::<Vec<_>>(), [10, 20, 30]);
        // The listen is years older than the opening made a moment ago.
        assert_eq!(stars[2].at.unix_timestamp(), 2_000_000);

        // Somebody else's sky stays dark.
        assert_eq!(opened_ids(&mut tx, 2).await, Vec::<i32>::new());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_creative_profile_keeps_no_progress_and_choosing_it_forgets_any() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        star(&mut tx, 10, id(10), "A").await;
        star(&mut tx, 20, id(20), "B").await;

        // Not chosen yet: kept, because the choice may be the fog.
        profile(&mut tx, 1, None).await;
        open(&mut tx, 1, id(10), How::Played).await.unwrap();
        assert_eq!(opened_ids(&mut tx, 1).await, [10]);

        assert!(choose_mode(&mut tx, 1, Mode::Create).await.unwrap());
        assert_eq!(opened_ids(&mut tx, 1).await, Vec::<i32>::new(), "the creative mode has no progress");

        open(&mut tx, 1, id(20), How::Played).await.unwrap();
        bring(&mut tx, 1, &[Brought { mbid: id(20), at: 0 }]).await.unwrap();
        assert_eq!(opened_ids(&mut tx, 1).await, Vec::<i32>::new(), "and it takes none on afterwards");
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_mode_is_chosen_once() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, None).await;

        assert!(choose_mode(&mut tx, 1, Mode::Explore).await.unwrap());
        assert!(!choose_mode(&mut tx, 1, Mode::Create).await.unwrap(), "a second choice is refused");
        let mode: String = sqlx::query_scalar("SELECT mode FROM user_profile WHERE user_id = 1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(mode, "explore");
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn stars_a_browser_brings_land_where_the_canon_says_and_keep_the_earliest_moment() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        star(&mut tx, 10, id(10), "Survivor").await;
        star(&mut tx, 20, id(20), "Another").await;
        // A browser remembers an id the canon has since merged into star 10.
        sqlx::query("INSERT INTO artist_redirect (mbid, artist_id) VALUES ($1, 10)")
            .bind(id(99))
            .execute(&mut *tx)
            .await
            .unwrap();

        bring(
            &mut tx,
            1,
            &[
                Brought { mbid: id(99), at: 3_000_000 },
                Brought { mbid: id(10), at: 5_000_000 },
                // A clock from the future is held to now rather than failing
                // the whole request.
                Brought { mbid: id(20), at: i64::MAX },
            ],
        )
        .await
        .unwrap();

        let stars = opened(&mut tx, 1).await.unwrap();
        assert_eq!(stars.iter().map(|star| star.id).collect::<Vec<_>>(), [10, 20]);
        // The earlier moment was under the id that no longer exists: read as a
        // separate row, it would light nothing and the star would seem newer.
        assert_eq!(stars[0].at.unix_timestamp(), 3_000, "the earliest of the two ids that became one");
        assert!(stars[1].at <= OffsetDateTime::now_utc(), "no moment after now");

        let how: String = sqlx::query_scalar("SELECT how FROM star_opening WHERE artist_mbid = $1")
            .bind(id(10))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(how, "brought", "only the browser said so, and the row says that");
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn an_id_the_canon_merged_opens_the_star_it_became() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        star(&mut tx, 10, id(10), "Survivor").await;
        sqlx::query("INSERT INTO artist_redirect (mbid, artist_id) VALUES ($1, 10)")
            .bind(id(99))
            .execute(&mut *tx)
            .await
            .unwrap();

        // A page left open across an update of the canon still names the old id.
        open(&mut tx, 1, id(99), How::Played).await.unwrap();
        assert_eq!(opened_ids(&mut tx, 1).await, [10]);

        // And an id nothing knows is still kept: the canon may hold it next year.
        open(&mut tx, 1, id(77), How::Followed).await.unwrap();
        assert_eq!(openings_of(&mut tx, 1).await.len(), 2);
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn an_open_star_echoes_its_three_closest_unheard_stars() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        for n in 1..=7_u8 {
            star(&mut tx, n.into(), id(n.into()), &format!("Star {n}")).await;
        }
        // Not on the sky, so it cannot be anybody's echo however close it is.
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES (8, $1, 'Off the sky', 'Off the sky')")
            .bind(id(8))
            .execute(&mut *tx)
            .await
            .unwrap();

        // Stored once per pair with the lower id first, so a neighbour can be
        // on either side of the edge.
        for (source, target, score) in [(1, 2, 0.9_f32), (1, 3, 0.8), (1, 4, 0.7), (1, 5, 0.6), (1, 6, 0.5), (1, 8, 0.99), (6, 7, 0.9)] {
            sqlx::query("INSERT INTO artist_similarity (metric_id, source_id, target_id, score) VALUES (1, $1, $2, $3)")
                .bind(source)
                .bind(target)
                .bind(score)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // Star 2 is open, so the next-closest takes its place.
        let got = echoes(&mut tx, &[1, 2]).await.unwrap();
        assert_eq!(got.iter().map(|echo| echo.id).collect::<Vec<_>>(), [3, 4, 5]);
        assert_eq!(got[0].of, [1]);

        // The same star reflected by two open ones is one silhouette, naming both.
        let got = echoes(&mut tx, &[1, 7]).await.unwrap();
        let six = got.iter().find(|echo| echo.id == 6);
        assert!(six.is_some_and(|echo| echo.of == [7]), "star 7's only neighbour echoes it: {got:?}");
        assert_eq!(echoes(&mut tx, &[]).await.unwrap(), Vec::<Echo>::new());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn the_echoes_of_a_second_metric_do_not_mix_with_the_sky_on_show() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        run(&mut tx, "INSERT INTO similarity_metric (id, key, description) VALUES (2, 'older', 'd')").await;
        star(&mut tx, 1, id(1), "One").await;
        star(&mut tx, 2, id(2), "Two").await;
        star(&mut tx, 3, id(3), "Three").await;
        run(
            &mut tx,
            "INSERT INTO artist_similarity (metric_id, source_id, target_id, score) VALUES (1, 1, 2, 0.1), (2, 1, 3, 0.9)",
        )
        .await;

        let got = echoes(&mut tx, &[1]).await.unwrap();
        assert_eq!(got.iter().map(|echo| echo.id).collect::<Vec<_>>(), [2]);
        tx.rollback().await.unwrap();
    }

    /// One row per star of what a person holds, the way carrying must leave it.
    async fn openings_of(tx: &mut PgConnection, user: i32) -> Vec<(Uuid, i64, String)> {
        sqlx::query_as("SELECT artist_mbid, extract(epoch FROM opened_at)::bigint, how FROM star_opening WHERE user_id = $1 ORDER BY artist_mbid")
            .bind(user)
            .fetch_all(tx)
            .await
            .unwrap()
    }

    async fn opening(tx: &mut PgConnection, user: i32, mbid: Uuid, at: i64, how: &str) {
        sqlx::query("INSERT INTO star_opening (user_id, artist_mbid, opened_at, how) VALUES ($1, $2, to_timestamp($3), $4)")
            .bind(user)
            .bind(mbid)
            .bind(at)
            .bind(how)
            .execute(tx)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn carrying_moves_a_merged_star_and_loses_nothing() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        star(&mut tx, 10, id(10), "Survivor").await;
        star(&mut tx, 20, id(20), "Untouched").await;
        star(&mut tx, 30, id(30), "Other survivor").await;
        for gone in [91, 92, 93] {
            let target = if gone == 93 { 30 } else { 10 };
            sqlx::query("INSERT INTO artist_redirect (mbid, artist_id) VALUES ($1, $2)")
                .bind(id(gone))
                .bind(target)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // Two merged ids and the survivor itself, all opened at different
        // moments; a star that did not change; an id with no redirect that the
        // canon no longer holds; and one merged id alone.
        opening(&mut tx, 1, id(91), 3000, "played").await;
        opening(&mut tx, 1, id(92), 1000, "brought").await;
        opening(&mut tx, 1, id(10), 2000, "followed").await;
        opening(&mut tx, 1, id(20), 4000, "played").await;
        opening(&mut tx, 1, id(77), 5000, "played").await;
        opening(&mut tx, 1, id(93), 6000, "followed").await;

        // Listens credited to merged ids, in order, one of them to two ids that
        // are now one artist.
        listen(&mut tx, 1, 7000, &[id(91), id(20), id(10), id(92)]).await;
        listen(&mut tx, 1, 8000, &[id(20)]).await;

        let (openings, listens) = carry_one(&mut tx, 1, 3).await.unwrap();
        assert_eq!(openings, 3, "every opening under a merged id moved, and no other");
        assert_eq!(listens, 1, "the listen naming no merged id was not touched");

        assert_eq!(
            openings_of(&mut tx, 1).await,
            [
                (id(10), 1000, "brought".to_string()), // the earliest moment, and what opened it then
                (id(20), 4000, "played".to_string()),
                (id(30), 6000, "followed".to_string()),
                (id(77), 5000, "played".to_string()), // kept: the canon may hold it again
            ]
        );

        let credits: Vec<Vec<Uuid>> = sqlx::query_scalar("SELECT artist_mbids FROM listen WHERE user_id = 1 ORDER BY listened_at")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(credits, [vec![id(10), id(20)], vec![id(20)]], "credit order kept, and one artist listed once");

        // The stars themselves are still open, under the ids the canon holds.
        assert_eq!(opened_ids(&mut tx, 1).await, [10, 20, 30]);

        let edition: Option<i16> = sqlx::query_scalar("SELECT canon_edition FROM user_profile WHERE user_id = 1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(edition, Some(3));

        // Carrying again changes nothing.
        assert_eq!(carry_one(&mut tx, 1, 3).await.unwrap(), (0, 0));
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn only_the_profiles_behind_the_newest_edition_are_carried() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        star(&mut tx, 10, id(10), "Survivor").await;
        sqlx::query("INSERT INTO artist_redirect (mbid, artist_id) VALUES ($1, 10)")
            .bind(id(91))
            .execute(&mut *tx)
            .await
            .unwrap();

        // No edition yet: nothing to be carried into, and nothing is touched.
        run(&mut tx, "INSERT INTO user_profile (user_id, mode) VALUES (1, NULL)").await;
        opening(&mut tx, 1, id(91), 1000, "played").await;
        assert_eq!(carry_over(&mut tx).await.unwrap(), None);
        assert_eq!(openings_of(&mut tx, 1).await[0].0, id(91));

        run(
            &mut tx,
            "INSERT INTO canon_edition (id, layout_key, artists, stars) VALUES (1, 'one', 1, 1), (2, 'two', 1, 1)",
        )
        .await;
        run(
            &mut tx,
            "INSERT INTO user_profile (user_id, mode, canon_edition) VALUES (2, 'explore', 1), (3, 'explore', 2), (4, 'create', 1)",
        )
        .await;
        // Someone already in the newest edition is not read again: this row
        // would move if they were, and it must not.
        opening(&mut tx, 2, id(91), 1000, "played").await;
        opening(&mut tx, 3, id(91), 1000, "played").await;
        // A creative profile keeps no openings, but its listens are real and
        // they move like anyone's.
        listen(&mut tx, 4, 1000, &[id(91)]).await;

        let carried = carry_over(&mut tx).await.unwrap().unwrap();
        assert_eq!(
            carried,
            Carried {
                edition: 2,
                profiles: 3,
                openings: 2,
                listens: 1
            }
        );
        assert_eq!(openings_of(&mut tx, 1).await[0].0, id(10));
        assert_eq!(openings_of(&mut tx, 2).await[0].0, id(10));
        assert_eq!(openings_of(&mut tx, 3).await[0].0, id(91), "already in the newest edition, so not carried");

        // A second run finds nobody behind.
        assert_eq!(
            carry_over(&mut tx).await.unwrap().unwrap(),
            Carried {
                edition: 2,
                ..Carried::default()
            }
        );
        tx.rollback().await.unwrap();
    }

    #[test]
    fn the_client_says_how_only_in_the_two_words_that_are_its_to_say() {
        assert_eq!(serde_json::from_str::<How>("\"played\"").unwrap(), How::Played);
        assert_eq!(serde_json::from_str::<How>("\"followed\"").unwrap(), How::Followed);
        // `brought` is decided by the route a star arrived through. A client
        // that could claim it, or anything else, would make the column a guess.
        assert!(serde_json::from_str::<How>("\"brought\"").is_err());
        assert!(serde_json::from_str::<How>("\"scrobbled\"").is_err());
    }

    #[test]
    fn the_modes_are_the_ones_the_schema_accepts() {
        let migration = include_str!("../migrations/0008_accounts.sql");
        let constraint = migration.lines().find(|line| line.contains("CHECK (mode IN")).expect("0008 declares the modes");
        for mode in [Mode::Explore, Mode::Create] {
            assert!(constraint.contains(&format!("'{}'", mode.as_str())), "{mode:?} is not in {constraint}");
        }
        assert!(serde_json::from_str::<Mode>("\"administrator\"").is_err());
    }
}
