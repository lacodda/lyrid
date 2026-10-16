//! The releases feed: what came out around today, as ListenBrainz lists it.
//!
//! ListenBrainz assembles from MusicBrainz a list of releases out within a
//! window of today, for everyone. It is read once a UTC day, thirty days either
//! side, and kept: a release group's row says when it came out and whose it
//! is, and over the months those rows become the rhythm a star releases to.
//!
//! The list is one request for all of it -- a dataset served over HTTP, not a
//! question about an artist -- so ADR 0002 holds: nothing waits on it, and a
//! day it cannot be read is retried an hour later.

use anyhow::{Context, Result};
use sqlx::{Connection, PgConnection, PgPool};

use super::{Feed, blocking};
use crate::scrobbling::Client;
use crate::scrobbling::client::FreshRelease;

/// How far either side of today the list is asked for, in days.
const WINDOW_DAYS: u32 = 30;

/// What a look at the feed came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Read(Read),
    /// Already read today.
    Done,
    /// The last attempt failed less than an hour ago.
    Waiting,
    /// Another reader holds the feed.
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    /// Releases the list held.
    pub listed: usize,
    /// Of those, the ones lyrid could read and kept.
    pub kept: usize,
}

/// Reads the list, unless it was read today or failed within the hour.
pub async fn read_if_due(pool: &PgPool, client: &Client) -> Result<Outcome> {
    let Some(mut conn) = super::hold(pool, Feed::Releases).await? else {
        return Ok(Outcome::Busy);
    };
    let outcome = read_holding(&mut conn, client).await;
    super::release(conn).await;
    outcome
}

async fn read_holding(conn: &mut PgConnection, client: &Client) -> Result<Outcome> {
    // The day is the database's, in UTC, so two servers in two time zones
    // agree on what today is.
    let (today, done, failing): (String, bool, bool) = sqlx::query_as(
        "SELECT to_char((now() AT TIME ZONE 'UTC')::date, 'YYYY-MM-DD'),
                EXISTS (SELECT 1 FROM feed_run WHERE feed = 'releases' AND source = to_char((now() AT TIME ZONE 'UTC')::date, 'YYYY-MM-DD')
                                                 AND finished_at IS NOT NULL AND error IS NULL),
                EXISTS (SELECT 1 FROM feed_run WHERE feed = 'releases' AND error IS NOT NULL AND started_at > now() - interval '1 hour')",
    )
    .fetch_one(&mut *conn)
    .await
    .context("cannot read when the releases were last read")?;
    if done {
        return Ok(Outcome::Done);
    }
    if failing {
        return Ok(Outcome::Waiting);
    }

    let run = super::begin(conn, Feed::Releases, &today).await?;
    match read(conn, client, run).await {
        Ok(read) => Ok(Outcome::Read(read)),
        Err(error) => {
            super::fail(conn, run, &error).await;
            Err(error)
        }
    }
}

