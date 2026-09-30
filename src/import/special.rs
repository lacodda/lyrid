//! MusicBrainz's special purpose artists: credits that are not acts.
//!
//! "Various Artists" is credited on every compilation, "[unknown]" wherever
//! nobody knows, "[traditional]" on folk songs. MusicBrainz keeps them as
//! artists so every release has someone to credit, and documents them as a
//! fixed set: <https://musicbrainz.org/doc/Style/Unknown_and_untitled/Special_purpose_artist>.
//!
//! They carry the other databases' equivalents with them -- MusicBrainz links
//! "Various Artists" to Discogs's "Various" (id 194) and "[unknown]" to
//! "Unknown Artist" (355) -- so a join through those links would put every
//! compilation label's catalogue on one star and give it the genres of all of
//! them. The set is written out here, by MBID, rather than guessed from the
//! names: the canon holds real acts called "[Insert Gore]" and "[cruz]".
#![allow(clippy::doc_markdown, reason = "documentation names upstream databases and their entries throughout")]

/// The MBIDs of the special purpose artists, as MusicBrainz documents them.
pub const SPECIAL_PURPOSE: [&str; 12] = [
    // Various Artists
    "89ad4ac3-39f7-470e-963a-56509c546377",
    // [anonymous]
    "f731ccc4-e22a-43af-a747-64213329e088",
    // [data]
    "33cf029c-63b0-41a0-9855-be2a3665fb3b",
    // [dialogue]
    "314e1c25-dde7-4e4d-b2f4-0a7b9f7c56dc",
    // [no artist]
    "eec63d3c-3b81-4ad4-b1e4-7c147d4d2b61",
    // [traditional]
    "9be7f096-97ec-4615-8957-8d40b5dcbc41",
    // [unknown]
    "125ec42a-7229-4250-afc5-e057484327fe",
    // MusicBrainz Test Artist
    "7e84f845-ac16-41fe-9ff8-df12eb32af55",
    // [Disney]
    "66ea0139-149f-4a0c-8fbf-5ea9ec4a6e49",
    // [theatre]
    "a0ef7e1d-44ff-4039-9435-7d5fefdeecc9",
    // [church chimes]
    "90068d37-bae7-4292-be4a-704c145bd616",
    // [language instruction]
    "80a8851f-444c-4539-892b-ad2a49292aa9",
];

/// The set as UUIDs, for binding into a query.
pub fn special_purpose() -> Vec<uuid::Uuid> {
    SPECIAL_PURPOSE
        .iter()
        .map(|mbid| uuid::Uuid::parse_str(mbid).expect("the special purpose MBIDs are valid UUIDs"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_special_purpose_mbid_parses_and_none_repeats() {
        let parsed = special_purpose();
        let unique: std::collections::HashSet<_> = parsed.iter().collect();
        assert_eq!(unique.len(), SPECIAL_PURPOSE.len());
    }
}
