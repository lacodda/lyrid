//! Real listening, read from ListenBrainz, turned into openings and light.
//!
//! A person links their ListenBrainz account once, proving it is theirs with a
//! token that is checked and dropped. From then on lyrid reads what they
//! listen to -- in the background every quarter of an hour, or at once when
//! they ask -- and writes each listen into a ledger with the light it paid.
//! Openings are not stored at all: a star is open once a listen names it, and
//! the ledger already says so. See ADR 0019.
//!
//! Nothing here sits between a visitor and the sky. The card and the tiles
//! never wait for ListenBrainz; a read that fails leaves its reason on the
//! link and is tried again later.

pub mod client;
pub mod light;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use sqlx::{Connection, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

pub use client::{Client, Failure, Fetched, Listen};

/// How often each linked account is read in the background, in minutes.
pub const READ_EVERY_MINUTES: i32 = 15;

/// How soon a read by hand may follow the last read, in seconds. Enough that a
/// button pressed twice is one read, and short enough that "I just played
/// something" is answered while it is still true.
pub const BY_HAND_EVERY_SECONDS: i32 = 60;

/// How far behind the last read each read reaches back.
///
/// Players that lose their connection submit listens later, with the time they
/// were played: a read that began only where the last one ended would never
/// see them. Two days covers a phone left offline over a weekend, and the
/// ledger's key makes reading a listen twice cost nothing.
const OVERLAP: time::Duration = time::Duration::hours(48);

/// How long a listen ListenBrainz has not yet matched to an artist is given to
/// be matched before it is counted without one.
///
/// The mapping arrives after the listen does. Counting a listen the moment it
/// appears would pay it as unnamed and open nothing, and the ledger would
/// never look at it again; waiting a day lets the mapping catch up, and the
/// overlap above is long enough that the listen is still in the window when
/// the day is up.
const MAPPING_GRACE: time::Duration = time::Duration::hours(24);

/// A link taken for reading: whose it is and where reading stands.
#[derive(Debug, Clone)]
pub struct Claim {
    pub user_id: i32,
    pub name: String,
    pub linked_at: OffsetDateTime,
    pub read_through: Option<OffsetDateTime>,
}

impl Claim {
    /// Where this read starts: two days behind the last one, and never before
    /// the account was linked. Listening from before the link belongs to the
    /// history import, not to the link.
    pub fn from(&self) -> OffsetDateTime {
        match self.read_through {
            Some(through) => (through - OVERLAP).max(self.linked_at),
            None => self.linked_at,
        }
    }
}

/// What a read added.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Read {
    /// Listens written to the ledger by this read.
    pub listens: usize,
    /// The light they paid.
    pub light: i64,
    /// Artists heard for the first time.
    pub opened: usize,
}

/// Why a read did not finish.
#[derive(Debug)]
pub enum ReadError {
    ListenBrainz(Failure),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ReadError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

type ClaimRow = (i32, String, OffsetDateTime, Option<OffsetDateTime>);

fn claim_of((user_id, name, linked_at, read_through): ClaimRow) -> Claim {
    Claim {
        user_id,
        name,
        linked_at,
        read_through,
    }
}

/// Takes the link that has waited longest for its read, if any is due.
///
/// `SKIP LOCKED` and the stamp in one statement: two servers, or a server and
/// a button, can never take the same link at once, and a link taken is marked
/// as tried before anything slow happens.
pub async fn claim_due(conn: &mut PgConnection) -> sqlx::Result<Option<Claim>> {
    let row = sqlx::query_as::<_, ClaimRow>(
        "UPDATE listenbrainz_link SET tried_at = now()
         WHERE user_id = (
             SELECT user_id FROM listenbrainz_link
             WHERE tried_at IS NULL OR tried_at < now() - make_interval(mins => $1)
             ORDER BY tried_at NULLS FIRST, user_id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING user_id, name, linked_at, read_through",
    )
    .bind(READ_EVERY_MINUTES)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(claim_of))
}

/// What asking to read now came to.
#[derive(Debug)]
pub enum ByHand {
    Claimed(Claim),
    /// Read less than a minute ago.
    TooSoon,
    NotLinked,
}

/// Takes one account's link for a read asked for by hand.
pub async fn claim_now(conn: &mut PgConnection, user_id: i32) -> sqlx::Result<ByHand> {
    let row = sqlx::query_as::<_, ClaimRow>(
        "UPDATE listenbrainz_link SET tried_at = now()
         WHERE user_id = $1 AND (tried_at IS NULL OR tried_at < now() - make_interval(secs => $2))
         RETURNING user_id, name, linked_at, read_through",
    )
    .bind(user_id)
    .bind(f64::from(BY_HAND_EVERY_SECONDS))
    .fetch_optional(&mut *conn)
    .await?;

    if let Some(row) = row {
        return Ok(ByHand::Claimed(claim_of(row)));
    }
    let linked: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM listenbrainz_link WHERE user_id = $1)")
        .bind(user_id)
        .fetch_one(conn)
        .await?;
    Ok(if linked { ByHand::TooSoon } else { ByHand::NotLinked })
}

/// Reads a claimed link's listening and writes what is new into the ledger.
///
/// The request to ListenBrainz happens outside any transaction: a database
/// transaction held open across a network round trip is how one slow service
/// becomes a pool with nothing left in it.
pub async fn read(conn: &mut PgConnection, client: &Client, claim: &Claim) -> Result<Read, ReadError> {
    let from = claim.from();
    let fetched = {
        let client = client.clone();
        let name = claim.name.clone();
        tokio::task::spawn_blocking(move || client.listens(&name, from))
            .await
            .unwrap_or(Err(Failure::Unreachable))
    };

    match fetched {
        Ok(fetched) => {
            let stopped = fetched.stopped;
            let read = apply(&mut *conn, claim, fetched, OffsetDateTime::now_utc()).await?;
            // A read that got part of the way keeps what it got, and the link
            // still says why it did not get further.
            if let Some(failure) = stopped {
                mark(conn, claim, failure).await?;
            }
            Ok(read)
        }
        Err(failure) => {
            mark(conn, claim, failure).await?;
            Err(ReadError::ListenBrainz(failure))
        }
    }
}

