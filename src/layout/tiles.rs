//! The tile pyramid: the sky cut into files a browser can fetch.
//!
//! ADR 0003 settles the shape. Browsing the sky never queries the database —
//! the client fetches small binary tiles for the region and zoom it is looking
//! at, and those tiles are static files a CDN can cache. This module turns a
//! finished layout into that pyramid.
//!
//! **Levels are a popularity filter, not a resolution change.** Level 0 is the
//! whole sky in one tile holding only the brightest stars; each level down
//! quadruples the tile count and lowers the threshold, so zooming in reveals
//! more stars rather than bigger ones. A star appears at every level from the
//! first one that admits it, so panning at a fixed zoom never pops stars in
//! and out.
//!
//! **The format is deliberately dumb**: a small header and then fixed-size
//! records. The client's hot path is uploading these straight into a GPU
//! buffer, so anything that needs parsing — JSON, protobuf, varints — would be
//! work done per frame for no benefit.

use std::io::Write;

/// Bytes per star in a tile: two coordinates, brightness, and the id.
///
/// The id is what a click turns into an artist, so it cannot be dropped; at
/// four bytes it also matches the `MusicBrainz` integer id the canon uses.
pub const RECORD: usize = 16;

/// Magic and version, so a stale tile from an older layout is refused rather
/// than drawn as garbage.
const MAGIC: &[u8; 4] = b"LYST";
const VERSION: u16 = 1;

/// The header of every tile: 16 bytes, then the records.
pub const HEADER: usize = 16;

/// One star, as the tile stores it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Star {
    pub artist_id: i32,
    /// Position in world coordinates.
    pub x: f32,
    pub y: f32,
    /// How brightly to draw it, already normalised to 0..1 so the client does
    /// no scaling.
    pub brightness: f32,
}

/// The square of world space a tile covers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TileId {
    pub level: u8,
    pub col: u32,
    pub row: u32,
}

/// The extent of the whole sky, needed to map world coordinates onto tiles.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Bounds {
    /// The bounding square of a layout, padded so no star sits exactly on the
    /// edge and rounds into a neighbouring tile.
    #[must_use]
    pub fn of(xs: &[f32], ys: &[f32]) -> Self {
        if xs.is_empty() {
            return Self {
                min_x: -1.0,
                min_y: -1.0,
                max_x: 1.0,
                max_y: 1.0,
            };
        }

        let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
        let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
        for (&x, &y) in xs.iter().zip(ys) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }

        // A square, so tiles stay square at every level and the client's
        // arithmetic is one scale factor rather than two.
        let cx = f32::midpoint(min_x, max_x);
        let cy = f32::midpoint(min_y, max_y);
        let half = ((max_x - min_x).max(max_y - min_y) * 0.5).max(1e-3) * 1.02;
        Self {
            min_x: cx - half,
            min_y: cy - half,
            max_x: cx + half,
            max_y: cy + half,
        }
    }

    /// Which tile a point falls into at a given level.
    #[must_use]
    pub fn tile_of(&self, level: u8, x: f32, y: f32) -> TileId {
        let side = 1u32 << level;
        // Levels are single digits, so `side` is at most a few hundred and
        // every conversion here is exact -- and the values are clamped into
        // range before they come back, so nothing can truncate.
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "side is 2^level with level below 32; values are clamped in range"
        )]
        {
            let side_f = side as f32;
            let span = (self.max_x - self.min_x).max(f32::MIN_POSITIVE);
            let fx = ((x - self.min_x) / span * side_f).floor();
            let fy = ((y - self.min_y) / span * side_f).floor();
            // Clamped rather than trusted: a star exactly on the far edge
            // would otherwise index one tile past the end.
            let col = fx.clamp(0.0, side_f - 1.0) as u32;
            let row = fy.clamp(0.0, side_f - 1.0) as u32;
            TileId { level, col, row }
        }
    }
}

/// How many stars a level admits, and the pyramid's depth.
#[derive(Clone, Copy)]
pub struct Plan {
    /// Deepest level to build. Level `n` has `4^n` tiles, so this is what
    /// bounds the number of files.
    pub max_level: u8,
    /// How many of the brightest stars level 0 shows. Each level multiplies
    /// this, so the budget per tile stays roughly constant while the tile
    /// count grows.
    pub level0_stars: usize,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            max_level: 6,
            level0_stars: 2000,
        }
    }
}

impl Plan {
    /// How many stars appear at or above a level.
    ///
    /// Quadrupling per level keeps roughly `level0_stars` per tile: the tile
    /// count quadruples too. That is what holds the client's per-tile decode
    /// cost flat as it zooms.
    #[must_use]
    pub fn stars_at(&self, level: u8) -> usize {
        self.level0_stars.saturating_mul(1usize << (2 * u32::from(level)))
    }
}

/// One tile ready to be written.
pub struct Tile {
    pub id: TileId,
    pub stars: Vec<Star>,
}

