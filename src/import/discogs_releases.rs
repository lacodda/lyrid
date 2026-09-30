//! Who released on which label, and when: one pass over the Discogs releases
//! file.
//!
//! Discogs links a label to an artist only through a release. The labels file
//! says what an imprint is and the masters file carries no labels at all, so a
//! label's roster -- and its history year by year -- can come from nowhere
//! else. The file is the largest in the dump (11.2 GB compressed for
//! 20260801), which is why the genre import never read it; see ADR 0017.
//!
//! What counts is decided here, once, and the tests exercise these functions
//! rather than a copy of the rules:
//!
//! - **Official releases only.** A bootleg names a label that never agreed to
//!   it; Discogs marks one with the format description "Unofficial Release".
//! - **No "Not On Label".** That is how Discogs says there was no label, in a
//!   family of per-artist entries ("Not On Label (Nirvana Self-released)"),
//!   and a roster of everyone who ever self-released is not a station.
//! - **Once per release.** A release that lists one label twice -- two
//!   catalogue numbers on one pressing -- is still one release on it.
//! - **Pressings count.** A label that issued an album in twelve countries did
//!   issue it twelve times, and Discogs's own label pages count the same way.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream element and file names throughout")]

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;

use super::discogs_xml::{Attributes, Record, Records};

/// One `<release>`, with only what the roster and the chronology need.
#[derive(Default)]
pub(super) struct Release {
    id: Option<i32>,
    /// False when the dump marks the release anything but accepted.
    accepted: bool,
    /// Discogs ids of the main credited artists -- not the track artists or
    /// the credits in `<extraartists>`, who did not release it.
    artists: Vec<i32>,
    /// `(id, name)` of each label printed on it.
    labels: Vec<(i32, String)>,
    released: Option<String>,
    unofficial: bool,
    title: Option<String>,
}

impl Record for Release {
    const ELEMENT: &'static str = "release";

    fn open(attributes: Attributes<'_>) -> Self {
        Self {
            id: attributes.parse("id"),
            // Absent in the 20260801 dump, where every record is accepted; a
            // dump that says otherwise is believed.
            accepted: attributes.get("status").is_none_or(|status| status == "Accepted"),
            ..Self::default()
        }
    }

    fn field(&mut self, path: &[&str], text: &str, attributes: Attributes<'_>) {
        match path {
            ["artists", "artist", "id"] => {
                if let Ok(id) = text.parse() {
                    self.artists.push(id);
                }
            }
            // `<label name="Svek" catno="SK032" id="5"/>`: everything is an
            // attribute, and the element has no text at all.
            ["labels", "label"] => {
                if let Some(id) = attributes.parse("id") {
                    self.labels.push((id, attributes.get("name").unwrap_or_default().to_string()));
                }
            }
            ["released"] if !text.is_empty() => self.released = Some(text.to_string()),
            ["formats", "format", "descriptions", "description"] if text == "Unofficial Release" => self.unofficial = true,
            ["title"] if !text.is_empty() => self.title = Some(text.to_string()),
            _ => {}
        }
    }
}

/// An artist's span on one label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Span {
    pub releases: i32,
    pub first: Option<i16>,
    pub last: Option<i16>,
}

impl Span {
    fn add(&mut self, year: Option<i16>) {
        self.releases += 1;
        self.widen(year, year);
    }

    /// Takes in another span's years, for when two Discogs entries are one
    /// canonical artist.
    pub(super) fn merge(&mut self, other: Self) {
        self.releases += other.releases;
        self.widen(other.first, other.last);
    }

    fn widen(&mut self, first: Option<i16>, last: Option<i16>) {
        if let Some(first) = first {
            self.first = Some(self.first.map_or(first, |held| held.min(first)));
        }
        if let Some(last) = last {
            self.last = Some(self.last.map_or(last, |held| held.max(last)));
        }
    }
}

/// What the pass yields.
#[derive(Default)]
pub(super) struct Rosters {
    /// `(label, Discogs artist)` -> the artist's span on the label, for the
    /// artists the canon links to.
    pub spans: HashMap<(i32, i32), Span>,
    /// `(label, year)` -> releases, over every official release.
    pub years: HashMap<(i32, i16), i32>,
    /// Titles of the releases a label profile points at by id.
    pub titles: HashMap<i32, String>,
}

