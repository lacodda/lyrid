//! The listening feed: how many people listened to each star, day by day.
//!
//! Every day ListenBrainz publishes the listens it received that day. A dump
//! is read once, in order, and boils down to pairs -- this listener heard this
//! star on this day -- which wait in `listener_seen` until no dump still to
//! come can add to their day. Then the day is closed: each star's listeners
//! are counted into `artist_day`, the pairs are deleted, and the stars rising
//! this week are worked out again.
//!
//! Three choices carry the meaning, each measured on the dump of 2026-10-15
//! before it was made:
//!
//! - **People, not listens.** One account sent 192,000 listens of one artist
//!   that day. Counted as listens it would be the brightest star in the sky;
//!   counted as people it is one.
//! - **Live listening only.** Of 3.76 million listens in that dump, 2.35
//!   million were histories imported by 660 people, years old. A listen
//!   counts when it was played no more than a day before ListenBrainz
//!   received it -- every live listen of that dump was -- and not in the
//!   future by more than an hour, which is as far as players' clocks were off.
//! - **The day it arrived.** A day is the day ListenBrainz received a listen.
//!   The dumps are cut by arrival, so only that day's coverage can be said
//!   exactly, and ListenBrainz does skip days: a day without its dump must read
//!   as missing, not as quiet.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use sqlx::{Connection, PgConnection, PgPool};
use time::Date;
use uuid::Uuid;

use super::remote::{self, Dump, Dumps};
use super::{Feed, blocking, spark};

/// How long before ListenBrainz received a listen it may have been played, in
/// seconds, and still count as live: a day.
const LATE: i64 = 24 * 60 * 60;

/// How far ahead of its arrival a listen may claim to have been played, in
/// seconds: an hour of a player's clock being wrong.
const EARLY: i64 = 60 * 60;

/// How long a day's counts are kept: thirteen weeks. Rising is read off the
/// last five; the rest is a season to look back on, and older than that the
/// full dumps are the better source.
const KEEP_DAYS: i32 = 13 * 7;

/// The longest span a dump may have and hold no files: a minute, against the
/// fraction of a second the empty ones measured on 2026-10-16 span.
const EMPTY_SPAN_SECONDS: f64 = 60.0;

/// Pairs per INSERT: three parameters each, far inside Postgres's limit.
const CHUNK: usize = 10_000;

/// What rising means, in numbers.
mod rising {
    /// The week a star is rising in, in days, and the weeks it rises from.
    pub const RECENT_DAYS: i32 = 7;
    pub const EARLIER_DAYS: i32 = 28;
    /// How much of each window the dumps must cover before anything is called
    /// rising: three days of the week, a week of the four before it.
    pub const RECENT_COVER_SECONDS: i64 = 3 * 86_400;
    pub const EARLIER_COVER_SECONDS: i64 = 7 * 86_400;
    /// Listeners added to both sides of the ratio, so a star going from no
    /// one to two people is not a thousandfold rise.
    pub const PRIOR: f64 = 3.0;
    /// A star needs this many listeners a day this week to be rising at all.
    pub const MIN_RECENT: f64 = 5.0;
    /// And this much more than before.
    pub const MIN_RATIO: f64 = 1.5;
    /// How many are kept.
    pub const KEEP: i64 = 500;
}

/// What a pass counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Caught {
    /// Dumps counted.
    pub dumps: usize,
    /// Listens read from them.
    pub listens: u64,
    /// Pairs of a listener and a star of this canon they left.
    pub kept: u64,
    /// Days closed.
    pub closed: usize,
}

/// Counts every dump on offer that comes after the last one counted.
///
/// `None` when another reader holds the feed. Stops at the first dump that
/// fails, leaving it to the next pass: counting a later dump first would close
/// the failed one's days without it.
pub async fn catch_up(pool: &PgPool, dumps: &Dumps) -> Result<Option<Caught>> {
    let Some(mut conn) = super::hold(pool, Feed::Listening).await? else {
        return Ok(None);
    };
    let caught = catch_up_holding(&mut conn, dumps).await;
    super::release(conn).await;
    caught.map(Some)
}

