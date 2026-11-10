//! The expedition: naming an artist you love, setting out from that star, and
//! the contracts the game hands you on the way.
//!
//! A person in the exploration mode starts in the dark. The expedition gives
//! that dark a first star: they name an artist they love, the star becomes
//! their home port and is open from that moment (they have heard it), and the
//! first contract asks them to chart the home sector -- to open three new stars
//! of its neighbourhood. The neighbourhood is the star's neighbours by the
//! similarity the sky on show was laid out from, which is exactly where its
//! echoes come from, so the dashed rings on the sky are the way to fulfil it.
//! See ADR 0028.
//!
//! Progress is read from the openings and never stored: the openings are the
//! record of what was heard, and a count kept beside them could only drift. A
//! contract stores the moment it was fulfilled, worked out from the openings
//! too -- the moment of the opening that reached the goal -- so it is the same
//! whenever someone happens to look.

use serde::Serialize;
use sqlx::{Connection, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::import::special::special_purpose;

/// How many new stars of the home sector the first contract asks for: the
/// three echoes a home port shows, which is a next step that is always in
/// sight.
pub const FIRST_GOAL: usize = 3;

/// Why an expedition could not begin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Only the exploration mode has a dark to set out into.
    NotExploring,
    /// An expedition begins once.
    Begun,
    /// The star is not on the sky on show, or is not an act.
    NotOnSky,
}

/// A star as the expedition names it: by MusicBrainz id always, and with its
/// place when the sky on show holds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Port {
    pub mbid: Uuid,
    pub id: Option<i32>,
    pub name: Option<String>,
    pub x: Option<f32>,
    pub y: Option<f32>,
}

/// A contract, with how far it has come.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub id: i32,
    pub kind: String,
    pub star: Port,
    pub goal: i16,
    /// New stars of the sector opened since it was issued, up to the goal.
    pub progress: i16,
    pub issued_at: OffsetDateTime,
    pub done_at: Option<OffsetDateTime>,
}

/// Sets out from the star with this MusicBrainz id: makes it the home port,
/// opens it as named, and issues the first contract. All of it or none.
///
/// An id the canon has since merged lands on the star it became. The first
/// contract asks for [`FIRST_GOAL`] stars, or as many of the neighbourhood as
/// are still unopened when that is fewer -- someone whose history lit the home
/// sector already is not asked for what cannot be done, and with nothing left
/// it is fulfilled as it is issued.
pub async fn begin(conn: &mut PgConnection, user: i32, mbid: Uuid) -> sqlx::Result<Result<Port, Refusal>> {
    let mut tx = conn.begin().await?;

    // Locked, so two tabs naming two artists at once cannot both begin.
    let profile = sqlx::query_as::<_, (Option<String>, Option<Uuid>)>("SELECT mode, home_port FROM user_profile WHERE user_id = $1 FOR UPDATE")
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?;
    match profile {
        Some((Some(mode), _)) if mode != "explore" => return Ok(Err(Refusal::NotExploring)),
        None | Some((None, _)) => return Ok(Err(Refusal::NotExploring)),
        Some((_, Some(_))) => return Ok(Err(Refusal::Begun)),
        Some((_, None)) => {}
    }

    let star = sqlx::query_as::<_, (i32, Uuid, String, f32, f32)>(
        "SELECT a.id, a.mbid, a.name, p.x, p.y
         FROM artist a
         JOIN artist_position p ON p.artist_id = a.id
             AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         WHERE a.mbid = COALESCE((SELECT t.mbid FROM artist_redirect r JOIN artist t ON t.id = r.artist_id WHERE r.mbid = $1), $1)
           AND a.mbid <> ALL($2::uuid[])",
    )
    .bind(mbid)
    .bind(special_purpose())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, mbid, name, x, y)) = star else {
        return Ok(Err(Refusal::NotOnSky));
    };

    sqlx::query("UPDATE user_profile SET home_port = $2 WHERE user_id = $1")
        .bind(user)
        .bind(mbid)
        .execute(&mut *tx)
        .await?;
    // Already open -- heard elsewhere, or here before -- stays open the way it
    // was: the first record of a star is the one that says how it was found.
    sqlx::query("INSERT INTO star_opening (user_id, artist_mbid, how) VALUES ($1, $2, 'named') ON CONFLICT (user_id, artist_mbid) DO NOTHING")
        .bind(user)
        .bind(mbid)
        .execute(&mut *tx)
        .await?;

    let unopened = sector(&mut tx, user, id).await?.iter().filter(|(_, first)| first.is_none()).count();
    let goal = i16::try_from(unopened.min(FIRST_GOAL)).unwrap_or(0);
    sqlx::query(
        "INSERT INTO contract (user_id, kind, star_mbid, goal, done_at)
         VALUES ($1, 'home_sector', $2, $3, CASE WHEN $3 = 0 THEN now() END)",
    )
    .bind(user)
    .bind(mbid)
    .bind(goal)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Ok(Port {
        mbid,
        id: Some(id),
        name: Some(name),
        x: Some(x),
        y: Some(y),
    }))
}

