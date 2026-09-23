//! Names on the sky: where to write "Techno" so it lands on techno.
//!
//! A genre is not a point. Its artists spread across the layout wherever
//! co-listening pulled them, usually as one dense core and a long scatter of
//! crossover acts. A label at the *mean* of all of them lands in the middle of
//! that scatter -- often in empty space between two clusters, naming nothing.
//!
//! So a label is anchored at the genre's **densest place** instead: the sky is
//! cut into a grid, the cell holding most of the genre's artists wins, and the
//! label sits at the mean of the artists in that cell and its eight
//! neighbours. The mean is taken over the block rather than the one cell so the
//! anchor follows the stars inside it and not the grid's arbitrary lines.
//!
//! This runs where the tiles are cut and is written beside them as a static
//! file: the map never asks the database anything, and names are part of the
//! map.

use serde::Serialize;

use super::tiles::Bounds;

/// Which layer a name belongs to.
///
/// Discogs has two vocabularies, and they suit two distances. Its fifteen
/// genres ("Electronic", "Jazz") are continents, legible from the whole sky;
/// its styles ("Techno", "Bossa Nova") are the constellations inside them and
/// only make sense once the view has closed in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Genre,
    Style,
}

/// A name and the place to write it.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct Label {
    pub name: String,
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    /// How many placed artists carry this as their main genre or style. The
    /// client ranks by it when two names would overlap.
    pub members: u32,
    /// How many times denser the genre is in the block its label sits on than
    /// it would be spread evenly over the sky. Near 1 means scattered: a name
    /// with no constellation under it.
    pub lift: f32,
}

/// How finely each layer's grid cuts the sky.
///
/// Coarse for genres: a continent's densest region should be a region, not the
/// one tight clump of a single scene inside it. Finer for styles, which are
/// themselves small.
fn grid_for(kind: Kind) -> u32 {
    match kind {
        Kind::Genre => 12,
        Kind::Style => 40,
    }
}

/// A name, its layer, and the positions of the artists that carry it.
pub type Group = (String, Kind, Vec<(f32, f32)>);

/// Anchors one name among its artists, or `None` when there are too few of
/// them to call a constellation.
#[must_use]
pub fn anchor(name: &str, kind: Kind, members: &[(f32, f32)], bounds: &Bounds, min_members: usize) -> Option<Label> {
    if members.len() < min_members.max(1) {
        return None;
    }
    let grid = grid_for(kind);
    // The same arithmetic as a tile lookup, on a grid that is not a power of
    // two, and with the same clamping for a star on the far edge.
    let cell_of = |x: f32, y: f32| {
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the grid is at most a few dozen cells a side; values are clamped in range"
        )]
        {
            let side = grid as f32;
            let span = (bounds.max_x - bounds.min_x).max(f32::MIN_POSITIVE);
            let col = ((x - bounds.min_x) / span * side).floor().clamp(0.0, side - 1.0) as u32;
            let row = ((y - bounds.min_y) / span * side).floor().clamp(0.0, side - 1.0) as u32;
            (col, row)
        }
    };

    let mut counts = std::collections::HashMap::<(u32, u32), u32>::new();
    for &(x, y) in members {
        *counts.entry(cell_of(x, y)).or_default() += 1;
    }
    // Ties broken by position so two runs over the same sky write the same
    // file: a label set belongs to a versioned layout, like the tiles do.
    let (&(col, row), _) = counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))?;

    let near = |(c, r): (u32, u32)| c.abs_diff(col) <= 1 && r.abs_diff(row) <= 1;
    let (mut sum_x, mut sum_y, mut n) = (0f64, 0f64, 0u32);
    for &(x, y) in members {
        if near(cell_of(x, y)) {
            sum_x += f64::from(x);
            sum_y += f64::from(y);
            n += 1;
        }
    }
    let total = u32::try_from(members.len()).unwrap_or(u32::MAX);
    #[expect(clippy::cast_possible_truncation, reason = "a mean of f32 coordinates fits back into an f32")]
    #[expect(clippy::cast_precision_loss, reason = "member counts are far below f32's exact range")]
    Some(Label {
        name: name.to_string(),
        kind,
        x: (sum_x / f64::from(n)) as f32,
        y: (sum_y / f64::from(n)) as f32,
        members: total,
        // The block is 3x3 cells of a grid x grid sky, so an even spread
        // would put 9 / grid^2 of the members in it.
        lift: (n as f32 / total as f32) / (9.0 / (grid * grid) as f32),
    })
}

