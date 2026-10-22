//! Imports who listens to whom from ListenBrainz's statistics dump: how many
//! people listen to each star, and how close stars are by the people they
//! share.
//!
//! Every full export ListenBrainz publishes carries, beside the listens
//! themselves, a statistics dump (CC0). Its first file,
//! `artists_all_time.jsonl`, is one line per person: the thousand artists they
//! have played most over all time, each with a listen count and the
//! MusicBrainz id ListenBrainz matched it to.
//!
//! ```text
//! {"user_id":1,"data":[{"listen_count":4298,"artist_name":"Wax Tailor","artist_mbid":"c24b47a1-…"},…]}
//! ```
//!
//! That one file answers both questions this stage asked of the full listen
//! dumps: how many people listen to an artist, and who is listened to by the
//! same people. It answers them in people rather than plays, as the weather
//! does (ADR 0022), and it is the front of the archive: the dump is one zstd
//! stream of 23 GB, and this file is its first 4 GB unpacked, so the import
//! reads the stream until the file ends and drops the connection there --
//! about 730 MB, three minutes, nothing on disk. See ADR 0024.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream file and field names throughout")]

mod cosine;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::feed::Dumps;
use crate::feed::remote::Dump;

/// What `dump_import` calls this source.
const SOURCE: &str = "listenbrainz-statistics";

/// The metric these edges are stored under. Re-importing a newer dump
/// replaces its edges; the vintage is in the description and the import.
pub const METRIC_KEY: &str = "listenbrainz-listeners";

/// The file of the statistics dump that is read, by the end of its path.
const MEMBER: &str = "lbdump/statistics/artists_all_time.jsonl";

/// How many rows go to Postgres at once.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// The full export to read, by its directory name on the dump server
    /// (`listenbrainz-dump-2723-20261015-000002-full`). Defaults to the
    /// newest the server offers.
    #[arg(long, value_name = "NAME")]
    pub dump: Option<String>,
}

pub async fn run(pool: &PgPool, dumps: &Dumps, args: &Args) -> Result<()> {
    // One transaction: the listeners and the edges are one reading of one
    // dump, and an interrupted run leaves the previous one standing.
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;
    import(&mut tx, dumps, args.dump.as_deref()).await?;
    tx.commit().await.context("failed to commit the import")?;
    Ok(())
}

/// The import itself, on a connection whose transaction the caller owns.
async fn import(conn: &mut PgConnection, dumps: &Dumps, wanted: Option<&str>) -> Result<()> {
    let canon = canon(conn).await?;
    if canon.is_empty() {
        bail!("the canon is empty: run `lyrid import musicbrainz` first");
    }

    let dumps = dumps.clone();
    let wanted = wanted.map(str::to_string);
    let (dump, mut audience) = tokio::task::spawn_blocking(move || -> Result<(Dump, Audience)> {
        let dump = choose(&dumps, wanted.as_deref())?;
        tracing::info!(dump = %dump.name, "reading who listens to whom");
        let audience = read(dumps.statistics(&dump)?, &canon).with_context(|| format!("cannot read the statistics of {}", dump.name))?;
        Ok((dump, audience))
    })
    .await
    .context("the dump reader panicked")??;

    let stats = &audience.stats;
    tracing::info!(
        people = stats.people,
        artists = audience.artists.len(),
        unmatched = stats.unmatched,
        unknown = stats.unknown,
        unreadable = stats.unreadable,
        "people read; working out who is listened to alongside whom"
    );

    // The people go to the workers and are dropped with them: what is written
    // is the listeners and the edges.
    let people = std::mem::take(&mut audience.people);
    let artists = audience.artists.len();
    let edges = tokio::task::spawn_blocking(move || cosine::neighbours(&people, artists, cosine::PARAMS))
        .await
        .context("the similarity workers panicked")?;
    tracing::info!(edges = edges.len(), "similarity worked out; writing to PostgreSQL");

    write(conn, &dump, &audience, &edges).await
}

