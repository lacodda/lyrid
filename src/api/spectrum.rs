//! A star's spectrum, as the card draws it.
//!
//! What `AcousticBrainz` measured in the star's recordings, said as a place
//! among all measured stars rather than as the numbers the models gave: the
//! models are poorly calibrated, so "relaxed 0.81" is the commonest reading
//! there is and means nothing, while "calmer than four stars in five" is the
//! answer to the question a reader is asking. Tempo is the one band that also
//! carries its own number, because beats per minute mean something by
//! themselves.

use serde::Serialize;
use sqlx::{AssertSqlSafe, PgPool, Row};

use crate::spectrum::{BANDS, rank};

/// What the card shows under "spectrum".
#[derive(Serialize, Debug, PartialEq)]
pub struct Spectrum {
    /// How many recordings every band is measured on.
    pub recordings: i32,
    /// Beats per minute: the median of the recordings' tempos.
    pub bpm: f32,
    /// The star's place on each band, in the order a card draws them.
    pub bands: Vec<Band>,
}

/// One band: its name, and where the star stands on it among all the stars
/// `AcousticBrainz` measured, from 0 (below every one of them) to 1.
#[derive(Serialize, Debug, PartialEq)]
pub struct Band {
    pub name: &'static str,
    pub rank: f32,
}

/// The spectrum of one star, or `None` when it was never measured.
///
/// Also `None`, with a warning, when the star has a row but the scale it is
/// read against is missing a band: a spectrum drawn against half a scale
/// would place the star on some bands and not others with no way to tell.
/// That happens only when the two tables were carried to a stand separately.
pub async fn of(pool: &PgPool, id: i32) -> sqlx::Result<Option<Spectrum>> {
    // The columns are named by the shared list, so a band added there is read
    // here without this query being edited.
    let sql = format!("SELECT recordings, {} FROM artist_spectrum WHERE artist_id = $1", BANDS.join(", "));
    let Some(row) = sqlx::query(AssertSqlSafe(sql)).bind(id).fetch_optional(pool).await? else {
        return Ok(None);
    };

    let scales: Vec<(String, Vec<f32>)> = sqlx::query_as("SELECT band, quantiles FROM spectrum_scale").fetch_all(pool).await?;

    let recordings: i32 = row.try_get(0)?;
    let mut values = Vec::with_capacity(BANDS.len());
    for index in 0..BANDS.len() {
        values.push(row.try_get::<f32, _>(index + 1)?);
    }

    let spectrum = place(recordings, &values, &scales);
    if spectrum.is_none() {
        tracing::warn!(
            artist = id,
            "a measured star has no complete scale to be placed on: was spectrum_scale carried with artist_spectrum?"
        );
    }
    Ok(spectrum)
}

/// Places a star's band values, given in `BANDS` order, on the scales.
fn place(recordings: i32, values: &[f32], scales: &[(String, Vec<f32>)]) -> Option<Spectrum> {
    let bpm = *values.first()?;
    let bands = BANDS
        .iter()
        .zip(values)
        .map(|(&name, &value)| {
            let (_, quantiles) = scales.iter().find(|(band, _)| band == name)?;
            Some(Band {
                name,
                rank: rank(value, quantiles)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Spectrum { recordings, bpm, bands })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Vec<f32> {
        (0u8..=100).map(|n| f32::from(n) / 100.0).collect()
    }

    fn scales() -> Vec<(String, Vec<f32>)> {
        BANDS
            .iter()
            .map(|&band| {
                // Tempo runs 60..160 BPM on this scale, the rest 0..1.
                let quantiles = if band == "tempo" {
                    (0u8..=100).map(|n| 60.0 + f32::from(n)).collect()
                } else {
                    identity()
                };
                (band.to_string(), quantiles)
            })
            .collect()
    }

    #[test]
    fn every_band_is_placed_in_the_drawing_order() {
        let spectrum = place(12, &[110.0, 0.9, 0.2, 0.5, 0.0, 1.0], &scales()).expect("a full scale");
        assert_eq!(spectrum.recordings, 12);
        assert_eq!(spectrum.bpm, 110.0, "tempo keeps its own number");
        let names: Vec<&str> = spectrum.bands.iter().map(|b| b.name).collect();
        assert_eq!(names, BANDS);
        let ranks: Vec<f32> = spectrum.bands.iter().map(|b| (b.rank * 100.0).round() / 100.0).collect();
        assert_eq!(ranks, vec![0.5, 0.9, 0.2, 0.5, 0.0, 1.0]);
    }

    #[test]
    fn a_scale_missing_a_band_places_nothing() {
        let mut partial = scales();
        partial.retain(|(band, _)| band != "mood");
        assert_eq!(place(12, &[110.0, 0.9, 0.2, 0.5, 0.0, 1.0], &partial), None);
    }
}
