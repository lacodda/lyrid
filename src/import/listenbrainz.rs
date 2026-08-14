//! Imports the artist similarity graph from ListenBrainz's published
//! relations dataset.
//!
//! The dataset is a tar.bz2 of JSON lines, one edge per line, keyed by
//! MusicBrainz artist_credit ids:
//!
//! ```text
//! {"id_0":1021,"name_0":"Ludwig van Beethoven","id_1":11285,"name_1":"Wolfgang Amadeus Mozart","score":1.0}
//! ```
//!
//! Credits are resolved to artists through the mapping the MusicBrainz import
//! leaves behind, so this import requires that one to have run first.
//!
//! Why a dataset from 2020: ListenBrainz serves current similarity only from a
//! live, rate-limited API, which ADR 0002 forbids in the critical path --
//! covering three million artists would take three million requests. The
//! published dump is CC0, complete, and already an enormous step up from what
//! per-request hydration achieves. A freshly computed graph is its own stage.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream table and column names throughout")]

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bzip2::read::MultiBzDecoder;
use clap::Args as ClapArgs;
use serde::Deserialize;
use sqlx::PgPool;

/// How many edges to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

/// Identifies this metric in `similarity_metric`. Scores from different
/// metrics share a table but are never comparable, so every edge carries the
/// key of the metric that produced it.
const METRIC_KEY: &str = "listenbrainz-2020";

const METRIC_DESCRIPTION: &str =
    "Artist similarity from ListenBrainz co-listening, published 2020-08-13 as artist_credit relations (CC0). Scores run 0.0-1.0 on this metric's own scale.";

#[derive(ClapArgs)]
pub struct Args {
    /// Path to an artist-credit-artist-credit-relations tar.bz2 from
    /// data.metabrainz.org.
    #[arg(long, value_name = "FILE")]
    pub dump: PathBuf,

    /// Ignore edges weaker than this score. The dataset's tail is very long --
    /// most edges are near zero -- and a threshold keeps the graph to the
    /// relationships that carry meaning.
    #[arg(long, default_value_t = 0.0, value_name = "SCORE")]
    pub min_score: f32,

    /// The dataset version. Defaults to the TIMESTAMP file inside the archive.
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,
}

/// One line of the dataset.
#[derive(Deserialize)]
struct Relation {
    id_0: i32,
    id_1: i32,
    score: f32,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    // The mapping comes from the MusicBrainz import; without it there is
    // nothing to resolve credit ids against.
    let credits = load_credits(pool).await?;
    if credits.is_empty() {
        bail!("no artist credits in the database: run `lyrid import musicbrainz` first");
    }
    tracing::info!(credits = credits.len(), "resolving similarity against the canon");

    let file = File::open(&args.dump).with_context(|| format!("cannot open {}", args.dump.display()))?;
    let dataset = read_archive(file, &credits, args.min_score)?;

    let version = args
        .version
        .clone()
        .or(dataset.timestamp)
        .context("the dataset carries no TIMESTAMP file; pass --dump-version")?;

    tracing::info!(
        version = %version,
        edges = dataset.edges.len(),
        skipped_unknown = dataset.stats.unknown_credit,
        skipped_weak = dataset.stats.below_threshold,
        skipped_self = dataset.stats.self_edge,
        "dataset read; writing to PostgreSQL"
    );

    write(pool, &dataset.edges, &version, args.min_score).await
}

/// Rows dropped while reading, so the import can report its own coverage
/// rather than leaving the gaps invisible.
#[derive(Default)]
struct Stats {
    unknown_credit: u64,
    below_threshold: u64,
    self_edge: u64,
}

/// One edge of the graph: an ordered artist pair and its strength.
type Edge = (i32, i32, f32);

/// What one pass over the dataset yields.
struct Dataset {
    edges: Vec<Edge>,
    /// The version string from the archive's TIMESTAMP file, when it has one.
    timestamp: Option<String>,
    stats: Stats,
}

async fn load_credits(pool: &PgPool) -> Result<HashMap<i32, i32>> {
    let rows: Vec<(i32, i32)> = sqlx::query_as("SELECT id, artist_id FROM artist_credit")
        .fetch_all(pool)
        .await
        .context("failed to read the artist credit mapping")?;
    Ok(rows.into_iter().collect())
}