/// Counters for the log line, so a run says what it skipped and why.
#[derive(Default, Debug)]
pub(super) struct Tally {
    pub releases: u64,
    pub unofficial: u64,
    pub not_accepted: u64,
    pub no_label: u64,
}

/// The year out of `released`, when it is one.
///
/// Discogs writes "1999-03-00", "1999" or nothing, and hand-typed values go
/// wrong in both directions. A year before recorded sound or after the dump
/// was made is a typo, and a typo in one release must not stretch a label's
/// history by a century.
pub(super) fn year_of(released: Option<&str>, latest: i16) -> Option<i16> {
    let released = released?;
    let digits = released.get(..4)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i16 = digits.parse().ok()?;
    (EARLIEST_YEAR..=latest).contains(&year).then_some(year)
}

/// The first year a release can honestly carry: the phonograph is 1877.
const EARLIEST_YEAR: i16 = 1877;

/// Whether a label entry is Discogs saying there was no label.
fn is_no_label(name: &str) -> bool {
    name.starts_with("Not On Label")
}

/// Adds one release to the rosters and the chronology.
///
/// `wanted` holds the Discogs artists the canon links to; `titled` the
/// releases some label profile names by id.
pub(super) fn tally(release: &Release, wanted: &HashSet<i32>, titled: &HashSet<i32>, latest: i16, rosters: &mut Rosters, tally: &mut Tally) {
    tally.releases += 1;
    if let (Some(id), Some(title)) = (release.id, &release.title)
        && titled.contains(&id)
    {
        rosters.titles.insert(id, title.clone());
    }
    if !release.accepted {
        tally.not_accepted += 1;
        return;
    }
    if release.unofficial {
        tally.unofficial += 1;
        return;
    }

    // Each label once, however many catalogue numbers carry it.
    let mut labels: Vec<i32> = release.labels.iter().filter(|(_, name)| !is_no_label(name)).map(|(id, _)| *id).collect();
    labels.sort_unstable();
    labels.dedup();
    if labels.is_empty() {
        tally.no_label += 1;
        return;
    }

    let year = year_of(release.released.as_deref(), latest);
    let mut artists: Vec<i32> = release.artists.iter().copied().filter(|id| wanted.contains(id)).collect();
    artists.sort_unstable();
    artists.dedup();

    for label in labels {
        if let Some(year) = year {
            *rosters.years.entry((label, year)).or_insert(0) += 1;
        }
        for artist in &artists {
            rosters.spans.entry((label, *artist)).or_default().add(year);
        }
    }
}

/// Reads the releases file.
pub(super) fn read(path: &Path, wanted: &HashSet<i32>, titled: &HashSet<i32>, latest: i16) -> Result<Rosters> {
    let mut records = Records::<_, Release>::new(super::discogs::open_dump(path)?);
    let mut rosters = Rosters::default();
    let mut counted = Tally::default();
    while let Some(release) = records.next_record()? {
        tally(&release, wanted, titled, latest, &mut rosters, &mut counted);
        if counted.releases.is_multiple_of(2_000_000) {
            tracing::info!(releases = counted.releases, pairs = rosters.spans.len(), "still reading releases");
        }
    }
    tracing::info!(
        releases = counted.releases,
        unofficial = counted.unofficial,
        not_accepted = counted.not_accepted,
        without_label = counted.no_label,
        roster_pairs = rosters.spans.len(),
        label_years = rosters.years.len(),
        "releases file read"
    );
    Ok(rosters)
}