/// Leaves why a read failed on its link.
///
/// Only against the same account: a link replaced while this read was out is
/// not this read's to mark.
async fn mark(conn: &mut PgConnection, claim: &Claim, failure: Failure) -> sqlx::Result<()> {
    sqlx::query("UPDATE listenbrainz_link SET failure = $3 WHERE user_id = $1 AND name = $2")
        .bind(claim.user_id)
        .bind(&claim.name)
        .bind(failure.code())
        .execute(conn)
        .await?;
    Ok(())
}

/// Writes what a read brought back, once.
///
/// Under a lock on the link, so a read in the background and a read by hand
/// cannot both decide the same listen is new. The listens already in the
/// ledger are subtracted before anything is priced, and the key would refuse
/// them anyway.
pub async fn apply(conn: &mut PgConnection, claim: &Claim, fetched: Fetched, now: OffsetDateTime) -> sqlx::Result<Read> {
    let mut tx = conn.begin().await?;

    // The link may have been dropped, or pointed at another account, while
    // the read was out. Either way these listens are not its listens.
    let current: Option<String> = sqlx::query_scalar("SELECT name FROM listenbrainz_link WHERE user_id = $1 FOR UPDATE")
        .bind(claim.user_id)
        .fetch_optional(&mut *tx)
        .await?;
    if current.as_deref() != Some(claim.name.as_str()) {
        tx.rollback().await?;
        return Ok(Read::default());
    }

    let mut listens: Vec<Listen> = fetched
        .listens
        .into_iter()
        .filter(|listen| listen.listened_at >= claim.linked_at)
        // Not yet matched and still young: left for a later read, by which
        // time ListenBrainz will usually have said whose it is.
        .filter(|listen| !listen.artists.is_empty() || now - listen.listened_at >= MAPPING_GRACE)
        .collect();
    listens.sort_by_key(|listen| (listen.listened_at, listen.recording_msid));

    if let (Some(first), Some(last)) = (listens.first(), listens.last()) {
        let known: Vec<(OffsetDateTime, Uuid)> = sqlx::query_as(
            "SELECT listened_at, recording_msid FROM listen
             WHERE user_id = $1 AND listened_at BETWEEN $2 AND $3",
        )
        .bind(claim.user_id)
        .bind(first.listened_at)
        .bind(last.listened_at)
        .fetch_all(&mut *tx)
        .await?;
        let known: HashSet<(OffsetDateTime, Uuid)> = known.into_iter().collect();
        listens.retain(|listen| !known.contains(&(listen.listened_at, listen.recording_msid)));
    }

    let artists: Vec<Uuid> = listens
        .iter()
        .flat_map(|listen| listen.artists.iter().copied())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let heard: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT artist, count(*) FROM listen, unnest(artist_mbids) AS artist
         WHERE user_id = $1 AND artist = ANY($2)
         GROUP BY artist",
    )
    .bind(claim.user_id)
    .bind(&artists)
    .fetch_all(&mut *tx)
    .await?;
    let heard: HashMap<Uuid, i64> = heard.into_iter().collect();

    let paid = light::price(&listens, &heard);
    let mut read = Read {
        opened: artists.iter().filter(|artist| !heard.contains_key(artist)).count(),
        ..Read::default()
    };

    for (listen, light) in listens.iter().zip(&paid) {
        let written = sqlx::query(
            "INSERT INTO listen (user_id, listened_at, recording_msid, artist_mbids, seconds, light)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(claim.user_id)
        .bind(listen.listened_at)
        .bind(listen.recording_msid)
        .bind(&listen.artists)
        .bind(listen.seconds)
        .bind(light)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() == 1 {
            read.listens += 1;
            read.light += i64::from(*light);
        }
    }

    sqlx::query("UPDATE listenbrainz_link SET read_through = $2, read_at = now(), failure = NULL WHERE user_id = $1")
        .bind(claim.user_id)
        .bind(fetched.through)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(read)
}

/// Reads every linked account in turn, forever.
///
/// One at a time, deliberately: ListenBrainz allows a handful of requests a
/// second from one address, and a read that respects that has no reason to be
/// fast. When nothing is due it sleeps a minute and looks again.
pub async fn run(pool: PgPool, client: Client) {
    loop {
        loop {
            let mut conn = match pool.acquire().await {
                Ok(conn) => conn,
                Err(error) => {
                    tracing::warn!(%error, "listening reader could not reach the database");
                    break;
                }
            };
            let claim = match claim_due(&mut conn).await {
                Ok(Some(claim)) => claim,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(%error, "listening reader could not take a link");
                    break;
                }
            };
            match read(&mut conn, &client, &claim).await {
                Ok(read) if read.listens > 0 => {
                    tracing::info!(
                        user = claim.user_id,
                        listens = read.listens,
                        light = read.light,
                        opened = read.opened,
                        "listening read"
                    );
                }
                Ok(_) => {}
                Err(ReadError::ListenBrainz(failure)) => {
                    tracing::info!(user = claim.user_id, failure = failure.code(), "listening not read");
                }
                Err(ReadError::Database(error)) => {
                    tracing::warn!(user = claim.user_id, %error, "listening could not be written");
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests;