async fn catch_up_holding(conn: &mut PgConnection, dumps: &Dumps) -> Result<Caught> {
    let offered = {
        let dumps = dumps.clone();
        blocking(move || dumps.incremental()).await?
    };
    let pending = pending(conn, offered).await?;

    let mut caught = Caught::default();
    for dump in pending {
        let run = super::begin(conn, Feed::Listening, &dump.name).await?;
        match count(conn, dumps, &dump, run).await {
            Ok(counted) => {
                caught.dumps += 1;
                caught.listens += counted.listens;
                caught.kept += counted.kept;
                caught.closed += counted.closed;
            }
            Err(error) => {
                super::fail(conn, run, &error).await;
                return Err(error.context(format!("{} was not counted", dump.name)));
            }
        }
    }
    Ok(caught)
}

/// The dumps after the last one counted, oldest first.
///
/// After, and not merely not yet counted: a dump older than one already
/// counted belongs to days already closed, and counting it would reopen them.
/// One the server stopped offering before it was read is simply passed over --
/// its days close as partly covered, which is what they are.
async fn pending(conn: &mut PgConnection, offered: Vec<Dump>) -> Result<Vec<Dump>> {
    let counted: Vec<String> = sqlx::query_scalar("SELECT source FROM feed_run WHERE feed = 'listening' AND finished_at IS NOT NULL AND error IS NULL")
        .fetch_all(&mut *conn)
        .await
        .context("cannot read which dumps were counted")?;
    let last = counted.iter().filter_map(|name| remote::sequence_of(name)).max();
    Ok(offered.into_iter().filter(|dump| last.is_none_or(|last| dump.sequence > last)).collect())
}

/// What one dump came to.
struct Counted {
    listens: u64,
    kept: u64,
    closed: usize,
}

/// What a dump boils down to before anything is written.
pub(crate) struct Gathered {
    /// The span the dump says it holds, as it says it.
    pub from: String,
    pub to: String,
    /// Listens in it.
    pub listens: u64,
    /// Parquet files in it. None at all is how ListenBrainz writes a dump of
    /// nothing.
    pub files: usize,
    /// (days since the epoch, star, listener), each once.
    pub pairs: Vec<(i32, Uuid, i64)>,
}

async fn count(conn: &mut PgConnection, dumps: &Dumps, dump: &Dump, run: i64) -> Result<Counted> {
    let gathered = {
        let dumps = dumps.clone();
        let dump = dump.clone();
        blocking(move || gather(&dumps, &dump)).await?
    };

    // ListenBrainz cuts a second dump a moment after the day's, spanning a
    // second or less -- sometimes ending before it starts -- and when nothing
    // arrived in it, it holds no Parquet at all. A dump of any real span with
    // nothing in it is not that, and is refused rather than counted as quiet.
    if gathered.files == 0 {
        let span: f64 = sqlx::query_scalar("SELECT extract(epoch FROM $2::timestamptz - $1::timestamptz)::float8")
            .bind(&gathered.from)
            .bind(&gathered.to)
            .fetch_one(&mut *conn)
            .await
            .context("cannot read the dump's span")?;
        if span > EMPTY_SPAN_SECONDS {
            bail!("{} spans {span:.0} seconds and holds no Parquet files", dump.name);
        }
    }

    let mut tx = conn.begin().await.context("cannot open the counting transaction")?;
    let kept = write(&mut tx, &gathered).await?;
    let closed = close(&mut tx, &gathered.to).await?;

    sqlx::query(
        "UPDATE feed_run SET finished_at = now(), span_from = $2::timestamptz, span_to = $3::timestamptz, rows_read = $4, rows_kept = $5
         WHERE id = $1",
    )
    .bind(run)
    .bind(&gathered.from)
    .bind(&gathered.to)
    .bind(i64::try_from(gathered.listens).unwrap_or(i64::MAX))
    .bind(i64::try_from(kept).unwrap_or(i64::MAX))
    .execute(&mut *tx)
    .await
    .context("cannot record the dump as counted")?;

    tx.commit().await.context("cannot commit the dump's counts")?;
    Ok(Counted {
        listens: gathered.listens,
        kept,
        closed,
    })
}

/// Reads a dump into pairs. Blocking: HTTP and decompression.
fn gather(dumps: &Dumps, dump: &Dump) -> Result<Gathered> {
    let remote = dumps.spark(dump)?;
    let entries = remote::entries(&remote)?;

    let text = |suffix: &str| -> Result<String> {
        let entry = entries
            .iter()
            .find(|entry| entry.name.ends_with(suffix))
            .with_context(|| format!("{} holds no {suffix}", remote.url()))?;
        let bytes = remote.read(entry.offset, usize::try_from(entry.size).context("a timestamp file larger than memory")?)?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    };
    let from = text("/START_TIMESTAMP")?;
    let to = text("/END_TIMESTAMP")?;

    let files: Vec<_> = entries.iter().filter(|entry| entry.name.ends_with(".parquet")).collect();
    let count = files.len();

    let mut pairs = HashSet::new();
    let mut listens = 0u64;
    for entry in files {
        listens += spark::read_listens(&remote, entry, |listen| {
            if !is_live(&listen) {
                return;
            }
            let Ok(day) = i32::try_from(listen.created.div_euclid(86_400)) else {
                return;
            };
            for artist in listen.artists {
                pairs.insert((day, artist, listen.listener));
            }
        })?;
    }

    Ok(Gathered {
        from,
        to,
        listens,
        files: count,
        pairs: pairs.into_iter().collect(),
    })
}

