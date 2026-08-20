//! The `lyrid layout` command: graph in, sky out.
//!
//! Reads the similarity graph, runs the force-directed layout, stores the
//! positions as a versioned layout, and cuts the tile pyramid ADR 0003 calls
//! for. Everything here is offline batch work — nothing in the serving path
//! touches it.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use sqlx::PgPool;

use super::force::{self, Graph, Params};
use super::tiles::{self, Bounds, Plan, Star};

/// How many rows to send to Postgres at once.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// Which similarity metric to lay out. Defaults to the only one present,
    /// which is the usual case.
    #[arg(long, value_name = "KEY")]
    pub metric: Option<String>,

    /// Name for this layout, e.g. `listenbrainz-2020-fa-v1`. Defaults to the
    /// metric key with the parameters appended, so two runs with different
    /// settings cannot quietly overwrite each other.
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,

    /// Rounds of forces. More is smoother and slower; the number is recorded
    /// with the layout because it is part of what makes a run reproducible.
    #[arg(long, default_value_t = 300)]
    pub iterations: u32,

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
    tracing::info!(stars = graph.len(), edges = edges.len(), "graph built; running the layout");
    drop(edges);

    let params = Params {
        iterations: args.iterations,
        theta: args.theta,
        seed: args.seed,
        ..Params::default()
    };

    let positions = force::run(&graph, &params, |iteration, movement| {
        // Every tenth round, and the last: enough to see it settling without
        // burying the log.
        if iteration % 10 == 0 || iteration == params.iterations {
            tracing::info!(iteration, movement, "laying out");
        }
    });

    let key = args.key.clone().unwrap_or_else(|| format!("{metric_key}-fd{}-s{}", args.iterations, args.seed));
    let description = format!(
        "Force-directed with Barnes-Hut (theta {}), {} iterations, seed {}, edges above {}.",
        args.theta, args.iterations, args.seed, args.min_score
    );

    let layout_id = write_layout(pool, metric_id, &key, &description, args.seed, graph.len()).await?;
    write_positions(pool, layout_id, &graph, &positions).await?;

    if let Some(directory) = &args.tiles {
        let stars = stars_for_tiles(pool, layout_id, &graph, &positions).await?;
        let bounds = Bounds::of(&positions.xs, &positions.ys);
        let plan = Plan {
            max_level: args.max_level,
            level0_stars: args.level0_stars,
        };
        write_tiles(directory, &stars, &bounds, &plan)?;
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

/// The whole graph, as edges.
///
/// Loaded in one query rather than streamed: seven million rows is about
/// 80 MB, and the layout needs all of it in memory anyway.
async fn load_edges(pool: &PgPool, metric_id: i16, min_score: f32) -> Result<Vec<(i32, i32, f32)>> {
    let rows: Vec<(i32, i32, f32)> = sqlx::query_as(
        "SELECT source_id, target_id, score FROM artist_similarity
         WHERE metric_id = $1 AND score >= $2",
    )
    .bind(metric_id)
    .bind(min_score)
    .fetch_all(pool)
    .await
    .context("failed to read the similarity graph")?;
    Ok(rows)
}

/// Records the layout itself, replacing an earlier run of the same name.
async fn write_layout(pool: &PgPool, metric_id: i16, key: &str, description: &str, seed: u64, stars: usize) -> Result<i16> {
    let seed = i64::try_from(seed).unwrap_or(i64::MAX);
    let id: i16 = sqlx::query_scalar(
        "INSERT INTO sky_layout (key, metric_id, description, seed, stars)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (key) DO UPDATE
             SET metric_id = EXCLUDED.metric_id,
                 description = EXCLUDED.description,
                 seed = EXCLUDED.seed,
                 stars = EXCLUDED.stars,
                 created_at = now()
         RETURNING id",
    )
    .bind(key)
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

/// Stars for the tile pyramid, brightest first.
///
/// Brightness comes from `artist_prominence` — connectivity, which ADR 0004
/// settled on because no listen counts exist as a dump. It is normalised here
/// so the client uploads a 0..1 value and does no scaling per frame.
async fn stars_for_tiles(pool: &PgPool, layout_id: i16, graph: &Graph, positions: &force::Positions) -> Result<Vec<Star>> {
    let rows: Vec<(i32, f32)> = sqlx::query_as(
        "SELECT p.artist_id, COALESCE(pr.weight, 0)
         FROM artist_position p
         LEFT JOIN sky_layout l ON l.id = p.layout_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE p.layout_id = $1",
    )
    .bind(layout_id)
    .fetch_all(pool)
    .await
    .context("failed to read star brightness")?;

    let weights: std::collections::HashMap<i32, f32> = rows.into_iter().collect();
    // Normalised against the brightest star rather than against a constant:
    // the scale of connectivity depends on the metric, and a fixed divisor
    // would wash out one graph and clip another.
    let max_weight = weights.values().copied().fold(0f32, f32::max).max(f32::MIN_POSITIVE);

    let mut stars: Vec<Star> = (0..graph.len())
        .map(|i| {
            let artist_id = graph.artist_ids[i];
            Star {
                artist_id,
                x: positions.xs[i],
                y: positions.ys[i],
                // Square root, so the long tail of faint stars stays visible
                // rather than collapsing to zero: connectivity is heavily
                // skewed, and a linear scale would show only the hubs.
                brightness: (weights.get(&artist_id).copied().unwrap_or(0.0) / max_weight).sqrt(),
            }
        })
        .collect();

    // Brightest first: the pyramid is a "brightest wins" filter, and sorting
    // once here saves sorting per level.
    stars.sort_by(|a, b| b.brightness.total_cmp(&a.brightness).then(a.artist_id.cmp(&b.artist_id)));
    Ok(stars)
}

/// Cuts and writes the pyramid.
fn write_tiles(directory: &std::path::Path, stars: &[Star], bounds: &Bounds, plan: &Plan) -> Result<()> {
    let tiles = tiles::build(stars, bounds, plan);
    tracing::info!(tiles = tiles.len(), "cutting the tile pyramid");

    let mut bytes = 0u64;
    for tile in &tiles {
        let path = directory.join(tiles::path(tile.id));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path).with_context(|| format!("cannot write {}", path.display()))?);
        tiles::write(tile, &mut file)?;
        bytes += (tiles::HEADER + tile.stars.len() * tiles::RECORD) as u64;
    }

    // The bounds belong with the tiles: without them a client cannot turn a
    // screen position into a tile, and they are a property of this layout
    // rather than of the product.
    let meta = format!(
        "{{\"min_x\":{},\"min_y\":{},\"max_x\":{},\"max_y\":{},\"max_level\":{},\"record_bytes\":{}}}",
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
        tiles.iter().map(|t| t.id.level).max().unwrap_or(0),
        tiles::RECORD
    );
    std::fs::write(directory.join("sky.json"), meta).context("cannot write the tile metadata")?;

    tracing::info!(tiles = tiles.len(), kilobytes = bytes / 1024, "tiles written");
    Ok(())
}