/// Every name worth writing, largest first.
///
/// `groups` is each name with the positions of the artists that carry it as
/// their main genre or style. Styles are capped: the canon has several hundred,
/// and the long tail are scenes of a dozen acts that would crowd out the names
/// people know.
///
/// A style is also dropped when it gathers nowhere -- when even its densest
/// block holds it at under `min_lift` times an even spread. Measured on the
/// slice: Soul and Pop Rock sit about thirty times denser in their block than
/// evenly, House about twelve, while Techno is four -- scattered across the
/// layout with no constellation to name. Writing "Techno" over a patch of sky
/// that is not techno would be the map telling a lie. Genres are continents
/// and are always written; they are wide by nature.
#[must_use]
pub fn labels(groups: &[Group], bounds: &Bounds, min_members: usize, max_styles: usize, min_lift: f32) -> Vec<Label> {
    let mut out: Vec<Label> = groups
        .iter()
        .filter_map(|(name, kind, members)| anchor(name, *kind, members, bounds, min_members))
        .filter(|label| label.kind == Kind::Genre || label.lift >= min_lift)
        .collect();
    out.sort_by(|a, b| b.members.cmp(&a.members).then_with(|| a.name.cmp(&b.name)));

    let mut styles = 0usize;
    out.retain(|label| {
        if label.kind == Kind::Genre {
            return true;
        }
        styles += 1;
        styles <= max_styles
    });
    out
}

#[cfg(test)]
#[expect(clippy::cast_precision_loss, reason = "test fixtures build coordinates from small loop counters")]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            min_x: -100.0,
            min_y: -100.0,
            max_x: 100.0,
            max_y: 100.0,
        }
    }

    #[test]
    fn a_label_lands_on_the_dense_core_not_on_the_mean() {
        // Forty artists packed near (60, 60) and ten scattered at the far
        // corner. The mean of all fifty is about (36, 36) -- open sky between
        // the two -- and a label there would name nothing at all.
        let mut members: Vec<(f32, f32)> = (0..40).map(|i| (60.0 + (i % 5) as f32, 60.0 + (i / 5) as f32 * 0.5)).collect();
        members.extend((0..10).map(|i| (-80.0 + i as f32, -80.0)));

        let label = anchor("Techno", Kind::Style, &members, &bounds(), 5).expect("enough members");
        assert!(
            (label.x - 62.0).abs() < 3.0 && (label.y - 61.0).abs() < 3.0,
            "anchored at {}, {}",
            label.x,
            label.y
        );
        assert_eq!(label.members, 50, "the count is every member, not only the core");
    }

    #[test]
    fn too_few_members_is_no_constellation() {
        let members = vec![(0.0, 0.0), (1.0, 1.0)];
        assert_eq!(anchor("Nobody", Kind::Style, &members, &bounds(), 3), None);
        assert!(anchor("Somebody", Kind::Style, &members, &bounds(), 2).is_some());
    }

    #[test]
    fn styles_are_capped_and_genres_never_are() {
        let group = |name: &str, kind: Kind, n: usize| (name.to_string(), kind, vec![(0.0, 0.0); n]);
        let groups = vec![
            group("Rock", Kind::Genre, 5),
            group("Jazz", Kind::Genre, 4),
            group("Techno", Kind::Style, 9),
            group("House", Kind::Style, 8),
            group("Dub", Kind::Style, 7),
        ];
        let out = labels(&groups, &bounds(), 1, 2, 0.0);
        let names: Vec<&str> = out.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["Techno", "House", "Rock", "Jazz"], "the smallest style goes, both genres stay");
    }

    #[test]
    fn a_scattered_style_gets_no_name_and_a_gathered_one_does() {
        // Eighty members spread evenly over the sky, and eighty packed in one
        // corner. Only the second is a constellation.
        let spread: Vec<(f32, f32)> = (0..80).map(|i| (-95.0 + (i % 9) as f32 * 23.0, -95.0 + (i / 9) as f32 * 23.0)).collect();
        let packed: Vec<(f32, f32)> = (0..80).map(|i| (50.0 + (i % 9) as f32 * 0.3, 50.0 + (i / 9) as f32 * 0.3)).collect();
        let groups = vec![
            ("Scattered".to_string(), Kind::Style, spread.clone()),
            ("Gathered".to_string(), Kind::Style, packed),
            ("Continent".to_string(), Kind::Genre, spread),
        ];
        let out = labels(&groups, &bounds(), 1, 10, 8.0);
        let names: Vec<&str> = out.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Continent", "Gathered"],
            "a scattered genre keeps its name, a scattered style does not"
        );
    }

    #[test]
    fn two_runs_write_the_same_names_in_the_same_places() {
        // Two cells with the same count: the tie must break the same way on
        // every run, or rebuilding the tiles would move labels about.
        let members = vec![(-50.0, -50.0), (-50.5, -50.5), (50.0, 50.0), (50.5, 50.5)];
        let first = anchor("Tie", Kind::Style, &members, &bounds(), 1);
        for _ in 0..20 {
            assert_eq!(anchor("Tie", Kind::Style, &members, &bounds(), 1), first);
        }
    }

    #[test]
    fn the_file_names_its_layers_in_lowercase() {
        let label = Label {
            name: "Jazz".into(),
            kind: Kind::Genre,
            x: 1.0,
            y: 2.0,
            members: 3,
            lift: 1.0,
        };
        let json = serde_json::to_string(&label).unwrap();
        assert!(json.contains(r#""kind":"genre""#), "{json}");
    }
}