/// Every MusicBrainz id the canon answers to, and the star it is: an artist's
/// own id, and the ids MusicBrainz merged into it -- ListenBrainz may match a
/// listen to either.
async fn canon(conn: &mut PgConnection) -> Result<HashMap<Uuid, i32>> {
    let rows: Vec<(Uuid, i32)> = sqlx::query_as("SELECT mbid, id FROM artist UNION ALL SELECT mbid, artist_id FROM artist_redirect")
        .fetch_all(&mut *conn)
        .await
        .context("failed to read the canon's artists")?;
    Ok(rows.into_iter().collect())
}

/// The export to read: the one named, or the newest.
fn choose(dumps: &Dumps, wanted: Option<&str>) -> Result<Dump> {
    let offered = dumps.full()?;
    match wanted {
        Some(name) => offered.iter().find(|dump| dump.name == name).cloned().with_context(|| {
            format!(
                "the dump server offers no full export named {name}; it offers {}",
                offered.iter().map(|dump| dump.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        }),
        None => offered.last().cloned().context("the dump server offers no full export"),
    }
}

/// One line of `artists_all_time.jsonl`, as far as it is read.
#[derive(Deserialize)]
struct Person {
    data: Vec<Played>,
}

#[derive(Deserialize)]
struct Played {
    listen_count: u64,
    /// `null` when ListenBrainz could not match the name to an artist.
    artist_mbid: Option<Uuid>,
}

/// What the dump says, resolved against the canon.
#[derive(Debug, Default)]
struct Audience {
    /// The canon's id of each artist, by the dense id the rest uses.
    artists: Vec<i32>,
    /// How many people listen to each artist, and their listens summed.
    listeners: Vec<u32>,
    listens: Vec<u64>,
    /// Each person's artists, as dense ids, each once.
    people: Vec<Vec<u32>>,
    stats: Stats,
}

/// What was read and what of it could not be used, so the import reports its
/// own coverage rather than leaving the gaps invisible.
#[derive(Debug, Default, PartialEq, Eq)]
struct Stats {
    people: u64,
    /// Artists in someone's list that ListenBrainz had not matched to an id.
    unmatched: u64,
    /// Artists matched to an id this canon does not hold.
    unknown: u64,
    /// Lines that are not a person.
    unreadable: u64,
}

/// Reads the statistics dump from its first byte to the end of the file of
/// people, and no further.
fn read(stream: impl Read, canon: &HashMap<Uuid, i32>) -> Result<Audience> {
    let decoder = zstd::stream::read::Decoder::new(stream).context("the statistics dump is not a zstd stream")?;
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().context("the statistics dump is not a tar")? {
        let entry = entry.context("a file of the statistics dump is unreadable")?;
        let path = entry
            .path()
            .context("a file of the statistics dump has no name")?
            .to_string_lossy()
            .into_owned();
        if path.ends_with(MEMBER) {
            // Returning here drops the archive, the decoder and the stream:
            // the 19 GB after this file are never downloaded.
            return people(BufReader::with_capacity(1 << 20, entry), canon);
        }
    }
    bail!("the statistics dump holds no {MEMBER}: ListenBrainz changed the dump")
}

/// Reads the file of people, one line each.
fn people(lines: impl BufRead, canon: &HashMap<Uuid, i32>) -> Result<Audience> {
    let mut audience = Audience::default();
    let mut dense: HashMap<i32, u32> = HashMap::new();
    let mut played: Vec<(u32, u64)> = Vec::new();
    for line in lines.lines() {
        let line = line.context("the file of people is unreadable")?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(person) = serde_json::from_str::<Person>(&line) else {
            audience.stats.unreadable += 1;
            continue;
        };
        audience.stats.people += 1;

        played.clear();
        for artist in person.data {
            // A count of nothing is not a listener.
            if artist.listen_count == 0 {
                continue;
            }
            let Some(mbid) = artist.artist_mbid else {
                audience.stats.unmatched += 1;
                continue;
            };
            let Some(&star) = canon.get(&mbid) else {
                audience.stats.unknown += 1;
                continue;
            };
            let next = u32::try_from(audience.artists.len()).context("more artists than u32 can count")?;
            let id = *dense.entry(star).or_insert_with(|| {
                audience.artists.push(star);
                audience.listeners.push(0);
                audience.listens.push(0);
                next
            });
            played.push((id, artist.listen_count));
        }

        // Two ids MusicBrainz has since merged are one star, and one listener
        // of it: the person's listens of both add up.
        played.sort_unstable_by_key(|&(id, _)| id);
        let mut artists = Vec::with_capacity(played.len());
        for group in played.chunk_by(|a, b| a.0 == b.0) {
            let id = group[0].0;
            audience.listeners[id as usize] += 1;
            audience.listens[id as usize] += group.iter().map(|&(_, listens)| listens).sum::<u64>();
            artists.push(id);
        }
        if !artists.is_empty() {
            audience.people.push(artists);
        }

        if audience.stats.people.is_multiple_of(20_000) {
            tracing::info!(people = audience.stats.people, artists = audience.artists.len(), "reading people");
        }
    }
    Ok(audience)
}

/// Writes the listeners and the edges, replacing the previous import of both.
async fn write(conn: &mut PgConnection, dump: &Dump, audience: &Audience, edges: &[cosine::Edge]) -> Result<()> {
    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ($1, $2)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(SOURCE)
    .bind(&dump.name)
    .fetch_one(&mut *conn)
    .await
    .context("failed to record the import")?;

    let params = cosine::PARAMS;
    let description = format!(
        "Artist similarity from ListenBrainz listeners ({}, CC0): the cosine of two artists' listeners, \
         a listener being a person with the artist among the thousand they have played most. \
         Artists with at least {} listeners, pairs with at least {} in common, {} neighbours kept per artist. \
         Scores run 0.0-1.0 on this metric's own scale.",
        dump.name, params.min_listeners, params.min_shared, params.neighbours
    );
    let metric_id: i16 = sqlx::query_scalar(
        "INSERT INTO similarity_metric (key, description) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET description = EXCLUDED.description
         RETURNING id",
    )
    .bind(METRIC_KEY)
    .bind(&description)
    .fetch_one(&mut *conn)
    .await
    .context("failed to record the similarity metric")?;

    // Replacing this metric leaves the others standing.
    sqlx::query("DELETE FROM artist_similarity WHERE metric_id = $1")
        .bind(metric_id)
        .execute(&mut *conn)
        .await
        .context("failed to clear the previous edges")?;

    for chunk in edges.chunks(BATCH) {
        // Dense ids are in the order artists were met, not the canon's: the
        // pair is put in the canon's order here, as the table requires.
        let (mut sources, mut targets, mut scores) = (
            Vec::with_capacity(chunk.len()),
            Vec::with_capacity(chunk.len()),
            Vec::with_capacity(chunk.len()),
        );
        for &(a, b, score) in chunk {
            let (a, b) = (audience.artists[a as usize], audience.artists[b as usize]);
            sources.push(a.min(b));
            targets.push(a.max(b));
            scores.push(score);
        }
        sqlx::query(
            "INSERT INTO artist_similarity (metric_id, source_id, target_id, score)
             SELECT $1, * FROM UNNEST($2::int[], $3::int[], $4::real[])",
        )
        .bind(metric_id)
        .bind(&sources)
        .bind(&targets)
        .bind(&scores)
        .execute(&mut *conn)
        .await
        .context("failed to write similarity edges")?;
    }
    tracing::info!(rows = edges.len(), "similarity edges written");

    sqlx::query("DELETE FROM artist_listeners")
        .execute(&mut *conn)
        .await
        .context("failed to clear the previous listeners")?;
    let ids: Vec<usize> = (0..audience.artists.len()).collect();
    for chunk in ids.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|&i| audience.artists[i]).collect();
        let listeners: Vec<i32> = chunk.iter().map(|&i| i32::try_from(audience.listeners[i]).unwrap_or(i32::MAX)).collect();
        let listens: Vec<i64> = chunk.iter().map(|&i| i64::try_from(audience.listens[i]).unwrap_or(i64::MAX)).collect();
        sqlx::query(
            "INSERT INTO artist_listeners (artist_id, listeners, listens)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::bigint[])",
        )
        .bind(&artists)
        .bind(&listeners)
        .bind(&listens)
        .execute(&mut *conn)
        .await
        .context("failed to write the listeners")?;
    }
    tracing::info!(rows = audience.artists.len(), "listeners written");

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(i64::try_from(edges.len()).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await
        .context("failed to close the import record")?;
    tracing::info!(dump = %dump.name, edges = edges.len(), listeners = audience.artists.len(), "listeners import complete");
    Ok(())
}