/// Reads the dataset, resolving credits to artists as it goes.
///
/// Edges are keyed by the ordered artist pair, so the two credits of one
/// artist collapsing onto the same pair keep only their strongest score
/// rather than colliding on insert.
fn read_archive(file: File, credits: &HashMap<i32, i32>, min_score: f32) -> Result<Dataset> {
    let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, file));
    let mut archive = tar::Archive::new(decoder);
    let mut best: HashMap<(i32, i32), f32> = HashMap::new();
    let mut timestamp = None;
    let mut stats = Stats::default();
    let mut saw_relations = false;

    for entry in archive.entries().context("the dataset is not a readable tar archive")? {
        let mut entry = entry.context("failed to read an archive entry")?;
        let path = entry.path().context("archive entry has an unreadable path")?.into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if name == "TIMESTAMP" {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text).ok();
            timestamp = Some(text.trim().to_string());
            continue;
        }

        if !name.ends_with("relations.json") {
            continue;
        }
        saw_relations = true;

        read_relations(BufReader::with_capacity(1 << 20, entry), credits, min_score, &mut best, &mut stats)?;
    }

    if !saw_relations {
        bail!("no relations file in the archive: is this an artist-credit relations dataset?");
    }

    let edges = best.into_iter().map(|((a, b), score)| (a, b, score)).collect();
    Ok(Dataset { edges, timestamp, stats })
}

/// Turns dataset lines into resolved edges, keeping the strongest score for
/// each artist pair.
fn read_relations<R: BufRead>(reader: R, credits: &HashMap<i32, i32>, min_score: f32, best: &mut HashMap<(i32, i32), f32>, stats: &mut Stats) -> Result<()> {
    for line in reader.lines() {
        let line = line.context("failed to read a dataset line")?;
        if line.trim().is_empty() {
            continue;
        }
        // A malformed line is upstream data, not a reason to abandon seven
        // million good ones.
        let Ok(relation) = serde_json::from_str::<Relation>(&line) else {
            continue;
        };

        if relation.score < min_score {
            stats.below_threshold += 1;
            continue;
        }
        let (Some(&a), Some(&b)) = (credits.get(&relation.id_0), credits.get(&relation.id_1)) else {
            stats.unknown_credit += 1;
            continue;
        };
        // Different credits of one artist ("Nirvana" and "Nirvana feat. X")
        // resolve to the same star; an edge from a star to itself is not a
        // route anywhere.
        if a == b {
            stats.self_edge += 1;
            continue;
        }

        let pair = if a < b { (a, b) } else { (b, a) };
        best.entry(pair)
            .and_modify(|score| *score = score.max(relation.score))
            .or_insert(relation.score);
    }
    Ok(())
}

