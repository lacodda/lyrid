//! Real listening, read from ListenBrainz, turned into openings and light.
//!
//! A person links their ListenBrainz account once, proving it is theirs with a
//! token that is checked and dropped. lyrid then reads their whole history --
//! from the first listen ListenBrainz holds -- and after that what they listen
//! to, in the background every quarter of an hour or at once when they ask,
//! writing each listen into a ledger with the light it paid. Openings are not
//! stored at all: a star is open once a listen names it, and the ledger
//! already says so. See ADR 0019, and ADR 0020 for the history.
//!
//! Nothing here sits between a visitor and the sky. The card and the tiles
//! never wait for ListenBrainz; a read that fails leaves its reason on the
//! link and is tried again later.

pub mod client;
pub mod home;
pub mod lastfm;
pub mod light;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use sqlx::{Connection, PgConnection, PgPool, Postgres, QueryBuilder};
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

/// Rows per INSERT. Seven parameters each, well inside Postgres's limit of
/// 65,535 per statement, and few enough statements that a page of a thousand
/// listens is one round trip.
pub(crate) const INSERT_CHUNK: usize = 1000;

/// The first key of the ledger's advisory lock: a number of its own, so the
/// lock cannot be mistaken for any other the database is asked to hold.
const LEDGER_LOCK: i32 = 0x1d19;

/// A link taken for reading: whose it is and where reading stands.
#[derive(Debug, Clone)]
pub struct Claim {
    pub user_id: i32,
    pub name: String,
    pub linked_at: OffsetDateTime,
    pub read_through: Option<OffsetDateTime>,
    /// Where the person's listening on ListenBrainz begins, once the first
    /// read has asked.
    pub history_from: Option<OffsetDateTime>,
}