/// Whether a listen was played around when it arrived, rather than brought
/// in with a history.
fn is_live(listen: &spark::Listen) -> bool {
    listen.listened >= listen.created - LATE && listen.listened <= listen.created + EARLY
}

/// Stages a dump's pairs and adds its span to the days it covers. Returns how
/// many pairs were new and belong to a star of this canon.
pub(crate) async fn write(tx: &mut PgConnection, gathered: &Gathered) -> Result<u64> {
    let mut kept = 0u64;
    for chunk in gathered.pairs.chunks(CHUNK) {
        let days: Vec<i32> = chunk.iter().map(|(day, _, _)| *day).collect();
        let stars: Vec<Uuid> = chunk.iter().map(|(_, star, _)| *star).collect();
        let listeners: Vec<i64> = chunk.iter().map(|(_, _, listener)| *listener).collect();
        // Only stars this canon holds: the table answers how many people
        // listen to a star of this sky. And never into a day already closed:
        // its count is final.
        kept += sqlx::query(
            "INSERT INTO listener_seen (day, artist_mbid, listener)
             SELECT DATE '1970-01-01' + t.day, t.star, t.listener
             FROM UNNEST($1::int[], $2::uuid[], $3::bigint[]) AS t (day, star, listener)
             WHERE EXISTS (SELECT 1 FROM artist a WHERE a.mbid = t.star)
               AND NOT EXISTS (
                   SELECT 1 FROM listening_day d WHERE d.day = DATE '1970-01-01' + t.day AND d.closed_at IS NOT NULL
               )
             ON CONFLICT DO NOTHING",
        )
        .bind(&days)
        .bind(&stars)
        .bind(&listeners)
        .execute(&mut *tx)
        .await
        .context("cannot stage the dump's listeners")?
        .rows_affected();
    }

    // The span's share of each UTC day it touches. All of it in UTC wall
    // time, without a time zone: mixing the two would let the session's zone
    // decide where a day begins.
    sqlx::query(
        "WITH span AS (
             SELECT ($1::timestamptz AT TIME ZONE 'UTC') AS f, ($2::timestamptz AT TIME ZONE 'UTC') AS t
         )
         INSERT INTO listening_day (day, covered_seconds)
         SELECT d::date, greatest(0, extract(epoch FROM least(span.t, d + interval '1 day') - greatest(span.f, d)))::int
         FROM span, generate_series(date_trunc('day', span.f), date_trunc('day', span.t), interval '1 day') AS d
         ON CONFLICT (day) DO UPDATE SET covered_seconds = least(86400, listening_day.covered_seconds + EXCLUDED.covered_seconds)",
    )
    .bind(&gathered.from)
    .bind(&gathered.to)
    .execute(&mut *tx)
    .await
    .context("cannot record which days the dump covers")?;

    Ok(kept)
}