/// Cuts a layout into a pyramid.
///
/// `stars` must be sorted by descending brightness: the pyramid is a
/// "brightest first" filter, and sorting once here beats sorting per level.
#[must_use]
pub fn build(stars: &[Star], bounds: &Bounds, plan: &Plan) -> Vec<Tile> {
    debug_assert!(
        stars.windows(2).all(|w| w[0].brightness >= w[1].brightness),
        "stars must be sorted brightest first"
    );

    let mut tiles: Vec<Tile> = Vec::new();
    for level in 0..=plan.max_level {
        let admitted = plan.stars_at(level).min(stars.len());
        let mut by_tile: std::collections::HashMap<(u32, u32), Vec<Star>> = std::collections::HashMap::new();

        for star in &stars[..admitted] {
            let id = bounds.tile_of(level, star.x, star.y);
            by_tile.entry((id.col, id.row)).or_default().push(*star);
        }

        // Sorted so the output is the same on every run: a tile set is part
        // of a versioned layout, and hash order is not reproducible.
        let mut keys: Vec<(u32, u32)> = by_tile.keys().copied().collect();
        keys.sort_unstable();
        for (col, row) in keys {
            let stars = by_tile.remove(&(col, row)).unwrap_or_default();
            tiles.push(Tile {
                id: TileId { level, col, row },
                stars,
            });
        }

        // Every star already fits in this level, so deeper levels would only
        // repeat it in smaller squares.
        if admitted == stars.len() {
            break;
        }
    }
    tiles
}

/// Writes one tile in the binary format the client reads.
///
/// Little-endian throughout: every platform the client runs on is
/// little-endian, and a typed array upload needs no byte swapping.
pub fn write(tile: &Tile, out: &mut impl Write) -> std::io::Result<()> {
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&tile.id.level.to_le_bytes())?;
    out.write_all(&[0u8])?; // padding, keeps the records 4-byte aligned
    out.write_all(&u32::try_from(tile.stars.len()).unwrap_or(u32::MAX).to_le_bytes())?;
    out.write_all(&[0u8; 4])?; // reserved, so a later field costs no version bump

    for star in &tile.stars {
        out.write_all(&star.artist_id.to_le_bytes())?;
        out.write_all(&star.x.to_le_bytes())?;
        out.write_all(&star.y.to_le_bytes())?;
        out.write_all(&star.brightness.to_le_bytes())?;
    }
    Ok(())
}

/// The path a tile is served at, relative to the tile root.
#[must_use]
pub fn path(id: TileId) -> String {
    format!("{}/{}/{}.bin", id.level, id.col, id.row)
}

#[cfg(test)]
#[expect(clippy::cast_precision_loss, reason = "test fixtures build coordinates from small loop counters")]
#[expect(
    clippy::float_cmp,
    reason = "these assert an exact byte layout: the values are written and read back with no arithmetic between"
)]
mod tests {
    use super::*;

    fn star(id: i32, x: f32, y: f32, brightness: f32) -> Star {
        Star {
            artist_id: id,
            x,
            y,
            brightness,
        }
    }

    fn sorted(mut stars: Vec<Star>) -> Vec<Star> {
        stars.sort_by(|a, b| b.brightness.total_cmp(&a.brightness));
        stars
    }

    #[test]
    fn level_zero_is_one_tile_holding_the_whole_sky() {
        let stars = sorted(vec![star(1, -5.0, -5.0, 1.0), star(2, 5.0, 5.0, 0.9)]);
        let bounds = Bounds::of(&[-5.0, 5.0], &[-5.0, 5.0]);
        let tiles = build(&stars, &bounds, &Plan::default());

        let level0: Vec<&Tile> = tiles.iter().filter(|t| t.id.level == 0).collect();
        assert_eq!(level0.len(), 1);
        assert_eq!(level0[0].stars.len(), 2);
    }

    #[test]
    fn a_deeper_level_splits_the_sky_into_four_times_as_many_tiles() {
        // Stars in opposite corners must land in different tiles at level 1.
        // `level0_stars` has to be below the star count, or the pyramid stops
        // at level 0 with everything already shown -- which is correct
        // behaviour and would hide what this test is about.
        let stars = sorted(vec![star(1, -9.0, -9.0, 1.0), star(2, 9.0, 9.0, 0.9)]);
        let bounds = Bounds::of(&[-10.0, 10.0], &[-10.0, 10.0]);
        let tiles = build(&stars, &bounds, &Plan { max_level: 1, level0_stars: 1 });

        let level1: Vec<&Tile> = tiles.iter().filter(|t| t.id.level == 1).collect();
        assert_eq!(level1.len(), 2, "corners should occupy two distinct tiles");
        assert_ne!(level1[0].id.col, level1[1].id.col);
    }

