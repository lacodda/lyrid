//! The `lyrid layout` command: graph in, sky out.
//!
//! Reads the similarity graph, runs the force-directed layout, stores the
//! positions as a versioned layout, and cuts the tile pyramid ADR 0003 calls
//! for. Everything here is offline batch work — nothing in the serving path
//! touches it.
//!
//! **A new edition starts from the last one.** The canon is rebuilt every few
//! stages, and a sky laid out from the spiral each time would move every star:
//! a person's sense of where their sector is, the cameras they saved and the
//! links they sent would all point at somewhere else. So unless asked to draw
//! fresh, the layout starts each star where the newest sealed edition had it
//! (by MusicBrainz id, through the redirects a merge leaves), places the new
//! ones among their neighbours, and runs a few cool rounds with every old star
//! held near its place. The layout keeps that edition's frame, and sealing it
//! measures how far the stars moved. See ADR 0029.
//!
//! **MusicBrainz's special purpose artists are not stars.** "Various Artists"
//! and "[unknown]" are credits, not acts; their edges are left out of the
//! graph, so they get no place (the list is `import::special`).

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use sqlx::PgPool;

use super::cut;
use super::force::{self, CONTINUED_ITERATIONS, Graph, Params, Start};
use super::tiles::Plan;
use crate::import::special::special_purpose;

/// How many rows to send to Postgres at once.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// Which similarity metric to lay out. Defaults to the only one present,
    /// which is the usual case.
    #[arg(long, value_name = "KEY")]
    pub metric: Option<String>,

    /// Name for this layout, e.g. `listenbrainz-2020-fa-v1`. Defaults to the
    /// metric key with how it was laid out appended -- the edition it
    /// continues, or the rounds and the seed of a fresh one -- so two runs
    /// cannot quietly overwrite each other.
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,

    /// Draw the sky fresh, from the spiral, rather than from the newest sealed
    /// edition: every star moves, and a new frame begins, so the cameras and
    /// view links saved in the old one stop pointing anywhere. For a graph that
    /// is not the one before (another metric), whose old places would be a
    /// worse start than none. The first sky is drawn fresh without asking.
    #[arg(long)]
    pub fresh: bool,

    /// Rounds of forces. More is smoother and slower; the number is recorded
    /// with the layout because it is part of what makes a run reproducible.
    /// Defaults to 300 for a fresh sky and 40 for one continuing an edition,
    /// which only has to make room for what is new.
    #[arg(long)]
    pub iterations: Option<u32>,

    /// Random seed. Recorded, so the same sky can be built again.
    #[arg(long, default_value_t = 0x5EED)]
    pub seed: u64,

    /// Barnes-Hut opening angle: smaller is more exact and slower.
    #[arg(long, default_value_t = 0.5)]
    pub theta: f32,

    /// Ignore edges weaker than this. The graph's tail is very long, and a
    /// layout spends most of its time on edges too faint to see.
    #[arg(long, default_value_t = 0.0)]
    pub min_score: f32,

    /// Where to write the tile pyramid. Without this the layout is stored but
    /// no tiles are cut.
    #[arg(long, value_name = "DIR")]
    pub tiles: Option<PathBuf>,

    /// Deepest tile level to build.
    #[arg(long, default_value_t = 6)]
    pub max_level: u8,

    /// How many of the brightest stars the top-level tile shows.
    #[arg(long, default_value_t = 2000)]
    pub level0_stars: usize,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    let (metric_id, metric_key) = resolve_metric(pool, args.metric.as_deref()).await?;
    tracing::info!(metric = %metric_key, "laying out the sky");

    let edges = load_edges(pool, metric_id, args.min_score).await?;
    if edges.is_empty() {
        bail!(
            "the similarity graph is empty for metric `{metric_key}`: run `lyrid import listenbrainz` first, \
             or lower --min-score"
        );
    }

    let graph = Graph::from_edges(&edges);
    tracing::info!(stars = graph.len(), edges = edges.len(), "graph built");
    drop(edges);

    let origin = if args.fresh { None } else { origin(pool).await? };
    let (params, start, key, description, frame) = match &origin {
        Some(origin) => {
            let start = Start::from_previous(&graph, &origin.places);
            if start.held() == 0 {
                bail!(
                    "none of edition {}'s stars is in this graph, so there is nothing to continue: \
                     lay the sky out with --fresh, which starts a frame of its own",
                    origin.edition
                );
            }
            tracing::info!(
                edition = origin.edition,
                held = start.held(),
                among_neighbours = start.by_neighbours,
                on_the_ring = start.by_ring,
                "continuing the newest edition: its stars start where they were"
            );
            let iterations = args.iterations.unwrap_or(CONTINUED_ITERATIONS);
            let params = Params {
                theta: args.theta,
                ..Params::continued(iterations, args.seed)
            };
            let key = args.key.clone().unwrap_or_else(|| format!("{metric_key}-e{}-c{iterations}", origin.edition));
            let description = format!(
                "Continued from edition {} in its frame: {} stars held where they were (anchor {}), {} new among their \
                 neighbours, {} on the ring; then force-directed with Barnes-Hut (theta {}), {iterations} iterations \
                 from heat {}, edges above {}.",
                origin.edition,
                start.held(),
                params.anchor,
                start.by_neighbours,
                start.by_ring,
                args.theta,
                params.heat,
                args.min_score
            );
            (params, Some(start), key, description, Some(origin.frame.clone()))
        }
        None => {
            let iterations = args.iterations.unwrap_or(FRESH_ITERATIONS);
            let params = Params {
                iterations,
                theta: args.theta,
                seed: args.seed,
                ..Params::default()
            };
            tracing::info!("drawing the sky fresh, from the spiral: a frame of its own");
            let key = args.key.clone().unwrap_or_else(|| format!("{metric_key}-fd{iterations}-s{}", args.seed));
            let description = format!(
                "Force-directed with Barnes-Hut (theta {}), {iterations} iterations, seed {}, edges above {}.",
                args.theta, args.seed, args.min_score
            );
            (params, None, key, description, None)
        }
    };

    let report = |iteration, movement| {
        // Every tenth round, and the last: enough to see it settling without
        // burying the log.
        if iteration % 10 == 0 || iteration == params.iterations {
            tracing::info!(iteration, movement, "laying out");
        }
    };
    let positions = match start {
        Some(start) => force::run_from(&graph, &params, &start, report),
        None => force::run(&graph, &params, report),
    };

    let layout_id = write_layout(pool, metric_id, &key, frame.as_deref(), &description, args.seed, graph.len()).await?;
    write_positions(pool, layout_id, &graph, &positions).await?;

    // Cut from what was just stored rather than from memory, so this and
    // `lyrid tiles` are one code path and cannot write different skies.
    if let Some(directory) = &args.tiles {
        let plan = Plan {
            max_level: args.max_level,
            level0_stars: args.level0_stars,
        };
        cut::cut(pool, layout_id, directory, &plan).await?;
    }

    tracing::info!(layout = %key, stars = graph.len(), "layout complete");
    Ok(())
}