/// The home port, where the sky on show puts it, or `None` before the
/// expedition has begun.
pub async fn home_port(conn: &mut PgConnection, user: i32) -> sqlx::Result<Option<Port>> {
    let row = sqlx::query_as::<_, (Option<Uuid>, Option<i32>, Option<String>, Option<f32>, Option<f32>)>(
        "SELECT pr.home_port, a.id, a.name, p.x, p.y
         FROM user_profile pr
         LEFT JOIN artist a ON a.mbid = pr.home_port
         LEFT JOIN artist_position p ON p.artist_id = a.id
             AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         WHERE pr.user_id = $1",
    )
    .bind(user)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|(mbid, id, name, x, y)| mbid.map(|mbid| placed(mbid, id, name, x, y))))
}

/// Every contract of this person, oldest first, with its progress -- and any
/// that the openings have fulfilled since the last look are marked so here, at
/// the moment of the opening that did it.
pub async fn contracts(conn: &mut PgConnection, user: i32) -> sqlx::Result<Vec<Contract>> {
    type Row = (
        i32,
        String,
        Uuid,
        i16,
        OffsetDateTime,
        Option<OffsetDateTime>,
        Option<i32>,
        Option<String>,
        Option<f32>,
        Option<f32>,
    );
    let rows = sqlx::query_as::<_, Row>(
        "SELECT c.id, c.kind, c.star_mbid, c.goal, c.issued_at, c.done_at, a.id, a.name, p.x, p.y
         FROM contract c
         LEFT JOIN artist a ON a.mbid = c.star_mbid
         LEFT JOIN artist_position p ON p.artist_id = a.id
             AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         WHERE c.user_id = $1
         ORDER BY c.issued_at, c.id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;

    let mut contracts = Vec::with_capacity(rows.len());
    for (id, kind, mbid, goal, issued_at, mut done_at, artist, name, x, y) in rows {
        let star = placed(mbid, artist, name, x, y);
        let progress = match (done_at, star.id.filter(|_| star.x.is_some())) {
            (Some(_), _) => goal,
            // A star the sky on show does not hold has no neighbourhood today;
            // the contract waits for a canon that holds it again.
            (None, None) => 0,
            (None, Some(artist)) => {
                let reached = fulfilled(&sector(conn, user, artist).await?, issued_at, goal);
                if let Some(at) = reached.done_at {
                    sqlx::query("UPDATE contract SET done_at = $2 WHERE id = $1 AND done_at IS NULL")
                        .bind(id)
                        .bind(at)
                        .execute(&mut *conn)
                        .await?;
                    done_at = Some(at);
                }
                reached.progress
            }
        };
        contracts.push(Contract {
            id,
            kind,
            star,
            goal,
            progress,
            issued_at,
            done_at,
        });
    }
    Ok(contracts)
}

fn placed(mbid: Uuid, id: Option<i32>, name: Option<String>, x: Option<f32>, y: Option<f32>) -> Port {
    // A place without both coordinates is no place.
    let (x, y) = match (x, y) {
        (Some(x), Some(y)) => (Some(x), Some(y)),
        _ => (None, None),
    };
    Port { mbid, id, name, x, y }
}

/// How far a contract has come, from its sector's openings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reached {
    progress: i16,
    done_at: Option<OffsetDateTime>,
}

/// New stars are the ones first opened at or after the issue -- a star heard
/// last year is not a discovery of this contract, however recently it was
/// heard again -- and the contract was fulfilled when the goal-th of them was.
fn fulfilled(sector: &[(Uuid, Option<OffsetDateTime>)], issued_at: OffsetDateTime, goal: i16) -> Reached {
    let mut new: Vec<OffsetDateTime> = sector.iter().filter_map(|(_, first)| *first).filter(|first| *first >= issued_at).collect();
    new.sort_unstable();
    let wanted = usize::try_from(goal).unwrap_or(0);
    let done_at = wanted.checked_sub(1).and_then(|at| new.get(at)).copied();
    Reached {
        progress: i16::try_from(new.len().min(wanted)).unwrap_or(goal),
        done_at,
    }
}