    #[test]
    fn brighter_stars_appear_at_shallower_levels() {
        // The pyramid is a popularity filter: level 0 shows only the
        // brightest, so a dim star must be absent there and present deeper.
        let stars = sorted(vec![star(1, 0.0, 0.0, 1.0), star(2, 1.0, 1.0, 0.5), star(3, 2.0, 2.0, 0.1)]);
        let bounds = Bounds::of(&[0.0, 2.0], &[0.0, 2.0]);
        let tiles = build(&stars, &bounds, &Plan { max_level: 2, level0_stars: 1 });

        let at = |level: u8| -> Vec<i32> {
            let mut ids: Vec<i32> = tiles
                .iter()
                .filter(|t| t.id.level == level)
                .flat_map(|t| t.stars.iter().map(|s| s.artist_id))
                .collect();
            ids.sort_unstable();
            ids
        };
        assert_eq!(at(0), vec![1], "level 0 admits only the brightest");
        assert_eq!(at(1), vec![1, 2, 3], "level 1 quadruples the budget");
    }

    #[test]
    fn a_star_present_at_one_level_stays_present_deeper() {
        // Otherwise stars would pop out of existence as the user zooms in,
        // which reads as a bug rather than as detail.
        let stars = sorted((0..40).map(|i| star(i, i as f32, 0.0, 1.0 - i as f32 * 0.01)).collect());
        let bounds = Bounds::of(&[0.0, 39.0], &[0.0, 0.0]);
        let tiles = build(&stars, &bounds, &Plan { max_level: 3, level0_stars: 2 });

        let ids_at = |level: u8| -> std::collections::HashSet<i32> {
            tiles
                .iter()
                .filter(|t| t.id.level == level)
                .flat_map(|t| t.stars.iter().map(|s| s.artist_id))
                .collect()
        };
        let shallow = ids_at(0);
        let deep = ids_at(2);
        assert!(shallow.is_subset(&deep), "stars vanished when zooming in");
    }

    #[test]
    fn the_pyramid_stops_once_every_star_is_shown() {
        // Deeper levels would only repeat the same stars in smaller squares.
        let stars = sorted(vec![star(1, 0.0, 0.0, 1.0), star(2, 1.0, 1.0, 0.5)]);
        let bounds = Bounds::of(&[0.0, 1.0], &[0.0, 1.0]);
        let tiles = build(
            &stars,
            &bounds,
            &Plan {
                max_level: 8,
                level0_stars: 100,
            },
        );

        let deepest = tiles.iter().map(|t| t.id.level).max().unwrap();
        assert_eq!(deepest, 0, "built {} levels for two stars", deepest + 1);
    }

    #[test]
    fn a_star_on_the_far_edge_lands_inside_the_grid() {
        // Floating point puts a star exactly on the boundary; without
        // clamping it would index one tile past the end.
        let bounds = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        };
        let id = bounds.tile_of(3, 10.0, 10.0);
        assert_eq!(id.col, 7);
        assert_eq!(id.row, 7);
    }

    #[test]
    fn the_written_tile_has_the_size_the_format_promises() {
        let tile = Tile {
            id: TileId { level: 2, col: 1, row: 3 },
            stars: vec![star(42, 1.5, -2.5, 0.75), star(43, 0.0, 0.0, 0.5)],
        };
        let mut out = Vec::new();
        write(&tile, &mut out).unwrap();
        assert_eq!(out.len(), HEADER + 2 * RECORD);
        assert_eq!(&out[..4], MAGIC);
    }

    #[test]
    fn a_written_tile_reads_back_exactly() {
        // The client parses these bytes; a field written in the wrong order
        // shows up as stars in the wrong place rather than as an error.
        let tile = Tile {
            id: TileId { level: 1, col: 0, row: 0 },
            stars: vec![star(-7, 1.25, -3.5, 0.5)],
        };
        let mut out = Vec::new();
        write(&tile, &mut out).unwrap();

        let count = u32::from_le_bytes(out[8..12].try_into().unwrap());
        assert_eq!(count, 1);
        let at = HEADER;
        assert_eq!(i32::from_le_bytes(out[at..at + 4].try_into().unwrap()), -7);
        assert_eq!(f32::from_le_bytes(out[at + 4..at + 8].try_into().unwrap()), 1.25);
        assert_eq!(f32::from_le_bytes(out[at + 8..at + 12].try_into().unwrap()), -3.5);
        assert_eq!(f32::from_le_bytes(out[at + 12..at + 16].try_into().unwrap()), 0.5);
    }

    #[test]
    fn tiles_come_out_in_a_stable_order() {
        // A tile set belongs to a versioned layout, so two runs over the same
        // input must produce the same files in the same order.
        let stars = sorted((0..50).map(|i| star(i, (i % 7) as f32, (i / 7) as f32, 1.0 - i as f32 * 0.01)).collect());
        let bounds = Bounds::of(&[0.0, 6.0], &[0.0, 7.0]);
        let plan = Plan { max_level: 2, level0_stars: 5 };

        let first: Vec<TileId> = build(&stars, &bounds, &plan).into_iter().map(|t| t.id).collect();
        let second: Vec<TileId> = build(&stars, &bounds, &plan).into_iter().map(|t| t.id).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn an_empty_sky_produces_no_tiles() {
        let bounds = Bounds::of(&[], &[]);
        assert!(build(&[], &bounds, &Plan::default()).is_empty());
    }

    #[test]
    fn the_path_is_level_column_row() {
        assert_eq!(path(TileId { level: 3, col: 5, row: 2 }), "3/5/2.bin");
    }
}
