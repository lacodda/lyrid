//! Cutting a stored layout into what the client fetches: tiles and names.
//!
//! Split out of `lyrid layout` because the two move on different clocks. A
//! layout is hours of forces over millions of edges and changes when the graph
//! does; the pyramid is a few seconds of sorting and changes whenever the
//! *format* does -- a year added to every record, a file of names beside the
//! tiles. Tying them together would mean re-running the forces to ship a
//! format change, and a slice on a small stand could not be re-cut at all.
//!
//! So everything here reads the layout back from the database rather than
//! taking it from memory. `lyrid layout --tiles` calls it once the positions
//! are written; `lyrid tiles` calls it on its own.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use sqlx::PgPool;

use super::labels::{self, Kind};
use super::tiles::{self, Bounds, Plan, Star};

#[derive(ClapArgs)]
pub struct Args {
    /// Where to write the tile pyramid and its `sky.json` and `labels.json`.
    #[arg(long, value_name = "DIR", default_value = "tiles")]
    pub out: PathBuf,

    /// Which stored layout to cut. Defaults to the newest, which is the one
    /// the card and the nearby list read positions from.
    #[arg(long, value_name = "KEY")]
    pub layout: Option<String>,

    /// Deepest tile level to build.
    #[arg(long, default_value_t = 6)]
    pub max_level: u8,

    /// How many of the brightest stars the top-level tile shows.
    #[arg(long, default_value_t = 2000)]
    pub level0_stars: usize,
}

/// A style carried by fewer artists than this gets no name on the sky.
const MIN_MEMBERS: usize = 25;

/// How much denser than an even spread a style must be where its name is
/// written; see `labels::labels` for the measurement behind the number.
const MIN_LIFT: f32 = 8.0;