/// Which metric to lay out, and its key for naming.
async fn resolve_metric(pool: &PgPool, wanted: Option<&str>) -> Result<(i16, String)> {
    let rows: Vec<(i16, String)> = sqlx::query_as("SELECT id, key FROM similarity_metric ORDER BY id")
        .fetch_all(pool)
        .await
        .context("failed to read similarity metrics")?;

    match (wanted, rows.len()) {
        (Some(key), _) => rows
            .into_iter()
            .find(|(_, k)| k == key)
            .with_context(|| format!("no similarity metric named `{key}`")),
        (None, 1) => Ok(rows.into_iter().next().expect("length checked")),
        (None, 0) => bail!("no similarity metric in the database: run `lyrid import listenbrainz` first"),
        (None, _) => bail!(
            "several similarity metrics exist; choose one with --metric: {}",
            rows.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// The rounds a fresh sky runs by default.
const FRESH_ITERATIONS: u32 = 300;

/// The whole graph, as edges, without the special purpose artists.
///
/// Loaded in one query rather than streamed: seven million rows is about
/// 80 MB, and the layout needs all of it in memory anyway.
async fn load_edges(pool: &PgPool, metric_id: i16, min_score: f32) -> Result<Vec<(i32, i32, f32)>> {
    let special: Vec<i32> = sqlx::query_scalar("SELECT id FROM artist WHERE mbid = ANY($1)")
        .bind(special_purpose())
        .fetch_all(pool)
        .await
        .context("failed to find the special purpose artists")?;
    let rows: Vec<(i32, i32, f32)> = sqlx::query_as(
        "SELECT source_id, target_id, score FROM artist_similarity
         WHERE metric_id = $1 AND score >= $2
           AND source_id <> ALL($3) AND target_id <> ALL($3)",
    )
    .bind(metric_id)
    .bind(min_score)
    .bind(&special)
    .fetch_all(pool)
    .await
    .context("failed to read the similarity graph")?;
    Ok(rows)
}

/// The edition a new layout continues: its number, its frame, and where each
/// of its stars was, by the canon's artist id now.
struct Origin {
    edition: i16,
    frame: String,
    places: HashMap<i32, (f32, f32)>,
}

/// The newest sealed edition and its stars' places, or `None` when nothing
/// has been sealed yet and the first sky is drawn fresh.
///
/// A star is found by its MusicBrainz id, and failing that through the
/// redirect MusicBrainz left when it merged that id into another artist: the
/// artist it became starts where it was. When several old stars became one,
/// the artist's own old place wins, then the brightest of the merged ones.
async fn origin(pool: &PgPool) -> Result<Option<Origin>> {
    let Some((edition, frame)) = sqlx::query_as::<_, (i16, String)>("SELECT id, frame FROM canon_edition ORDER BY id DESC LIMIT 1")
        .fetch_optional(pool)
        .await
        .context("failed to find the newest edition")?
    else {
        tracing::info!("no edition has been sealed yet: the first sky is drawn fresh");
        return Ok(None);
    };

    let rows: Vec<(i32, f32, f32)> = sqlx::query_as(
        "SELECT DISTINCT ON (artist_id) artist_id, x, y
         FROM (
             SELECT a.id AS artist_id, s.x, s.y, 0 AS via, s.rank
             FROM canon_edition_star s JOIN artist a ON a.mbid = s.mbid
             WHERE s.edition_id = $1 AND s.x IS NOT NULL
             UNION ALL
             SELECT r.artist_id, s.x, s.y, 1, s.rank
             FROM canon_edition_star s JOIN artist_redirect r ON r.mbid = s.mbid
             WHERE s.edition_id = $1 AND s.x IS NOT NULL
         ) AS places
         ORDER BY artist_id, via, rank NULLS LAST",
    )
    .bind(edition)
    .fetch_all(pool)
    .await
    .context("failed to read where the edition's stars were")?;
    if rows.is_empty() {
        bail!(
            "edition {edition} recorded no places for its stars (it was sealed before they were kept), \
             so there is nothing to start from: lay the sky out with --fresh, which starts a frame of its own"
        );
    }

    Ok(Some(Origin {
        edition,
        frame,
        places: rows.into_iter().map(|(artist, x, y)| (artist, (x, y))).collect(),
    }))
}

/// Records the layout itself, replacing an earlier run of the same name.
///
/// `frame` is the frame of the edition it continues; `None` for a sky drawn
/// fresh, which the database gives a frame of its own: its key.
async fn write_layout(pool: &PgPool, metric_id: i16, key: &str, frame: Option<&str>, description: &str, seed: u64, stars: usize) -> Result<i16> {
    let seed = i64::try_from(seed).unwrap_or(i64::MAX);
    let id: i16 = sqlx::query_scalar(
        "INSERT INTO sky_layout (key, frame, metric_id, description, seed, stars)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (key) DO UPDATE
             SET frame = EXCLUDED.frame,
                 metric_id = EXCLUDED.metric_id,
                 description = EXCLUDED.description,
                 seed = EXCLUDED.seed,
                 stars = EXCLUDED.stars,
                 created_at = now()
         RETURNING id",
    )
    .bind(key)
    .bind(frame)
    .bind(metric_id)
    .bind(description)
    .bind(seed)
    .bind(i32::try_from(stars).unwrap_or(i32::MAX))
    .fetch_one(pool)
    .await
    .context("failed to record the layout")?;

    // Replacing a layout of the same name replaces its positions too;
    // otherwise the old sky would show through the new one.
    sqlx::query("DELETE FROM artist_position WHERE layout_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("failed to clear the previous positions")?;
    Ok(id)
}

async fn write_positions(pool: &PgPool, layout_id: i16, graph: &Graph, positions: &force::Positions) -> Result<()> {
    let mut written = 0i64;
    for chunk in (0..graph.len()).collect::<Vec<_>>().chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|&i| graph.artist_ids[i]).collect();
        let xs: Vec<f32> = chunk.iter().map(|&i| positions.xs[i]).collect();
        let ys: Vec<f32> = chunk.iter().map(|&i| positions.ys[i]).collect();

        sqlx::query(
            "INSERT INTO artist_position (layout_id, artist_id, x, y)
             SELECT $1, * FROM UNNEST($2::int[], $3::real[], $4::real[])
             ON CONFLICT (layout_id, artist_id) DO NOTHING",
        )
        .bind(layout_id)
        .bind(&artists)
        .bind(&xs)
        .bind(&ys)
        .execute(pool)
        .await
        .context("failed to write positions")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "positions written");
    Ok(())
}