/// Turns per-Discogs-artist spans into `label_artist` rows.
///
/// Merged before insertion, as genres are: a canonical artist can carry
/// several Discogs links, so one `(label, artist)` pair can arrive twice, and
/// Postgres refuses to touch one row twice in a statement. Labels the labels
/// file does not hold are dropped rather than left to fail the foreign key.
pub(super) fn roster_rows(spans: &HashMap<(i32, i32), Span>, mapping: &HashMap<i32, Vec<i32>>, labels: &HashSet<i32>) -> Vec<(i32, i32, Span)> {
    let mut merged: HashMap<(i32, i32), Span> = HashMap::new();
    for ((label, discogs_id), span) in spans {
        if !labels.contains(label) {
            continue;
        }
        let Some(artists) = mapping.get(discogs_id) else {
            continue;
        };
        for artist in artists {
            merged.entry((*label, *artist)).or_default().merge(*span);
        }
    }
    // Sorted so a re-import writes the same rows in the same order.
    let mut rows: Vec<(i32, i32, Span)> = merged.into_iter().map(|((label, artist), span)| (label, artist, span)).collect();
    rows.sort_unstable_by_key(|(label, artist, _)| (*label, *artist));
    rows
}

/// The chronology rows worth keeping: those of labels with a roster.
pub(super) fn year_rows(years: &HashMap<(i32, i16), i32>, stations: &HashSet<i32>) -> Vec<(i32, i16, i32)> {
    let mut rows: Vec<(i32, i16, i32)> = years
        .iter()
        .filter(|((label, _), _)| stations.contains(label))
        .map(|((label, year), releases)| (*label, *year, *releases))
        .collect();
    rows.sort_unstable();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: i16 = 2027;

    fn read_all(xml: &str, wanted: &[i32]) -> (Rosters, Tally) {
        let wanted: HashSet<i32> = wanted.iter().copied().collect();
        let mut records = Records::<_, Release>::new(xml.as_bytes());
        let mut rosters = Rosters::default();
        let mut counted = Tally::default();
        while let Some(release) = records.next_record().unwrap() {
            tally(&release, &wanted, &HashSet::from([2]), LATEST, &mut rosters, &mut counted);
        }
        (rosters, counted)
    }

    #[test]
    fn a_release_puts_its_main_artist_on_its_label_in_its_year() {
        let xml = concat!(
            "<releases><release id=\"1\"><artists><artist><id>1</id><name>The Persuader</name></artist></artists>",
            "<title>Stockholm</title><labels><label name=\"Svek\" catno=\"SK032\" id=\"5\"/></labels>",
            "<extraartists><artist><id>239</id><name>Jesper</name><role>Written-By</role></artist></extraartists>",
            "<released>1999-03-00</released></release></releases>"
        );
        let (rosters, _) = read_all(xml, &[1, 239]);
        assert_eq!(
            rosters.spans.get(&(5, 1)),
            Some(&Span {
                releases: 1,
                first: Some(1999),
                last: Some(1999)
            })
        );
        // A writing credit is not a release on the label.
        assert!(!rosters.spans.contains_key(&(5, 239)));
        assert_eq!(rosters.years.get(&(5, 1999)), Some(&1));
    }

    #[test]
    fn two_catalogue_numbers_on_one_label_are_one_release() {
        let xml = concat!(
            "<releases><release id=\"2\"><artists><artist><id>2</id></artist></artists>",
            "<labels><label name=\"Svek\" catno=\"SK 026\" id=\"5\"/><label name=\"Svek\" catno=\"SK026\" id=\"5\"/></labels>",
            "<released>1998</released></release></releases>"
        );
        let (rosters, _) = read_all(xml, &[2]);
        assert_eq!(rosters.spans[&(5, 2)].releases, 1);
        assert_eq!(rosters.years[&(5, 1998)], 1);
    }

    #[test]
    fn a_bootleg_and_no_label_count_for_nothing() {
        let xml = concat!(
            "<releases>",
            "<release id=\"3\"><artists><artist><id>7</id></artist></artists><labels><label name=\"Pirate\" id=\"9\"/></labels>",
            "<formats><format name=\"CD\" qty=\"1\"><descriptions><description>Unofficial Release</description></descriptions></format></formats>",
            "<released>2001</released></release>",
            "<release id=\"4\"><artists><artist><id>7</id></artist></artists>",
            "<labels><label name=\"Not On Label (Seven Self-released)\" id=\"12\"/></labels><released>2002</released></release>",
            "</releases>"
        );
        let (rosters, counted) = read_all(xml, &[7]);
        assert!(rosters.spans.is_empty(), "{:?}", rosters.spans);
        assert!(rosters.years.is_empty());
        assert_eq!((counted.unofficial, counted.no_label), (1, 1));
    }

    #[test]
    fn a_release_that_is_not_accepted_is_not_counted() {
        let xml = "<releases><release id=\"5\" status=\"Draft\"><artists><artist><id>7</id></artist></artists><labels><label name=\"L\" id=\"9\"/></labels></release></releases>";
        let (rosters, counted) = read_all(xml, &[7]);
        assert!(rosters.spans.is_empty());
        assert_eq!(counted.not_accepted, 1);
    }

    #[test]
    fn the_chronology_counts_artists_the_canon_does_not_hold() {
        // The label's history is the label's, not the canon's.
        let xml = "<releases><release id=\"6\"><artists><artist><id>999</id></artist></artists><labels><label name=\"L\" id=\"9\"/></labels><released>1985-01-01</released></release></releases>";
        let (rosters, _) = read_all(xml, &[7]);
        assert!(rosters.spans.is_empty());
        assert_eq!(rosters.years[&(9, 1985)], 1);
    }

    #[test]
    fn an_undated_release_is_on_the_roster_and_off_the_timeline() {
        let xml =
            "<releases><release id=\"8\"><artists><artist><id>7</id></artist></artists><labels><label name=\"L\" id=\"9\"/></labels></release></releases>";
        let (rosters, _) = read_all(xml, &[7]);
        assert_eq!(
            rosters.spans[&(9, 7)],
            Span {
                releases: 1,
                first: None,
                last: None
            }
        );
        assert!(rosters.years.is_empty());
    }

    #[test]
    fn keeps_the_title_only_of_a_release_a_profile_names() {
        let xml = concat!(
            "<releases><release id=\"2\"><title>Knockin' Boots</title></release>",
            "<release id=\"3\"><title>Other</title></release></releases>"
        );
        let (rosters, _) = read_all(xml, &[]);
        assert_eq!(rosters.titles, HashMap::from([(2, "Knockin' Boots".to_string())]));
    }

    #[test]
    fn reads_a_year_only_where_there_is_one() {
        assert_eq!(year_of(Some("1999-03-00"), LATEST), Some(1999));
        assert_eq!(year_of(Some("1999"), LATEST), Some(1999));
        assert_eq!(year_of(Some("0000"), LATEST), None);
        assert_eq!(year_of(Some("2091-01-01"), LATEST), None, "past the dump is a typo");
        assert_eq!(year_of(Some("?"), LATEST), None);
        assert_eq!(year_of(Some("19"), LATEST), None);
        assert_eq!(year_of(None, LATEST), None);
    }

    #[test]
    fn spans_merge_when_two_discogs_entries_are_one_artist() {
        let spans = HashMap::from([
            (
                (5, 700),
                Span {
                    releases: 3,
                    first: Some(1990),
                    last: Some(1994),
                },
            ),
            (
                (5, 701),
                Span {
                    releases: 2,
                    first: Some(1988),
                    last: None,
                },
            ),
        ]);
        let mapping = HashMap::from([(700, vec![42]), (701, vec![42])]);
        let rows = roster_rows(&spans, &mapping, &HashSet::from([5]));
        assert_eq!(
            rows,
            vec![(
                5,
                42,
                Span {
                    releases: 5,
                    first: Some(1988),
                    last: Some(1994)
                }
            )]
        );
    }

    #[test]
    fn a_label_the_labels_file_does_not_hold_is_dropped() {
        let spans = HashMap::from([((6, 700), Span::default())]);
        let mapping = HashMap::from([(700, vec![42])]);
        assert!(roster_rows(&spans, &mapping, &HashSet::from([5])).is_empty());
    }

    #[test]
    fn keeps_the_chronology_of_stations_only() {
        let years = HashMap::from([((5, 1990), 2), ((6, 1990), 9)]);
        assert_eq!(year_rows(&years, &HashSet::from([5])), vec![(5, 1990, 2)]);
    }
}
