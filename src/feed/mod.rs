//! Living data: what ListenBrainz publishes every day, read on a schedule.
//!
//! Two feeds. **Listening**: every day ListenBrainz publishes the day's
//! listens of everyone who uses it, as a CC0 dump; lyrid counts how many
//! people listened to each star that day, and from that which stars are
//! rising. **Releases**: ListenBrainz's list of what came out around today,
//! read once a day, kept as the dates stars release on.
//!
//! The canon is rebuilt every few stages and these are not: they are the
//! weather over a sky that holds still. Nothing here is on the path of drawing
//! the sky or opening a card -- the server reads both in the background, every
//! half hour it looks, and a feed that fails says so in its record and is
//! tried again on the next look. See ADR 0022.
//!
//! Each pass over something upstream is a row of `feed_run`: the admin page
//! reads that as the health of the imports, and a dump's successful row is
//! what says it has been counted.

pub mod listening;
pub mod releases;
pub mod remote;
pub mod spark;

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::{Connection, PgConnection, PgPool};

pub use remote::Dumps;

/// How often the server looks for something new. ListenBrainz writes one
/// dump a day, a little after midnight UTC; looking every half hour costs a
/// request and finds the new day within half an hour of it appearing.
const EVERY: Duration = Duration::from_mins(30);

/// The first key of the feeds' advisory locks: a number of their own, so the
/// lock cannot be mistaken for any other the database is asked to hold.
const FEED_LOCK: i32 = 0x1d1a;

/// What the feeds read from.
#[derive(Clone)]
pub struct Sources {
    pub dumps: Dumps,
    pub listenbrainz: crate::scrobbling::Client,
}

/// Reads both feeds for as long as the server runs.
pub async fn run(pool: PgPool, sources: Sources) {
    // A minute's grace: a server that has just started has a sky to warm
    // before it has a day of listening to count.
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        pass(&pool, &sources).await;
        tokio::time::sleep(EVERY).await;
    }
}

/// One look at both feeds. Failures are logged and recorded, never raised:
/// the next look is the retry.
pub async fn pass(pool: &PgPool, sources: &Sources) {
    match listening::catch_up(pool, &sources.dumps).await {
        Ok(Some(caught)) if caught.dumps > 0 => tracing::info!(
            dumps = caught.dumps,
            listens = caught.listens,
            kept = caught.kept,
            closed = caught.closed,
            "listening counted"
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!(error = format!("{error:#}"), "the listening feed stopped"),
    }
    match releases::read_if_due(pool, &sources.listenbrainz).await {
        Ok(releases::Outcome::Read(read)) => tracing::info!(listed = read.listed, kept = read.kept, "releases read"),
        Ok(_) => {}
        Err(error) => tracing::warn!(error = format!("{error:#}"), "the releases feed stopped"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    Listening,
    Releases,
}

impl Feed {
    /// The name in `feed_run.feed`, whose check constraint names the same two.
    pub fn name(self) -> &'static str {
        match self {
            Self::Listening => "listening",
            Self::Releases => "releases",
        }
    }

    fn lock(self) -> i32 {
        match self {
            Self::Listening => 1,
            Self::Releases => 2,
        }
    }
}

/// A connection holding the feed's lock, or `None` when someone else holds it.
///
/// The connection is taken out of the pool and closed when the pass is over:
/// a session lock lives as long as its connection, and a pooled connection
/// would carry the lock on into whatever borrowed it next. Closing it -- or
/// the process dying -- is what lets go.
///
/// Holding the lock means nobody else is reading this feed, so any run still
/// marked as going is one that died mid-pass, and is closed as such.
pub(crate) async fn hold(pool: &PgPool, feed: Feed) -> Result<Option<PgConnection>> {
    let mut conn = pool.acquire().await.context("the feed could not reach the database")?.detach();
    let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(FEED_LOCK)
        .bind(feed.lock())
        .fetch_one(&mut conn)
        .await
        .context("the feed could not ask for its lock")?;
    if !held {
        let _ = conn.close().await;
        return Ok(None);
    }
    sqlx::query("UPDATE feed_run SET finished_at = now(), error = 'interrupted before it finished' WHERE feed = $1 AND finished_at IS NULL")
        .bind(feed.name())
        .execute(&mut conn)
        .await
        .context("the feed could not close the runs it found interrupted")?;
    Ok(Some(conn))
}

/// Records a pass as begun, and returns its row.
pub(crate) async fn begin(conn: &mut PgConnection, feed: Feed, source: &str) -> Result<i64> {
    sqlx::query_scalar("INSERT INTO feed_run (feed, source) VALUES ($1, $2) RETURNING id")
        .bind(feed.name())
        .bind(source)
        .fetch_one(conn)
        .await
        .context("the feed could not record its run")
}

/// Records a pass as failed, with the whole chain of why.
pub(crate) async fn fail(conn: &mut PgConnection, run: i64, error: &anyhow::Error) {
    let written = sqlx::query("UPDATE feed_run SET finished_at = now(), error = $2 WHERE id = $1 AND finished_at IS NULL")
        .bind(run)
        .bind(format!("{error:#}"))
        .execute(conn)
        .await;
    if let Err(error) = written {
        tracing::warn!(%error, run, "a feed's failure could not be recorded");
    }
}

/// Runs blocking work -- HTTP, decompression -- off the async threads.
pub(crate) async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work).await.context("the feed's worker stopped")?
}

/// Closes a connection taken by `hold`, letting go of its lock.
pub(crate) async fn release(conn: PgConnection) {
    let _ = conn.close().await;
}

/// `lyrid feed ...`: one pass of one feed, by hand.
#[derive(clap::Subcommand)]
pub enum Command {
    /// Count every ListenBrainz daily dump not yet counted.
    Listening,
    /// Read ListenBrainz's list of fresh releases, unless it was read today.
    Releases,
}

pub async fn command(pool: &PgPool, config: &crate::config::Config, command: &Command) -> Result<()> {
    match command {
        Command::Listening => {
            if let Some(caught) = listening::catch_up(pool, &Dumps::new(&config.dumps_url)).await? {
                tracing::info!(
                    dumps = caught.dumps,
                    listens = caught.listens,
                    kept = caught.kept,
                    closed = caught.closed,
                    "listening counted"
                );
            } else {
                tracing::info!("another reader is counting listening right now; nothing was done");
            }
        }
        Command::Releases => {
            let client = crate::scrobbling::Client::new(&config.listenbrainz_url);
            match releases::read_if_due(pool, &client).await? {
                releases::Outcome::Read(read) => tracing::info!(listed = read.listed, kept = read.kept, "releases read"),
                releases::Outcome::Done => tracing::info!("the releases were already read today"),
                releases::Outcome::Waiting => tracing::info!("the last read failed less than an hour ago; try again after"),
                releases::Outcome::Busy => tracing::info!("another reader is reading releases right now"),
            }
        }
    }
    Ok(())
}