/// How many style names the sky carries at most.
///
/// The client already drops a name that would overlap a larger one, so this is
/// not what keeps the screen legible; it keeps the file small and the long
/// tail of scenes of a few dozen acts from being shipped to every visitor.
const MAX_STYLES: usize = 150;

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    let layout_id: Option<i16> = match &args.layout {
        Some(key) => sqlx::query_scalar("SELECT id FROM sky_layout WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .context("failed to read layouts")?,
        None => sqlx::query_scalar("SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(pool)
            .await
            .context("failed to read layouts")?,
    };
    let Some(layout_id) = layout_id else {
        match &args.layout {
            Some(key) => bail!("no layout named `{key}`"),
            None => bail!("no layout has been stored yet: run `lyrid layout` first"),
        }
    };

    let plan = Plan {
        max_level: args.max_level,
        level0_stars: args.level0_stars,
    };
    cut(pool, layout_id, &args.out, &plan).await
}

/// Writes the pyramid, `sky.json` and `labels.json` for one stored layout.
pub async fn cut(pool: &PgPool, layout_id: i16, directory: &Path, plan: &Plan) -> Result<()> {
    let stars = stars(pool, layout_id).await?;
    if stars.is_empty() {
        bail!("layout {layout_id} has no positions to cut");
    }
    let xs: Vec<f32> = stars.iter().map(|s| s.x).collect();
    let ys: Vec<f32> = stars.iter().map(|s| s.y).collect();
    let bounds = Bounds::of(&xs, &ys);

    let tiles = tiles::build(&stars, &bounds, plan);
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
    let meta = serde_json::json!({
        "min_x": bounds.min_x,
        "min_y": bounds.min_y,
        "max_x": bounds.max_x,
        "max_y": bounds.max_y,
        "max_level": tiles.iter().map(|t| t.id.level).max().unwrap_or(0),
        "record_bytes": tiles::RECORD,
        "stamp": stamp(),
    });
    std::fs::write(directory.join("sky.json"), meta.to_string()).context("cannot write the tile metadata")?;
    tracing::info!(tiles = tiles.len(), kilobytes = bytes / 1024, "tiles written");

    let groups = groups(pool, layout_id).await?;
    let names = labels::labels(&groups, &bounds, MIN_MEMBERS, MAX_STYLES, MIN_LIFT);
    let file = serde_json::json!({ "version": 1, "labels": names });
    std::fs::write(directory.join("labels.json"), file.to_string()).context("cannot write the labels")?;
    tracing::info!(labels = names.len(), "labels written");
    Ok(())
}

/// Which cut this is, for the client to put in every tile address.
///
/// Tiles are cached by browsers for a day under addresses the next cut reuses,
/// so without this a returning visitor draws yesterday's pyramid -- and after
/// a format change, refuses it and draws nothing. `sky.json` is never cached,
/// so a stamp in it reaches every visitor at once and turns each cut's tiles
/// into addresses of their own. The moment of cutting is enough: two cuts in
/// the same second are the same run.
fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Every placed star, brightest first.
///
/// Brightness comes from `artist_prominence` -- connectivity, which ADR 0004
/// settled on because no listen counts exist as a dump. It is normalised here
/// so the client uploads a 0..1 value and does no scaling per frame.
///
/// The year follows the card's rule: `MusicBrainz`'s own begin year, and
/// Wikidata's inception only where `MusicBrainz` has none. One rule in two
/// places would be two answers to "when did they start" -- so the card and the
/// time machine read it the same way.
async fn stars(pool: &PgPool, layout_id: i16) -> Result<Vec<Star>> {
    let rows: Vec<(i32, f32, f32, f32, Option<i16>)> = sqlx::query_as(
        "SELECT p.artist_id, p.x, p.y, COALESCE(pr.weight, 0), COALESCE(a.begin_year, f.inception_year)
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         JOIN artist a ON a.id = p.artist_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         LEFT JOIN artist_fact f ON f.artist_id = p.artist_id
         WHERE p.layout_id = $1",
    )
    .bind(layout_id)
    .fetch_all(pool)
    .await
    .context("failed to read the stored layout")?;

    // Normalised against the brightest star rather than against a constant:
    // the scale of connectivity depends on the metric, and a fixed divisor
    // would wash out one graph and clip another.
    let max_weight = rows.iter().map(|r| r.3).fold(0f32, f32::max).max(f32::MIN_POSITIVE);

    let mut stars: Vec<Star> = rows
        .into_iter()
        .map(|(artist_id, x, y, weight, year)| Star {
            artist_id,
            x,
            y,
            // Square root, so the long tail of faint stars stays visible
            // rather than collapsing to zero: connectivity is heavily
            // skewed, and a linear scale would show only the hubs.
            brightness: (weight / max_weight).sqrt(),
            begin_year: plausible_year(year),
        })
        .collect();

    // Brightest first: the pyramid is a "brightest wins" filter, and sorting
    // once here saves sorting per level.
    stars.sort_by(|a, b| b.brightness.total_cmp(&a.brightness).then(a.artist_id.cmp(&b.artist_id)));
    Ok(stars)
}

/// A year the time machine can place, or 0 for "not known".
///
/// Both sources are crowdsourced at the edges, and a begin year of 12 or 3024
/// is a typo rather than a fact: drawn, it would put a star at the very start
/// or the very end of the slider and nowhere near its era. Such a star is
/// treated as undated, which the client draws as undated -- honest, where a
/// wrong year would not be.
fn plausible_year(year: Option<i16>) -> i16 {
    match year {
        Some(year) if (1000..=2100).contains(&year) => year,
        _ => 0,
    }
}

/// Each genre and style with the positions of the artists that carry it as
/// their *main* one.
///
/// Main, not any: an artist's genres run long -- a jazz musician with one
/// electronic remix carries "Electronic" -- and counting every mention would
/// pull each label towards the crossover acts that are loosely everything.
/// "Main" is the `main_genres` function, the one definition the compass and
/// the radio read too: a name on the sky and the radio of that name play the
/// same stars.
async fn groups(pool: &PgPool, layout_id: i16) -> Result<Vec<labels::Group>> {
    let rows: Vec<(String, bool, f32, f32)> = sqlx::query_as(
        "SELECT g.name, g.is_style, p.x, p.y
         FROM artist_position p
         JOIN main_genres(ARRAY(SELECT artist_id FROM artist_position WHERE layout_id = $1)) main
             ON main.artist_id = p.artist_id
         JOIN genre g ON g.id = main.genre_id
         WHERE p.layout_id = $1",
    )
    .bind(layout_id)
    .fetch_all(pool)
    .await
    .context("failed to read the genres of placed artists")?;

    let mut by_name = std::collections::BTreeMap::<(String, bool), Vec<(f32, f32)>>::new();
    for (name, is_style, x, y) in rows {
        by_name.entry((name, is_style)).or_default().push((x, y));
    }
    Ok(by_name
        .into_iter()
        .map(|((name, is_style), members)| (name, if is_style { Kind::Style } else { Kind::Genre }, members))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typo_of_a_year_is_undated_rather_than_placed() {
        assert_eq!(plausible_year(Some(1969)), 1969);
        assert_eq!(plausible_year(None), 0);
        // Found in crowdsourced data: years that are digits dropped or added.
        assert_eq!(plausible_year(Some(12)), 0);
        assert_eq!(plausible_year(Some(3024)), 0);
        assert_eq!(plausible_year(Some(-500)), 0);
    }
}