/// The neighbourhood of a star on the sky on show, and when this person first
/// opened each of its stars, by either record -- `None` for the ones still
/// dark to them. Only placed stars, and no special purpose artist: the same
/// neighbours the star's echoes are drawn from.
async fn sector(conn: &mut PgConnection, user: i32, star: i32) -> sqlx::Result<Vec<(Uuid, Option<OffsetDateTime>)>> {
    sqlx::query_as(
        "WITH shown AS MATERIALIZED (
             SELECT id, metric_id FROM sky_layout ORDER BY created_at DESC LIMIT 1
         ),
         neighbour AS MATERIALIZED (
             SELECT DISTINCT a.mbid
             FROM (
                 SELECT e.target_id AS id FROM artist_similarity e, shown
                 WHERE e.metric_id = shown.metric_id AND e.source_id = $2
                 UNION ALL
                 SELECT e.source_id FROM artist_similarity e, shown
                 WHERE e.metric_id = shown.metric_id AND e.target_id = $2
             ) n
             JOIN shown ON true
             JOIN artist_position p ON p.layout_id = shown.id AND p.artist_id = n.id
             JOIN artist a ON a.id = n.id
             WHERE a.mbid <> ALL($3::uuid[])
         ),
         first AS (
             SELECT mbid, min(at) AS at FROM (
                 SELECT artist AS mbid, l.listened_at AS at
                 FROM listen l, unnest(l.artist_mbids) AS artist
                 WHERE l.user_id = $1 AND artist IN (SELECT mbid FROM neighbour)
                 UNION ALL
                 SELECT o.artist_mbid, o.opened_at FROM star_opening o
                 WHERE o.user_id = $1 AND o.artist_mbid IN (SELECT mbid FROM neighbour)
             ) both_records
             GROUP BY mbid
         )
         SELECT n.mbid, f.at FROM neighbour n LEFT JOIN first f ON f.mbid = n.mbid
         ORDER BY n.mbid",
    )
    .bind(user)
    .bind(star)
    .bind(special_purpose())
    .fetch_all(conn)
    .await
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use sqlx::postgres::PgPoolOptions;

    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(seconds).unwrap()
    }

    #[test]
    fn a_contract_counts_only_stars_first_opened_after_it_was_issued() {
        let sector = [
            (id(1), Some(at(50))),  // heard before the contract: not a discovery of it
            (id(2), Some(at(120))), // the first new star
            (id(3), None),          // still dark
            (id(4), Some(at(100))), // opened at the moment it was issued: counts
        ];
        assert_eq!(fulfilled(&sector, at(100), 3), Reached { progress: 2, done_at: None });
    }

    #[test]
    fn a_contract_is_fulfilled_at_the_opening_that_reached_its_goal() {
        let sector = [(id(1), Some(at(300))), (id(2), Some(at(150))), (id(3), Some(at(200))), (id(4), Some(at(400)))];
        // The third new star in time, not the last one in the list or the
        // moment somebody looked.
        assert_eq!(
            fulfilled(&sector, at(100), 3),
            Reached {
                progress: 3,
                done_at: Some(at(300))
            }
        );
    }

    #[test]
    fn a_contract_with_nothing_to_ask_for_has_no_progress_to_make() {
        assert_eq!(fulfilled(&[(id(1), Some(at(200)))], at(100), 0), Reached { progress: 0, done_at: None });
    }

    /// A schema of its own inside a transaction that is rolled back: the tables
    /// the expedition reads, empty, whatever the database under the tests holds.
    async fn sandbox(tx: &mut PgConnection) {
        let schema = format!("expedition_{:016x}", rand::random::<u64>());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&mut *tx)
            .await
            .unwrap();
        for table in [
            "artist",
            "artist_redirect",
            "artist_similarity",
            "artist_position",
            "sky_layout",
            "similarity_metric",
            "listen",
            "star_opening",
            "user_profile",
            "contract",
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
        // The home port and five neighbours, one of them off the sky and one a
        // special purpose artist; and a stranger with no edge to any of them.
        for n in 1..=4_u8 {
            star(tx, n.into(), id(n.into()), &format!("Star {n}")).await;
        }
        star(tx, 9, id(9), "A stranger").await;
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES (5, $1, 'Off the sky', 'Off the sky')")
            .bind(id(5))
            .execute(&mut *tx)
            .await
            .unwrap();
        let various = Uuid::parse_str(crate::import::special::SPECIAL_PURPOSE[0]).unwrap();
        star(tx, 6, various, "Various Artists").await;
        run(
            tx,
            "INSERT INTO artist_similarity (metric_id, source_id, target_id, score)
             VALUES (1, 1, 2, 0.9), (1, 1, 3, 0.8), (1, 1, 4, 0.7), (1, 1, 5, 0.99), (1, 1, 6, 0.95)",
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

    async fn open(tx: &mut PgConnection, user: i32, mbid: Uuid, at: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO star_opening (user_id, artist_mbid, opened_at, how) VALUES ($1, $2, {at}, 'played')"
        )))
        .bind(user)
        .bind(mbid)
        .execute(tx)
        .await
        .unwrap();
    }

    /// A pool on the test database, or `None` with a note when there is none.
    async fn database() -> Option<PgPool> {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set; skipping the database half of the expedition tests");
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

    async fn openings(tx: &mut PgConnection, user: i32) -> Vec<(Uuid, String)> {
        sqlx::query_as("SELECT artist_mbid, how FROM star_opening WHERE user_id = $1 ORDER BY artist_mbid")
            .bind(user)
            .fetch_all(tx)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn an_expedition_sets_out_from_a_star_once_and_only_in_the_dark() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        profile(&mut tx, 2, Some("create")).await;
        profile(&mut tx, 3, None).await;

        // Not an act, not on the sky, or not a mode with a dark to set out into.
        let various = Uuid::parse_str(crate::import::special::SPECIAL_PURPOSE[0]).unwrap();
        assert_eq!(begin(&mut tx, 1, various).await.unwrap(), Err(Refusal::NotOnSky));
        assert_eq!(begin(&mut tx, 1, id(5)).await.unwrap(), Err(Refusal::NotOnSky));
        assert_eq!(begin(&mut tx, 2, id(1)).await.unwrap(), Err(Refusal::NotExploring));
        assert_eq!(begin(&mut tx, 3, id(1)).await.unwrap(), Err(Refusal::NotExploring));
        assert_eq!(home_port(&mut tx, 1).await.unwrap(), None);

        let port = begin(&mut tx, 1, id(1)).await.unwrap().unwrap();
        assert_eq!((port.id, port.name.as_deref()), (Some(1), Some("Star 1")));
        assert_eq!(home_port(&mut tx, 1).await.unwrap().map(|port| port.mbid), Some(id(1)));
        // The star named is open, and says it was named.
        assert_eq!(openings(&mut tx, 1).await, [(id(1), "named".to_string())]);

        // Once: a second star is refused, and the first stays the home port.
        assert_eq!(begin(&mut tx, 1, id(2)).await.unwrap(), Err(Refusal::Begun));
        assert_eq!(home_port(&mut tx, 1).await.unwrap().map(|port| port.mbid), Some(id(1)));
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn the_first_contract_is_fulfilled_by_three_new_stars_of_the_home_sector() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        begin(&mut tx, 1, id(1)).await.unwrap().unwrap();

        let first = contracts(&mut tx, 1).await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].kind.as_str(), first[0].goal, first[0].progress), ("home_sector", 3, 0));
        assert_eq!(first[0].star.mbid, id(1));

        // A stranger is not of the sector, and opening it counts for nothing.
        open(&mut tx, 1, id(9), "now() + interval '1 minute'").await;
        open(&mut tx, 1, id(2), "now() + interval '2 minutes'").await;
        open(&mut tx, 1, id(3), "now() + interval '3 minutes'").await;
        let two = contracts(&mut tx, 1).await.unwrap();
        assert_eq!((two[0].progress, two[0].done_at), (2, None));

        open(&mut tx, 1, id(4), "now() + interval '4 minutes'").await;
        let done = contracts(&mut tx, 1).await.unwrap();
        assert_eq!(done[0].progress, 3);
        let done_at = done[0].done_at.expect("three new stars fulfil it");
        // At the opening that reached the goal, not when somebody looked.
        let fourth: OffsetDateTime = sqlx::query_scalar("SELECT opened_at FROM star_opening WHERE user_id = 1 AND artist_mbid = $1")
            .bind(id(4))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(done_at, fourth);
        // And it is written down, so a later look does not have to agree.
        let stored: Option<OffsetDateTime> = sqlx::query_scalar("SELECT done_at FROM contract WHERE user_id = 1")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(stored, Some(fourth));
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn a_home_sector_already_heard_asks_only_for_what_is_left() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        sandbox(&mut tx).await;
        profile(&mut tx, 1, Some("explore")).await;
        profile(&mut tx, 2, Some("explore")).await;
        // Two of the three neighbours on the sky were heard long ago.
        open(&mut tx, 1, id(2), "now() - interval '1 year'").await;
        open(&mut tx, 1, id(3), "now() - interval '1 year'").await;
        begin(&mut tx, 1, id(1)).await.unwrap().unwrap();
        let contract = &contracts(&mut tx, 1).await.unwrap()[0];
        assert_eq!((contract.goal, contract.progress, contract.done_at), (1, 0, None));

        // All of them heard: nothing to ask for, fulfilled as it was issued.
        for n in 2..=4 {
            open(&mut tx, 2, id(n), "now() - interval '1 year'").await;
        }
        begin(&mut tx, 2, id(1)).await.unwrap().unwrap();
        let contract = &contracts(&mut tx, 2).await.unwrap()[0];
        assert_eq!(contract.goal, 0);
        assert!(contract.done_at.is_some());
        tx.rollback().await.unwrap();
    }
}