/// Writes edges and the prominence derived from them, in one transaction.
async fn write(pool: &PgPool, edges: &[(i32, i32, f32)], version: &str, min_score: f32) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('listenbrainz-similarity', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    let description = if min_score > 0.0 {
        format!("{METRIC_DESCRIPTION} Edges below {min_score} were not imported.")
    } else {
        METRIC_DESCRIPTION.to_string()
    };
    let metric_id: i16 = sqlx::query_scalar(
        "INSERT INTO similarity_metric (key, description) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET description = EXCLUDED.description
         RETURNING id",
    )
    .bind(METRIC_KEY)
    .bind(&description)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the similarity metric")?;

    // Replacing one metric leaves the others standing.
    sqlx::query("DELETE FROM artist_similarity WHERE metric_id = $1")
        .bind(metric_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous edges")?;
    sqlx::query("DELETE FROM artist_prominence WHERE metric_id = $1")
        .bind(metric_id)
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous prominence")?;

    let mut written = 0i64;
    for chunk in edges.chunks(BATCH) {
        let sources: Vec<i32> = chunk.iter().map(|(a, _, _)| *a).collect();
        let targets: Vec<i32> = chunk.iter().map(|(_, b, _)| *b).collect();
        let scores: Vec<f32> = chunk.iter().map(|(_, _, s)| *s).collect();

        sqlx::query(
            "INSERT INTO artist_similarity (metric_id, source_id, target_id, score)
             SELECT $1, * FROM UNNEST($2::int[], $3::int[], $4::real[])",
        )
        .bind(metric_id)
        .bind(&sources)
        .bind(&targets)
        .bind(&scores)
        .execute(&mut *tx)
        .await
        .context("failed to write similarity edges")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "similarity edges written");

    // Brightness comes from the graph because no listen-count dump exists:
    // an artist woven into many strong edges is a prominent one. Computed in
    // the database rather than in Rust -- the data is already there, and this
    // is exactly what a database is for.
    let prominence = sqlx::query(
        "INSERT INTO artist_prominence (metric_id, artist_id, degree, weight)
         SELECT $1, artist_id, count(*)::int, sum(score)::real
         FROM (
             SELECT source_id AS artist_id, score FROM artist_similarity WHERE metric_id = $1
             UNION ALL
             SELECT target_id AS artist_id, score FROM artist_similarity WHERE metric_id = $1
         ) AS ends
         GROUP BY artist_id",
    )
    .bind(metric_id)
    .execute(&mut *tx)
    .await
    .context("failed to compute artist prominence")?;
    tracing::info!(rows = prominence.rows_affected(), "artist prominence computed");

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, edges = written, "similarity import complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Credits 10 and 11 are two credits of artist 1; 20 is artist 2.
    fn credits() -> HashMap<i32, i32> {
        HashMap::from([(10, 1), (11, 1), (20, 2), (30, 3)])
    }

    fn read(lines: &str, min_score: f32) -> (Vec<Edge>, Stats) {
        let mut best = HashMap::new();
        let mut stats = Stats::default();
        read_relations(lines.as_bytes(), &credits(), min_score, &mut best, &mut stats).unwrap();
        let mut edges: Vec<Edge> = best.into_iter().map(|((a, b), s)| (a, b, s)).collect();
        edges.sort_by_key(|&(a, b, _)| (a, b));
        (edges, stats)
    }

    #[test]
    fn resolves_credits_to_artists() {
        let (edges, _) = read(r#"{"id_0":10,"id_1":20,"score":0.5}"#, 0.0);
        assert_eq!(edges, vec![(1, 2, 0.5)]);
    }

    #[test]
    fn orders_each_pair_the_same_way() {
        // The same relationship written in either direction must land on one
        // row, or the primary key would hold two contradictory copies.
        let (forward, _) = read(r#"{"id_0":10,"id_1":20,"score":0.5}"#, 0.0);
        let (backward, _) = read(r#"{"id_0":20,"id_1":10,"score":0.5}"#, 0.0);
        assert_eq!(forward, backward);
    }

    #[test]
    fn keeps_the_strongest_score_when_credits_collapse() {
        // Credits 10 and 11 are the same artist, so both lines describe the
        // same pair of stars.
        let (edges, _) = read(
            concat!(r#"{"id_0":10,"id_1":20,"score":0.2}"#, "\n", r#"{"id_0":11,"id_1":20,"score":0.7}"#),
            0.0,
        );
        assert_eq!(edges, vec![(1, 2, 0.7)]);
    }

    #[test]
    fn drops_self_edges() {
        let (edges, stats) = read(r#"{"id_0":10,"id_1":11,"score":0.9}"#, 0.0);
        assert!(edges.is_empty());
        assert_eq!(stats.self_edge, 1);
    }

    #[test]
    fn drops_unknown_credits() {
        let (edges, stats) = read(r#"{"id_0":10,"id_1":9999,"score":0.9}"#, 0.0);
        assert!(edges.is_empty());
        assert_eq!(stats.unknown_credit, 1);
    }

    #[test]
    fn applies_the_score_threshold() {
        let (edges, stats) = read(
            concat!(r#"{"id_0":10,"id_1":20,"score":0.05}"#, "\n", r#"{"id_0":20,"id_1":30,"score":0.5}"#),
            0.1,
        );
        assert_eq!(edges, vec![(2, 3, 0.5)]);
        assert_eq!(stats.below_threshold, 1);
    }

    #[test]
    fn skips_malformed_lines_without_failing() {
        let (edges, _) = read(concat!("not json\n", r#"{"id_0":10,"id_1":20,"score":0.5}"#, "\n\n"), 0.0);
        assert_eq!(edges, vec![(1, 2, 0.5)]);
    }
}