async fn read(conn: &mut PgConnection, client: &Client, run: i64) -> Result<Read> {
    let releases = {
        let client = client.clone();
        blocking(move || {
            client
                .fresh_releases(WINDOW_DAYS)
                .map_err(|failure| anyhow::anyhow!("ListenBrainz's fresh releases: {}", failure.code()))
        })
        .await?
    };
    // How many the answer held is not known once unreadable ones are dropped;
    // the count kept is what the record can stand behind.
    let listed = releases.len();

    let mut tx = conn.begin().await.context("cannot open the releases transaction")?;
    let kept = keep(&mut tx, &releases).await?;
    sqlx::query("UPDATE feed_run SET finished_at = now(), rows_read = $2, rows_kept = $3 WHERE id = $1")
        .bind(run)
        .bind(i64::try_from(listed).unwrap_or(i64::MAX))
        .bind(i64::try_from(kept).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .context("cannot record the releases as read")?;
    tx.commit().await.context("cannot commit the releases")?;
    Ok(Read { listed, kept })
}

/// Writes the releases, each over its earlier row: the list as last read is
/// what MusicBrainz says now, dates corrected and all.
pub(crate) async fn keep(tx: &mut PgConnection, releases: &[FreshRelease]) -> Result<usize> {
    let mut kept = 0usize;
    for chunk in releases.chunks(2_000) {
        let groups: Vec<uuid::Uuid> = chunk.iter().map(|release| release.group).collect();
        let titles: Vec<&str> = chunk.iter().map(|release| release.title.as_str()).collect();
        let credits: Vec<&str> = chunk.iter().map(|release| release.credit.as_str()).collect();
        // A release credits one artist or several, and UNNEST cannot take an
        // array of arrays of different lengths: each list travels as text
        // and is split back on the other side.
        let artists: Vec<String> = chunk
            .iter()
            .map(|release| release.artists.iter().map(uuid::Uuid::to_string).collect::<Vec<_>>().join(","))
            .collect();
        let kinds: Vec<Option<&str>> = chunk.iter().map(|release| release.kind.as_deref()).collect();
        let dates: Vec<time::Date> = chunk.iter().map(|release| release.out_on).collect();

        let written = sqlx::query(
            "INSERT INTO new_release (release_group_mbid, title, credit, artist_mbids, primary_type, out_on)
             SELECT g, t, c, string_to_array(a, ',')::uuid[], k, o
             FROM UNNEST($1::uuid[], $2::text[], $3::text[], $4::text[], $5::text[], $6::date[]) AS r (g, t, c, a, k, o)
             ON CONFLICT (release_group_mbid) DO UPDATE SET
                 title = EXCLUDED.title,
                 credit = EXCLUDED.credit,
                 artist_mbids = EXCLUDED.artist_mbids,
                 primary_type = EXCLUDED.primary_type,
                 out_on = EXCLUDED.out_on,
                 seen_at = now()",
        )
        .bind(&groups)
        .bind(&titles)
        .bind(&credits)
        .bind(&artists)
        .bind(&kinds)
        .bind(&dates)
        .execute(&mut *tx)
        .await
        .context("cannot write the releases")?
        .rows_affected();
        kept += usize::try_from(written).unwrap_or(usize::MAX);
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use crate::scrobbling::client::parse_fresh_releases;

    #[test]
    fn reads_the_list_as_listenbrainz_writes_it() {
        // Two releases as the list held them on 2026-10-16, a third with a
        // partial date, and a fourth with nobody credited.
        let body = r#"{"payload": {"releases": [
            {"artist_credit_name": "Ai Hasegawa feat. Kim", "artist_mbids": ["aeaecb45-db50-4ee7-9aff-d502ccb0d39c", "bddd7cc1-357e-41a1-a58f-9aee1511c7f5"],
             "caa_id": null, "caa_release_mbid": null, "listen_count": 0, "release_date": "2026-09-16",
             "release_group_mbid": "d5fac2e8-7153-4d80-b0af-ee12ab31aa9f", "release_group_primary_type": "Single",
             "release_mbid": "fa580f72-8b69-4e30-bc57-8e89b585d007", "release_name": "Long Island", "release_tags": ["pop"]},
            {"artist_credit_name": "Somebody", "artist_mbids": ["bddd7cc1-357e-41a1-a58f-9aee1511c7f5"], "release_date": "2026-10-23",
             "release_group_mbid": "a8f26aa8-fe71-4677-bed9-2a9b0b66caae", "release_group_primary_type": null, "release_name": "Later"},
            {"artist_credit_name": "Partial", "artist_mbids": ["bddd7cc1-357e-41a1-a58f-9aee1511c7f5"], "release_date": "2026-10",
             "release_group_mbid": "11111111-fe71-4677-bed9-2a9b0b66caae", "release_name": "No Day"},
            {"artist_credit_name": "Nobody", "artist_mbids": [], "release_date": "2026-10-01",
             "release_group_mbid": "22222222-fe71-4677-bed9-2a9b0b66caae", "release_name": "Unsung"}
        ], "total_count": 4}}"#;

        let releases = parse_fresh_releases(body).expect("the list reads");
        assert_eq!(releases.len(), 2, "a partial date and an uncredited release are left out");
        assert_eq!(releases[0].title, "Long Island");
        assert_eq!(releases[0].artists.len(), 2);
        assert_eq!(releases[0].kind.as_deref(), Some("Single"));
        assert_eq!(releases[0].out_on.to_string(), "2026-09-16");
        assert_eq!(releases[1].kind, None);
    }

    #[test]
    fn an_answer_that_is_not_the_list_is_malformed() {
        assert!(parse_fresh_releases(r#"{"error": "nope"}"#).is_err());
    }
}