/// Closes every day before the one the dump ends in: the dumps are read in
/// order, so nothing still to come arrived on those days. Returns how many
/// days were closed.
pub(crate) async fn close(tx: &mut PgConnection, to: &str) -> Result<usize> {
    let line: Date = sqlx::query_scalar("SELECT ($1::timestamptz AT TIME ZONE 'UTC')::date")
        .bind(to)
        .fetch_one(&mut *tx)
        .await
        .context("cannot read where the dump ends")?;

    // A day no dump covered has no row yet. It gets one, empty, so that it
    // reads as missing rather than not being there at all.
    sqlx::query(
        "INSERT INTO listening_day (day, covered_seconds)
         SELECT d::date, 0
         FROM generate_series((SELECT min(day) FROM listening_day)::timestamp, ($1::date - 1)::timestamp, interval '1 day') AS d
         ON CONFLICT DO NOTHING",
    )
    .bind(line)
    .execute(&mut *tx)
    .await
    .context("cannot record the days no dump covered")?;

    sqlx::query(
        "INSERT INTO artist_day (day, artist_mbid, listeners)
         SELECT s.day, s.artist_mbid, count(*)
         FROM listener_seen s JOIN listening_day d ON d.day = s.day AND d.closed_at IS NULL
         WHERE s.day < $1
         GROUP BY s.day, s.artist_mbid
         HAVING count(*) >= 2",
    )
    .bind(line)
    .execute(&mut *tx)
    .await
    .context("cannot count the closed days' listeners")?;

    let closed = sqlx::query(
        "UPDATE listening_day d
         SET closed_at = now(),
             listeners = (SELECT count(DISTINCT s.listener) FROM listener_seen s WHERE s.day = d.day)
         WHERE d.day < $1 AND d.closed_at IS NULL",
    )
    .bind(line)
    .execute(&mut *tx)
    .await
    .context("cannot close the days")?
    .rows_affected();

    sqlx::query("DELETE FROM listener_seen WHERE day < $1")
        .bind(line)
        .execute(&mut *tx)
        .await
        .context("cannot let go of the closed days' listeners")?;

    if closed > 0 {
        sqlx::query(
            "DELETE FROM artist_day
             WHERE day <= (SELECT max(day) FROM listening_day WHERE closed_at IS NOT NULL) - $1",
        )
        .bind(KEEP_DAYS)
        .execute(&mut *tx)
        .await
        .context("cannot let go of the oldest days")?;
        rise(tx).await?;
    }

    Ok(usize::try_from(closed).unwrap_or(usize::MAX))
}

/// Works out the stars rising over the last closed week, against the four
/// weeks before it.
///
/// Each side is listeners a day: the sum over the window divided by how much
/// of the window the dumps cover, so a week with a missing day is not a week
/// everyone listened less. Nothing is called rising until both windows are
/// covered well enough to say so.
pub(crate) async fn rise(tx: &mut PgConnection) -> Result<()> {
    sqlx::query("DELETE FROM star_rising")
        .execute(&mut *tx)
        .await
        .context("cannot clear the risers")?;
    sqlx::query(
        "WITH latest AS (
             SELECT max(day) AS through FROM listening_day WHERE closed_at IS NOT NULL
         ),
         cover AS (
             SELECT l.through,
                    coalesce(sum(d.covered_seconds) FILTER (WHERE d.day > l.through - $1), 0) AS recent_seconds,
                    coalesce(sum(d.covered_seconds) FILTER (WHERE d.day <= l.through - $1), 0) AS earlier_seconds
             FROM latest l
             JOIN listening_day d ON d.closed_at IS NOT NULL AND d.day > l.through - ($1 + $2) AND d.day <= l.through
             GROUP BY l.through
         ),
         per AS (
             SELECT a.artist_mbid,
                    c.through,
                    coalesce(sum(a.listeners) FILTER (WHERE a.day > c.through - $1), 0) / (c.recent_seconds / 86400.0) AS recent,
                    coalesce(sum(a.listeners) FILTER (WHERE a.day <= c.through - $1), 0) / (c.earlier_seconds / 86400.0) AS earlier
             FROM cover c
             JOIN artist_day a ON a.day > c.through - ($1 + $2) AND a.day <= c.through
             WHERE c.recent_seconds >= $3 AND c.earlier_seconds >= $4
             GROUP BY a.artist_mbid, c.through, c.recent_seconds, c.earlier_seconds
         ),
         ranked AS (
             SELECT artist_mbid, through, recent, earlier,
                    row_number() OVER (ORDER BY (recent + $5) / (earlier + $5) DESC, recent DESC, artist_mbid) AS rank
             FROM per
             WHERE recent >= $6 AND (recent + $5) / (earlier + $5) >= $7
         )
         INSERT INTO star_rising (artist_mbid, rank, recent, earlier, through)
         SELECT artist_mbid, rank, recent, earlier, through FROM ranked WHERE rank <= $8",
    )
    .bind(rising::RECENT_DAYS)
    .bind(rising::EARLIER_DAYS)
    .bind(rising::RECENT_COVER_SECONDS)
    .bind(rising::EARLIER_COVER_SECONDS)
    .bind(rising::PRIOR)
    .bind(rising::MIN_RECENT)
    .bind(rising::MIN_RATIO)
    .bind(rising::KEEP)
    .execute(&mut *tx)
    .await
    .context("cannot work out the risers")?;
    Ok(())
}

#[cfg(test)]
mod tests;