impl Claim {
    /// Where this read starts: two days behind the last one, and never before
    /// the person's history begins. The first read starts at the beginning of
    /// that history, so the sky lights up with everything they have heard and
    /// not only with what comes after the link.
    pub fn from(&self) -> OffsetDateTime {
        let start = self.history_from.unwrap_or(self.linked_at);
        match self.read_through {
            Some(through) => (through - OVERLAP).max(start),
            None => start,
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

type ClaimRow = (i32, String, OffsetDateTime, Option<OffsetDateTime>, Option<OffsetDateTime>);

fn claim_of((user_id, name, linked_at, read_through, history_from): ClaimRow) -> Claim {
    Claim {
        user_id,
        name,
        linked_at,
        read_through,
        history_from,
    }
}

/// Takes the link that has waited longest for its read, if any is due.
///
/// A link is due a quarter of an hour after its last read began -- or at once
/// while it is still reading its history and its last read finished well, so
/// twenty years are read back to back rather than fifty requests every
/// fifteen minutes. "Finished well" is `read_at` having passed
/// `tried_at`: a read still out has not, and neither has one that failed,
/// which waits its quarter hour like any other.
///
/// `SKIP LOCKED` and the stamp in one statement: two servers, or a server and
/// a button, can never take the same link at once, and a link taken is marked
/// as tried before anything slow happens.
pub async fn claim_due(conn: &mut PgConnection) -> sqlx::Result<Option<Claim>> {
    let row = sqlx::query_as::<_, ClaimRow>(
        "UPDATE listenbrainz_link SET tried_at = now()
         WHERE user_id = (
             SELECT user_id FROM listenbrainz_link
             WHERE tried_at IS NULL
                OR tried_at < now() - make_interval(mins => $1)
                OR (failure IS NULL
                    AND (read_through IS NULL OR read_through < linked_at)
                    AND read_at > tried_at)
             ORDER BY tried_at NULLS FIRST, user_id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING user_id, name, linked_at, read_through, history_from",
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
    /// Still reading the history from before the link, as fast as
    /// ListenBrainz allows: a read by hand would only be a second reader of
    /// the same history.
    ReadingHistory,
    NotLinked,
}

/// Takes one account's link for a read asked for by hand.
pub async fn claim_now(conn: &mut PgConnection, user_id: i32) -> sqlx::Result<ByHand> {
    let mut tx = conn.begin().await?;
    let state = sqlx::query_as::<_, (bool, bool)>(
        "SELECT read_through IS NULL OR read_through < linked_at,
                tried_at IS NULL OR tried_at < now() - make_interval(secs => $2)
         FROM listenbrainz_link WHERE user_id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .bind(f64::from(BY_HAND_EVERY_SECONDS))
    .fetch_optional(&mut *tx)
    .await?;

    let outcome = match state {
        None => ByHand::NotLinked,
        Some((true, _)) => ByHand::ReadingHistory,
        Some((false, false)) => ByHand::TooSoon,
        Some((false, true)) => {
            let row = sqlx::query_as::<_, ClaimRow>(
                "UPDATE listenbrainz_link SET tried_at = now() WHERE user_id = $1
                 RETURNING user_id, name, linked_at, read_through, history_from",
            )
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
            ByHand::Claimed(claim_of(row))
        }
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Reads a claimed link's listening and writes what is new into the ledger.
///
/// The requests to ListenBrainz happen outside any transaction: a database
/// transaction held open across a network round trip is how one slow service
/// becomes a pool with nothing left in it.
pub async fn read(conn: &mut PgConnection, client: &Client, claim: &Claim) -> Result<Read, ReadError> {
    let mut claim = claim.clone();
    if claim.history_from.is_none() {
        let started = {
            let client = client.clone();
            let name = claim.name.clone();
            tokio::task::spawn_blocking(move || client.history_start(&name))
                .await
                .unwrap_or(Err(Failure::Unreachable))
        };
        match started {
            Ok(started) => {
                // Someone who has heard nothing yet has nothing before the
                // link: their history begins where the link does.
                let from = started.map_or(claim.linked_at, |started| started.min(claim.linked_at));
                remember_start(conn, &claim, from).await?;
                claim.history_from = Some(from);
            }
            Err(failure) => {
                mark(conn, &claim, failure).await?;
                return Err(ReadError::ListenBrainz(failure));
            }
        }
    }

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
            let read = apply(&mut *conn, &claim, fetched, OffsetDateTime::now_utc()).await?;
            // A read that got part of the way keeps what it got, and the link
            // still says why it did not get further.
            if let Some(failure) = stopped {
                mark(conn, &claim, failure).await?;
            }
            Ok(read)
        }
        Err(failure) => {
            mark(conn, &claim, failure).await?;
            Err(ReadError::ListenBrainz(failure))
        }
    }
}

/// Leaves where the person's history begins on their link, once.
async fn remember_start(conn: &mut PgConnection, claim: &Claim, from: OffsetDateTime) -> sqlx::Result<()> {
    sqlx::query("UPDATE listenbrainz_link SET history_from = $3 WHERE user_id = $1 AND name = $2 AND history_from IS NULL")
        .bind(claim.user_id)
        .bind(&claim.name)
        .bind(from)
        .execute(conn)
        .await?;
    Ok(())
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

/// Holds one account's ledger for the rest of the transaction.
///
/// Every writer of the ledger takes it -- a read of ListenBrainz in the
/// background, a read by hand, a Last.fm file -- so no two of them can decide
/// at once that the same moment holds no listen yet. An advisory lock rather
/// than a row's: the ledger belongs to the account, and an account need not
/// be linked to receive a file.
pub(crate) async fn hold_ledger(conn: &mut PgConnection, user_id: i32) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(LEDGER_LOCK)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// A row of the ledger as pricing sees it: when, which recording, by whom,
/// how long, and from where.
type Row = (OffsetDateTime, Uuid, Vec<Uuid>, Option<i32>, String);

/// Writes what a read brought back, once.
///
/// Under the ledger's lock, so a read in the background and a read by hand
/// cannot both decide the same listen is new. The price is worked out over the
/// listening in the order it was heard: the listens the ledger already holds
/// within the same stretch are walked through with the new ones -- counted,
/// not paid again -- so a listen read late is priced as the listen it was at
/// its own moment.
pub async fn apply(conn: &mut PgConnection, claim: &Claim, fetched: Fetched, now: OffsetDateTime) -> sqlx::Result<Read> {
    let mut tx = conn.begin().await?;
    hold_ledger(&mut tx, claim.user_id).await?;

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
        // Not yet matched and still young: left for a later read, by which
        // time ListenBrainz will usually have said whose it is.
        .filter(|listen| !listen.artists.is_empty() || now - listen.listened_at >= MAPPING_GRACE)
        .collect();
    listens.sort_by_key(|listen| (listen.listened_at, listen.recording_msid));

    let mut read = Read::default();
    if let (Some(first), Some(last)) = (listens.first(), listens.last()) {
        let (first, last) = (first.listened_at, last.listened_at);
        let known: Vec<Row> = sqlx::query_as(
            "SELECT listened_at, recording, artist_mbids, seconds, source FROM listen
             WHERE user_id = $1 AND listened_at BETWEEN $2 AND $3",
        )
        .bind(claim.user_id)
        .bind(first)
        .bind(last)
        .fetch_all(&mut *tx)
        .await?;

        let keys: HashSet<(OffsetDateTime, Uuid)> = known.iter().map(|row| (row.0, row.1)).collect();
        listens.retain(|listen| !keys.contains(&(listen.listened_at, listen.recording_msid)));

        // A listen from a file at the very second ListenBrainz reports one is
        // the same listen, told twice -- someone who scrobbles to both, or a
        // history ListenBrainz imported from Last.fm itself. ListenBrainz's
        // telling knows the artists by id, so it replaces the file's.
        let moments: HashSet<OffsetDateTime> = listens.iter().map(|listen| listen.listened_at).collect();
        let (told_twice, known): (Vec<Row>, Vec<Row>) = known.into_iter().partition(|row| row.4 == "lastfm" && moments.contains(&row.0));

        read = write_listens(&mut tx, claim, &listens, &known, &told_twice, first).await?;
    }

    sqlx::query("UPDATE listenbrainz_link SET read_through = $2, read_at = now(), failure = NULL WHERE user_id = $1")
        .bind(claim.user_id)
        .bind(fetched.through)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(read)
}

/// Prices the new listens against everything heard before them and writes
/// them, replacing a file's copy of any moment ListenBrainz also tells.
async fn write_listens(
    tx: &mut PgConnection,
    claim: &Claim,
    listens: &[Listen],
    known: &[Row],
    told_twice: &[Row],
    first: OffsetDateTime,
) -> sqlx::Result<Read> {
    if listens.is_empty() {
        return Ok(Read::default());
    }

    let artists: Vec<Uuid> = listens
        .iter()
        .flat_map(|listen| listen.artists.iter().copied())
        .chain(known.iter().flat_map(|row| row.2.iter().copied()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    // For each artist: how often heard before this stretch, which is where
    // pricing starts counting, and whether heard at all, which is what makes
    // an artist in this read an opening or not.
    let counts: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "SELECT artist, count(*) FILTER (WHERE listened_at < $3), count(*)
         FROM listen, unnest(artist_mbids) AS artist
         WHERE user_id = $1 AND artist = ANY($2)
         GROUP BY artist",
    )
    .bind(claim.user_id)
    .bind(&artists)
    .bind(first)
    .fetch_all(&mut *tx)
    .await?;
    let before: HashMap<Uuid, i64> = counts.iter().map(|(artist, before, _)| (*artist, *before)).collect();
    let ever: HashSet<Uuid> = counts.iter().filter(|(_, _, ever)| *ever > 0).map(|(artist, _, _)| *artist).collect();

    // The stretch in the order it was heard: what the ledger holds, and the
    // new listens among it. Only a new listen heard while linked is paid.
    let mut timeline: Vec<(OffsetDateTime, Uuid, light::Heard<'_>, Option<usize>)> = known
        .iter()
        .map(|row| {
            let heard = light::Heard {
                artists: &row.2,
                seconds: row.3,
                pays: false,
            };
            (row.0, row.1, heard, None)
        })
        .chain(listens.iter().enumerate().map(|(index, listen)| {
            let heard = light::Heard {
                artists: &listen.artists,
                seconds: listen.seconds,
                pays: listen.listened_at >= claim.linked_at,
            };
            (listen.listened_at, listen.recording_msid, heard, Some(index))
        }))
        .collect();
    timeline.sort_by_key(|entry| (entry.0, entry.1));
    let prices = light::price(&timeline.iter().map(|entry| entry.2).collect::<Vec<_>>(), &before);

    let mut paid: Vec<Option<i32>> = vec![None; listens.len()];
    for (entry, price) in timeline.iter().zip(prices) {
        if let Some(index) = entry.3 {
            paid[index] = price;
        }
    }

    if !told_twice.is_empty() {
        let moments: Vec<OffsetDateTime> = told_twice.iter().map(|row| row.0).collect();
        sqlx::query("DELETE FROM listen WHERE user_id = $1 AND source = 'lastfm' AND listened_at = ANY($2)")
            .bind(claim.user_id)
            .bind(&moments)
            .execute(&mut *tx)
            .await?;
    }

    let mut read = Read {
        opened: listens
            .iter()
            .flat_map(|listen| listen.artists.iter())
            .filter(|artist| !ever.contains(*artist))
            .collect::<HashSet<_>>()
            .len(),
        ..Read::default()
    };
    let rows: Vec<(&Listen, Option<i32>)> = listens.iter().zip(paid).collect();
    for chunk in rows.chunks(INSERT_CHUNK) {
        let mut insert = QueryBuilder::<Postgres>::new("INSERT INTO listen (user_id, listened_at, recording, artist_mbids, seconds, light, source) ");
        insert.push_values(chunk, |mut row, (listen, light)| {
            row.push_bind(claim.user_id)
                .push_bind(listen.listened_at)
                .push_bind(listen.recording_msid)
                .push_bind(listen.artists.clone())
                .push_bind(listen.seconds)
                .push_bind(*light)
                .push_bind("listenbrainz");
        });
        insert.push(" ON CONFLICT DO NOTHING RETURNING light");
        let written: Vec<Option<i32>> = insert.build_query_scalar().fetch_all(&mut *tx).await?;
        read.listens += written.len();
        read.light += written.iter().flatten().map(|light| i64::from(*light)).sum::<i64>();
    }
    Ok(read)
}

/// A star of the current sky that a person's listening has named.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// How many listens named it.
    pub listens: i64,
    /// The first of them: when the star opened.
    pub first_at: OffsetDateTime,
}

impl Placed {
    pub fn heard(&self) -> home::Heard {
        home::Heard {
            id: self.id,
            x: self.x,
            y: self.y,
            listens: self.listens,
        }
    }
}

/// Every star of the current sky the ledger names, from listens before
/// `until` -- or from all of them.
///
/// Read out of the ledger rather than kept beside it: a star is open from the
/// first listen that names it, and that is a question the ledger answers. An
/// artist the canon does not place is someone heard, not a star.
pub async fn placed(conn: &mut PgConnection, user_id: i32, until: Option<OffsetDateTime>) -> sqlx::Result<Vec<Placed>> {
    let rows = sqlx::query_as::<_, (i32, String, f32, f32, i64, OffsetDateTime)>(
        "WITH heard AS (
             SELECT artist AS mbid, count(*) AS listens, min(listened_at) AS first_at
             FROM listen, unnest(artist_mbids) AS artist
             WHERE user_id = $1 AND ($2::timestamptz IS NULL OR listened_at < $2)
             GROUP BY artist
         )
         SELECT a.id, a.name, p.x, p.y, h.listens, h.first_at
         FROM heard h
         JOIN artist a ON a.mbid = h.mbid
         JOIN artist_position p ON p.artist_id = a.id
          AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         ORDER BY a.id",
    )
    .bind(user_id)
    .bind(until)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, x, y, listens, first_at)| Placed {
            id,
            name,
            x,
            y,
            listens,
            first_at,
        })
        .collect())
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
