//! A star's spectrum: the bands it is drawn in, and a star's place on each.
//!
//! Shared by the importer, which writes each band's percentiles into
//! `spectrum_scale`, and by the card, which reads a star's place off them --
//! so the list of bands exists once, and the two can never disagree about
//! which ones there are.

/// The bands, in the order a card draws them. Each name is a column of
/// `artist_spectrum` and a row of `spectrum_scale`; the migration's check
/// constraint holds the same list, and a test reads it from there.
pub const BANDS: [&str; 6] = ["tempo", "energy", "mood", "danceable", "sound", "voice"];

/// How many percentiles a band's scale holds: the 0th to the 100th.
pub const QUANTILES: usize = 101;

/// Where `value` sits among the measured stars, from 0.0 (below all of them)
/// to 1.0 (above all of them), read off a band's percentiles.
///
/// Interpolated between the two percentiles either side, so two stars a hair
/// apart are drawn a hair apart rather than snapping to the same step.
/// `None` when the scale is not one -- fewer than two points, or not in
/// order -- or the value is not a number: a place made up from a broken scale
/// is worse than no place.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a scale has a hundred and one points, far inside what f32 counts exactly")]
pub fn rank(value: f32, quantiles: &[f32]) -> Option<f32> {
    if !value.is_finite() || quantiles.len() < 2 || quantiles.iter().any(|q| !q.is_finite()) || quantiles.windows(2).any(|pair| pair[1] < pair[0]) {
        return None;
    }
    let steps = (quantiles.len() - 1) as f32;

    // The first percentile above the value. On a plateau of equal
    // percentiles the value is placed at its top: "at least as high as this
    // many stars" is the claim a percentile makes.
    let above = quantiles.partition_point(|&q| q <= value);
    if above == 0 {
        return Some(0.0);
    }
    if above == quantiles.len() {
        return Some(1.0);
    }

    let (low, high) = (quantiles[above - 1], quantiles[above]);
    // `high > value >= low`, so the span is never zero.
    let within = (value - low) / (high - low);
    Some(((above - 1) as f32 + within) / steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0, 1, 2 ... 100: a scale on which a value is its own percentile.
    fn identity() -> Vec<f32> {
        (0u8..=100).map(f32::from).collect()
    }

    #[test]
    fn a_value_on_a_percentile_ranks_at_that_percentile() {
        assert_eq!(rank(25.0, &identity()), Some(0.25));
        assert_eq!(rank(80.0, &identity()), Some(0.8));
    }

    #[test]
    fn a_value_between_two_percentiles_is_placed_between_them() {
        let place = rank(25.5, &identity()).unwrap();
        assert!((place - 0.255).abs() < 1e-6, "got {place}");
    }

    #[test]
    fn values_beyond_the_scale_pin_to_its_ends() {
        assert_eq!(rank(-5.0, &identity()), Some(0.0));
        assert_eq!(rank(100.0, &identity()), Some(1.0));
        assert_eq!(rank(500.0, &identity()), Some(1.0));
    }

    #[test]
    fn a_plateau_places_the_value_at_its_top() {
        // Percentiles 0..=2 are all 1.0: a third of a tiny scale has the same
        // value, and a star at that value is at least as high as all of them.
        let scale = [1.0, 1.0, 1.0, 2.0, 3.0];
        assert_eq!(rank(1.0, &scale), Some(0.5));
    }

    #[test]
    fn a_broken_scale_gives_no_place_rather_than_a_wrong_one() {
        assert_eq!(rank(1.0, &[]), None);
        assert_eq!(rank(1.0, &[1.0]), None);
        assert_eq!(rank(1.0, &[2.0, 1.0, 3.0]), None, "out of order");
        assert_eq!(rank(f32::NAN, &identity()), None);
    }

    #[test]
    fn the_bands_are_the_ones_the_schema_allows() {
        // The migration lists the bands in a check constraint; the list here
        // is what the importer writes and the card reads. Reading the
        // migration keeps the two from drifting apart.
        let migration = include_str!("../migrations/0012_spectrum.sql");
        let constraint = migration
            .lines()
            .find(|line| line.contains("spectrum_scale_band_known"))
            .expect("the scale's band constraint");
        let listed: Vec<&str> = constraint.split('\'').skip(1).step_by(2).collect();
        assert_eq!(listed, BANDS);
    }
}
