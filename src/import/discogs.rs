//! Imports genres, styles and labels from the Discogs monthly XML dumps.
//!
//! Three files, each one gzipped XML document, read in one streaming pass
//! apiece:
//!
//! - `artists` — only to learn which Discogs ids exist, so a genre can never
//!   be attached through a dangling reference.
//! - `labels` — the stations of the map: imprints, their descriptions, and
//!   which imprint owns which.
//! - `masters` — where the genres actually live. Discogs puts genre and style
//!   on releases, never on artists, so an artist's genres are aggregated from
//!   their discography.
//!
//! The 10 GB `releases` file is deliberately not read: a master release
//! carries the same genres as its pressings, and the pressings add ten
//! gigabytes to say it again.
//!
//! **Why Discogs and not MusicBrainz for genres.** MusicBrainz models genre as
//! a folksonomy tag, and its tag tables ship in `mbdump-derived` under
//! CC BY-NC-SA — a non-commercial licence that travels to everything computed
//! from it, which is why MLHD+ was already turned down for brightness (ADR
//! 0004). The Discogs dumps are CC0, so the canon stays public domain end to
//! end. See ADR 0005.
//!
//! **How an artist is joined to their Discogs entry.** Through MusicBrainz's
//! own `discogs` artist-URL relationship, which the MusicBrainz import already
//! stores in `artist_url`. Not by name: names collide constantly — Discogs
//! disambiguates with a numeric suffix ("Jack Jones (4)") precisely because
//! they do — and a wrong join would put someone else's genres on a star.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream element and file names throughout")]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use flate2::read::MultiGzDecoder;
use sqlx::PgPool;

use super::discogs_xml::{Attributes, Record, Records};

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to discogs_<date>_masters.xml.gz, which carries the genres.
    #[arg(long, value_name = "FILE")]
    pub masters: PathBuf,

    /// Path to discogs_<date>_labels.xml.gz. Optional: without it the labels
    /// table is left as it was, and genres still import.
    #[arg(long, value_name = "FILE")]
    pub labels: Option<PathBuf>,

    /// Path to discogs_<date>_artists.xml.gz. Optional, and only used to check
    /// that the ids the masters credit actually exist.
    #[arg(long, value_name = "FILE")]
    pub artists: Option<PathBuf>,

    /// The dump version. Defaults to the date in the masters filename
    /// (discogs_20260801_masters.xml.gz -> 20260801).
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,
}

/// One `<master>`: the genres of one release, and who it is credited to.
#[derive(Default)]
struct Master {
    /// Discogs ids of the credited artists.
    artists: Vec<i32>,
    genres: Vec<String>,
    styles: Vec<String>,
}

impl Record for Master {
    const ELEMENT: &'static str = "master";

    fn open(_: Attributes<'_>) -> Self {
        // The master's own id is not needed: nothing points back at a master,
        // and the genres are counted per artist.
        Self::default()
    }

    fn field(&mut self, path: &[&str], text: &str, _: Attributes<'_>) {
        match path {
            // `<artists><artist><id>` — the credited artist. Not `<name>`:
            // the id is the join key, and `<anv>` next to it is a per-release
            // spelling that must not be mistaken for either.
            ["artists", "artist", "id"] => {
                if let Ok(id) = text.parse() {
                    self.artists.push(id);
                }
            }
            ["genres", "genre"] if !text.is_empty() => self.genres.push(text.to_string()),
            ["styles", "style"] if !text.is_empty() => self.styles.push(text.to_string()),
            _ => {}
        }
    }
}

/// One `<label>`.
#[derive(Default)]
struct Label {
    id: Option<i32>,
    name: Option<String>,
    profile: Option<String>,
    parent_id: Option<i32>,
}

impl Record for Label {
    const ELEMENT: &'static str = "label";

    fn open(_: Attributes<'_>) -> Self {
        Self::default()
    }

    fn field(&mut self, path: &[&str], text: &str, attributes: Attributes<'_>) {
        match path {
            ["id"] => self.id = text.parse().ok(),
            ["name"] => self.name = Some(text.to_string()),
            ["profile"] if !text.is_empty() => self.profile = Some(text.to_string()),
            // camelCase, alone among these element names; the id is on the
            // attribute while the text is the parent's name.
            ["parentLabel"] => self.parent_id = attributes.parse("id"),
            // `<sublabels><label>` names the other direction of the same
            // fact and is skipped: storing both invites them to disagree.
            _ => {}
        }
    }
}

/// One `<artist>`, read only for its id.
struct DiscogsArtist {
    id: Option<i32>,
}

impl Record for DiscogsArtist {
    const ELEMENT: &'static str = "artist";

    fn open(_: Attributes<'_>) -> Self {
        Self { id: None }
    }

    fn field(&mut self, path: &[&str], text: &str, _: Attributes<'_>) {
        // Only the record's own id, at depth one. `["members", "id"]` and
        // `["aliases", "name"]` are other artists.
        if path == ["id"] {
            self.id = text.parse().ok();
        }
    }
}

/// What one pass over the masters file yields, per artist.
type GenreCounts = HashMap<i32, HashMap<(String, bool), i32>>;

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    // The join comes from the MusicBrainz import; without it there is nothing
    // to attach genres to.
    let mapping = load_discogs_ids(pool).await?;
    if mapping.is_empty() {
        bail!(
            "no artist has a Discogs link in the canon: run `lyrid import musicbrainz` first \
             (the link comes from MusicBrainz's `discogs` artist-URL relationship)"
        );
    }
    tracing::info!(linked = mapping.len(), "resolving Discogs data against the canon");

    let version = args
        .version
        .clone()
        .or_else(|| version_from_filename(&args.masters))
        .context("cannot tell the dump version from the filename; pass --dump-version")?;

    // Only the ids that some canonical artist actually points at are worth
    // counting; the rest of Discogs is millions of artists this sky has never
    // heard of.
    let wanted: HashSet<i32> = mapping.keys().copied().collect();

    let known_artists = match &args.artists {
        Some(path) => Some(read_artist_ids(path, &wanted)?),
        None => None,
    };

    let counts = read_masters(&args.masters, &wanted, known_artists.as_ref())?;
    let labels = match &args.labels {
        Some(path) => read_labels(path)?,
        None => Vec::new(),
    };

    write(pool, &mapping, &counts, &labels, &version).await
}

/// Which Discogs id each canonical artist points at, taken from the URL
/// relationships the MusicBrainz import stored.
async fn load_discogs_ids(pool: &PgPool) -> Result<HashMap<i32, Vec<i32>>> {
    // The URL is matched in SQL rather than by pulling every row into Rust:
    // there are millions of URL rows and only the Discogs artist ones matter.
    // `/artist/` specifically -- the same relationship kind also points at
    // label and release pages on some artists.
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT artist_id, url FROM artist_url
         WHERE kind = 'discogs' AND url LIKE '%discogs.com/artist/%'",
    )
    .fetch_all(pool)
    .await
    .context("failed to read Discogs links from the canon")?;

    let mut mapping: HashMap<i32, Vec<i32>> = HashMap::new();
    for (artist_id, url) in rows {
        if let Some(discogs_id) = discogs_id_from_url(&url) {
            mapping.entry(discogs_id).or_default().push(artist_id);
        }
    }
    Ok(mapping)
}

/// Pulls the id out of a Discogs artist URL.
///
/// The address has taken several forms over the years -- `www.discogs.com`,
/// no `www`, a trailing slug after the number, a trailing slash -- so the
/// number is read up to the first non-digit rather than matched as a whole
/// string.
fn discogs_id_from_url(url: &str) -> Option<i32> {
    let after = url.split("/artist/").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The date out of `discogs_20260801_masters.xml.gz`.
fn version_from_filename(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let after = name.strip_prefix("discogs_")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    (digits.len() == 8).then_some(digits)
}

/// Opens a gzipped dump for streaming.
///
/// `MultiGzDecoder` rather than `GzDecoder`: a file built by concatenating
/// gzip members would otherwise stop at the first one and silently truncate
/// the import — the same trap the MusicBrainz importer hit with bzip2.
fn open_dump(path: &Path) -> Result<impl BufRead> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let size = file.metadata().map_or(0, |m| m.len());
    tracing::info!(dump = %path.display(), size_mb = size / 1_048_576, "reading a Discogs dump");
    let decoder = MultiGzDecoder::new(BufReader::with_capacity(1 << 20, file));
    Ok(BufReader::with_capacity(1 << 20, decoder))
}

/// Reads the artists file, keeping only the ids the canon points at.
fn read_artist_ids(path: &Path, wanted: &HashSet<i32>) -> Result<HashSet<i32>> {
    let mut records = Records::<_, DiscogsArtist>::new(open_dump(path)?);
    let mut found = HashSet::with_capacity(wanted.len());
    let mut total: u64 = 0;
    while let Some(record) = records.next_record()? {
        total += 1;
        if let Some(id) = record.id
            && wanted.contains(&id)
        {
            found.insert(id);
        }
    }
    tracing::info!(records = total, linked_found = found.len(), "artists file read");
    Ok(found)
}

/// Adds one master's genres to the running counts, returning how many artist
/// credits it contributed.
///
/// Separate from the reading loop so the counting rules -- which credits count,
/// which masters are skipped -- are the same ones the tests exercise, rather
/// than a second copy of them written in the test module.
fn tally(master: &Master, wanted: &HashSet<i32>, known: Option<&HashSet<i32>>, counts: &mut GenreCounts) -> u64 {
    // A master with no genre at all says nothing about its artists.
    if master.genres.is_empty() && master.styles.is_empty() {
        return 0;
    }

    let mut credited = 0;
    for artist in &master.artists {
        if !wanted.contains(artist) {
            continue;
        }
        // When the artists file was read, a credit pointing at an id that is
        // not in it is a dangling reference in the dump itself.
        if known.is_some_and(|known| !known.contains(artist)) {
            continue;
        }
        credited += 1;
        let per_artist = counts.entry(*artist).or_default();
        for genre in &master.genres {
            *per_artist.entry((genre.clone(), false)).or_insert(0) += 1;
        }
        for style in &master.styles {
            *per_artist.entry((style.clone(), true)).or_insert(0) += 1;
        }
    }
    credited
}

/// Reads the masters file, counting genres and styles per Discogs artist.
fn read_masters(path: &Path, wanted: &HashSet<i32>, known: Option<&HashSet<i32>>) -> Result<GenreCounts> {
    let mut records = Records::<_, Master>::new(open_dump(path)?);
    let mut counts: GenreCounts = HashMap::new();
    let mut total: u64 = 0;
    let mut credited: u64 = 0;

    while let Some(master) = records.next_record()? {
        total += 1;
        credited += tally(&master, wanted, known, &mut counts);
    }

    tracing::info!(
        records = total,
        artists_with_genres = counts.len(),
        credits_counted = credited,
        "masters file read"
    );
    Ok(counts)
}

/// Reads the labels file.
fn read_labels(path: &Path) -> Result<Vec<Label>> {
    let mut records = Records::<_, Label>::new(open_dump(path)?);
    let mut labels = Vec::new();
    let mut total: u64 = 0;
    while let Some(label) = records.next_record()? {
        total += 1;
        if label.id.is_some() && label.name.is_some() {
            labels.push(label);
        }
    }
    tracing::info!(records = total, kept = labels.len(), "labels file read");
    Ok(labels)
}

/// Writes everything in one transaction: an interrupted import leaves the
/// previous genres and labels intact rather than a half-replaced set.
async fn write(pool: &PgPool, mapping: &HashMap<i32, Vec<i32>>, counts: &GenreCounts, labels: &[Label], version: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('discogs', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    // Re-importing replaces what this source owns and nothing else.
    sqlx::query("TRUNCATE artist_genre, artist_discogs, genre RESTART IDENTITY CASCADE")
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous genres")?;

    let mut written: i64 = 0;
    written += write_artist_discogs(&mut tx, mapping).await?;
    written += write_genres(&mut tx, mapping, counts).await?;
    if !labels.is_empty() {
        written += write_labels(&mut tx, labels).await?;
    }

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, rows = written, "Discogs import complete");
    Ok(())
}

/// Records which Discogs artist each canonical artist is, so the join is
/// visible in the database rather than re-derived from URLs every time.
async fn write_artist_discogs(tx: &mut sqlx::PgTransaction<'_>, mapping: &HashMap<i32, Vec<i32>>) -> Result<i64> {
    let pairs: Vec<(i32, i32)> = mapping
        .iter()
        .flat_map(|(discogs_id, artists)| artists.iter().map(move |artist| (*artist, *discogs_id)))
        .collect();

    let mut written = 0i64;
    for chunk in pairs.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let discogs: Vec<i32> = chunk.iter().map(|(_, d)| *d).collect();

        sqlx::query(
            "INSERT INTO artist_discogs (artist_id, discogs_id)
             SELECT * FROM UNNEST($1::int[], $2::int[])
             ON CONFLICT (artist_id) DO NOTHING",
        )
        .bind(&artists)
        .bind(&discogs)
        .execute(&mut **tx)
        .await
        .context("failed to write Discogs links")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "Discogs links written");
    Ok(written)
}

/// Turns per-Discogs-artist counts into the rows `artist_genre` holds.
///
/// Summed before insertion, not after. A canonical artist can carry several
/// Discogs links -- MusicBrainz keeps one act where Discogs split it, and
/// 24,185 artists in a full canon do -- so the same (artist, genre) pair can
/// arrive from two Discogs entries. Postgres refuses to let one statement
/// update a row twice ("ON CONFLICT DO UPDATE command cannot affect row a
/// second time"), and summing is also the honest answer: those really are the
/// same artist's releases.
///
/// Separate from the writing so the tests exercise these rules rather than a
/// reimplementation of them.
fn genre_rows(mapping: &HashMap<i32, Vec<i32>>, counts: &GenreCounts, genre_ids: &HashMap<(String, bool), i32>) -> Vec<(i32, i32, i32)> {
    let mut totals: HashMap<(i32, i32), i32> = HashMap::new();
    for (discogs_id, per_artist) in counts {
        let Some(artists) = mapping.get(discogs_id) else {
            continue;
        };
        for artist in artists {
            for (key, releases) in per_artist {
                if let Some(genre_id) = genre_ids.get(key) {
                    *totals.entry((*artist, *genre_id)).or_insert(0) += *releases;
                }
            }
        }
    }

    // Sorted so a re-import writes the same rows in the same order: the canon
    // is rebuilt from dumps, and hash order is not reproducible.
    let mut rows: Vec<(i32, i32, i32)> = totals.into_iter().map(|((artist, genre), releases)| (artist, genre, releases)).collect();
    rows.sort_unstable();
    rows
}

/// Writes the genre vocabulary and each artist's aggregated genres.
async fn write_genres(tx: &mut sqlx::PgTransaction<'_>, mapping: &HashMap<i32, Vec<i32>>, counts: &GenreCounts) -> Result<i64> {
    // The vocabulary first: a few thousand rows, inserted once, so the
    // per-artist rows can reference ids instead of repeating text.
    let vocabulary: HashSet<(&str, bool)> = counts
        .values()
        .flat_map(|per_artist| per_artist.keys().map(|(name, is_style)| (name.as_str(), *is_style)))
        .collect();

    let names: Vec<&str> = vocabulary.iter().map(|(name, _)| *name).collect();
    let styles: Vec<bool> = vocabulary.iter().map(|(_, is_style)| *is_style).collect();
    let ids: Vec<(i32, String, bool)> = sqlx::query_as(
        "INSERT INTO genre (name, is_style)
         SELECT * FROM UNNEST($1::text[], $2::bool[])
         RETURNING id, name, is_style",
    )
    .bind(&names)
    .bind(&styles)
    .fetch_all(&mut **tx)
    .await
    .context("failed to write the genre vocabulary")?;
    tracing::info!(rows = ids.len(), "genre vocabulary written");

    let genre_ids: HashMap<(String, bool), i32> = ids.into_iter().map(|(id, name, is_style)| ((name, is_style), id)).collect();

    // One Discogs artist can be several canonical artists (MusicBrainz splits
    // an act Discogs keeps whole), so the counts fan out over all of them.
    let rows = genre_rows(mapping, counts, &genre_ids);

    let mut written = 0i64;
    for chunk in rows.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _, _)| *a).collect();
        let genres: Vec<i32> = chunk.iter().map(|(_, g, _)| *g).collect();
        let releases: Vec<i32> = chunk.iter().map(|(_, _, r)| *r).collect();

        sqlx::query(
            "INSERT INTO artist_genre (artist_id, genre_id, releases)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::int[])
             ON CONFLICT (artist_id, genre_id) DO UPDATE SET releases = artist_genre.releases + EXCLUDED.releases",
        )
        .bind(&artists)
        .bind(&genres)
        .bind(&releases)
        .execute(&mut **tx)
        .await
        .context("failed to write artist genres")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "artist genres written");
    Ok(written)
}

async fn write_labels(tx: &mut sqlx::PgTransaction<'_>, labels: &[Label]) -> Result<i64> {
    // Labels are replaced wholesale, but the parent link is set in a second
    // pass: a parent can appear after its child in the dump, and the foreign
    // key would reject the row.
    sqlx::query("TRUNCATE label CASCADE")
        .execute(&mut **tx)
        .await
        .context("failed to clear the previous labels")?;

    let mut written = 0i64;
    for chunk in labels.chunks(BATCH) {
        let ids: Vec<i32> = chunk.iter().filter_map(|l| l.id).collect();
        let names: Vec<&str> = chunk.iter().filter_map(|l| l.name.as_deref()).collect();
        let profiles: Vec<Option<&str>> = chunk.iter().map(|l| l.profile.as_deref()).collect();

        sqlx::query(
            "INSERT INTO label (id, name, profile)
             SELECT * FROM UNNEST($1::int[], $2::text[], $3::text[])
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&ids)
        .bind(&names)
        .bind(&profiles)
        .execute(&mut **tx)
        .await
        .context("failed to write labels")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }

    // Now the ownership links, dropping the ones that point nowhere or at
    // themselves.
    let children: Vec<i32> = labels.iter().filter(|l| l.parent_id.is_some()).filter_map(|l| l.id).collect();
    let parents: Vec<i32> = labels.iter().filter(|l| l.id.is_some()).filter_map(|l| l.parent_id).collect();
    let updated = sqlx::query(
        "UPDATE label SET parent_label_id = pairs.parent
         FROM UNNEST($1::int[], $2::int[]) AS pairs(child, parent)
         WHERE label.id = pairs.child
           AND pairs.parent <> pairs.child
           AND EXISTS (SELECT 1 FROM label AS p WHERE p.id = pairs.parent)",
    )
    .bind(&children)
    .bind(&parents)
    .execute(&mut **tx)
    .await
    .context("failed to link labels to their parents")?;

    tracing::info!(rows = written, parents = updated.rows_affected(), "labels written");
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_genres_when_one_artist_has_several_discogs_entries() {
        // MusicBrainz keeps one act where Discogs split it: 24,185 artists in
        // a full canon carry more than one Discogs link. Both entries'
        // releases belong to the same star, so they add up -- and emitting the
        // pair twice instead would make Postgres refuse the whole insert
        // ("ON CONFLICT DO UPDATE command cannot affect row a second time").
        let mut counts: GenreCounts = HashMap::new();
        counts.insert(700, HashMap::from([(("Techno".to_string(), true), 3)]));
        counts.insert(701, HashMap::from([(("Techno".to_string(), true), 5)]));

        // Both Discogs ids resolve to canonical artist 42.
        let mapping: HashMap<i32, Vec<i32>> = HashMap::from([(700, vec![42]), (701, vec![42])]);
        let genre_ids: HashMap<(String, bool), i32> = HashMap::from([(("Techno".to_string(), true), 9)]);

        let rows = genre_rows(&mapping, &counts, &genre_ids);
        assert_eq!(rows, vec![(42, 9, 8)], "the two entries should sum, not collide");
    }

    #[test]
    fn genre_rows_come_out_in_a_stable_order() {
        // The canon is rebuilt from dumps; hash order would make two imports
        // of the same input write different files.
        let counts: GenreCounts = HashMap::from([
            (1, HashMap::from([(("Rock".to_string(), false), 2)])),
            (2, HashMap::from([(("Jazz".to_string(), false), 1)])),
        ]);
        let mapping: HashMap<i32, Vec<i32>> = HashMap::from([(1, vec![5]), (2, vec![3])]);
        let genre_ids: HashMap<(String, bool), i32> = HashMap::from([(("Rock".to_string(), false), 10), (("Jazz".to_string(), false), 11)]);

        let first = genre_rows(&mapping, &counts, &genre_ids);
        let second = genre_rows(&mapping, &counts, &genre_ids);
        assert_eq!(first, second);
        assert!(first.windows(2).all(|w| w[0] <= w[1]), "rows should be sorted");
    }

    #[test]
    fn reads_the_discogs_id_out_of_every_url_form() {
        for url in [
            "https://www.discogs.com/artist/11136",
            "http://discogs.com/artist/11136",
            "https://www.discogs.com/artist/11136-Peter-Gabriel",
            "https://www.discogs.com/artist/11136/",
        ] {
            assert_eq!(discogs_id_from_url(url), Some(11136), "failed on {url}");
        }
    }

    #[test]
    fn ignores_urls_that_are_not_artist_pages() {
        // The same relationship kind points at label and release pages too.
        assert_eq!(discogs_id_from_url("https://www.discogs.com/label/1-Planet-E"), None);
        assert_eq!(discogs_id_from_url("https://www.discogs.com/release/116925"), None);
        assert_eq!(discogs_id_from_url("https://www.discogs.com/artist/none"), None);
    }

    #[test]
    fn takes_the_version_from_the_filename() {
        assert_eq!(
            version_from_filename(Path::new("/dumps/discogs_20260801_masters.xml.gz")).as_deref(),
            Some("20260801")
        );
        assert_eq!(version_from_filename(Path::new("/dumps/masters.xml.gz")), None);
        // A date of the wrong length is not a date.
        assert_eq!(version_from_filename(Path::new("/dumps/discogs_2026_masters.xml.gz")), None);
    }

    /// Reads masters out of a literal document through the importer's own
    /// counting function, so these tests exercise the shipped rules rather
    /// than a reimplementation of them.
    fn count(xml: &str, wanted: &[i32]) -> GenreCounts {
        let wanted: HashSet<i32> = wanted.iter().copied().collect();
        let mut records = Records::<_, Master>::new(xml.as_bytes());
        let mut counts: GenreCounts = HashMap::new();
        while let Some(master) = records.next_record().unwrap() {
            tally(&master, &wanted, None, &mut counts);
        }
        counts
    }

    #[test]
    fn counts_a_genre_once_per_release() {
        // Two releases, both Techno: the weight is what makes the aggregate
        // honest, so it must be 2 rather than "present".
        let xml = concat!(
            "<masters>",
            "<master id=\"1\"><artists><artist><id>7</id><name>A</name></artist></artists>",
            "<genres><genre>Electronic</genre></genres><styles><style>Techno</style></styles></master>",
            "<master id=\"2\"><artists><artist><id>7</id><name>A</name></artist></artists>",
            "<genres><genre>Electronic</genre></genres><styles><style>Techno</style></styles></master>",
            "</masters>"
        );
        let counts = count(xml, &[7]);
        assert_eq!(counts[&7][&("Techno".to_string(), true)], 2);
        assert_eq!(counts[&7][&("Electronic".to_string(), false)], 2);
    }

    #[test]
    fn keeps_genres_and_styles_apart() {
        // "Electronic" the genre and "Electronic" as a style would collide on
        // the name alone; `is_style` is what separates them.
        let xml = concat!(
            "<masters><master id=\"1\"><artists><artist><id>7</id></artist></artists>",
            "<genres><genre>Rock</genre></genres><styles><style>Rock &amp; Roll</style></styles>",
            "</master></masters>"
        );
        let counts = count(xml, &[7]);
        assert!(counts[&7].contains_key(&("Rock".to_string(), false)));
        assert!(counts[&7].contains_key(&("Rock & Roll".to_string(), true)));
    }

    #[test]
    fn credits_every_artist_on_a_collaboration() {
        let xml = concat!(
            "<masters><master id=\"1\">",
            "<artists><artist><id>7</id><join>&amp;</join></artist><artist><id>8</id></artist></artists>",
            "<styles><style>Techno</style></styles></master></masters>"
        );
        let counts = count(xml, &[7, 8]);
        assert_eq!(counts[&7][&("Techno".to_string(), true)], 1);
        assert_eq!(counts[&8][&("Techno".to_string(), true)], 1);
    }

    #[test]
    fn ignores_artists_outside_the_canon() {
        let xml = concat!(
            "<masters><master id=\"1\"><artists><artist><id>999</id></artist></artists>",
            "<styles><style>Techno</style></styles></master></masters>"
        );
        assert!(count(xml, &[7]).is_empty());
    }

    #[test]
    fn does_not_take_the_release_name_as_an_artist_id() {
        // `<anv>` is a per-release spelling and `<name>` is a display name;
        // only `<id>` is the join key.
        let xml = concat!(
            "<masters><master id=\"1\">",
            "<artists><artist><id>7</id><name>Samuel L Session</name><anv>Samuel L</anv></artist></artists>",
            "<styles><style>Techno</style></styles></master></masters>"
        );
        let counts = count(xml, &[7]);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[&7].len(), 1);
    }

    #[test]
    fn skips_a_master_with_no_genres_at_all() {
        let xml = "<masters><master id=\"1\"><artists><artist><id>7</id></artist></artists><title>Untitled</title></master></masters>";
        assert!(count(xml, &[7]).is_empty());
    }

    /// Reads labels out of a literal document.
    fn labels_of(xml: &str) -> Vec<Label> {
        let mut records = Records::<_, Label>::new(xml.as_bytes());
        let mut out = Vec::new();
        while let Some(label) = records.next_record().unwrap() {
            out.push(label);
        }
        out
    }

    #[test]
    fn reads_a_label_with_its_parent() {
        let xml = concat!(
            "<labels><label><id>5</id><name>Svek</name><data_quality>Correct</data_quality>",
            "<parentLabel id=\"4711\">Goldhead Music</parentLabel>",
            "<sublabels><label id=\"2437\">Birdy</label></sublabels></label></labels>"
        );
        let labels = labels_of(xml);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].id, Some(5));
        assert_eq!(labels[0].name.as_deref(), Some("Svek"));
        assert_eq!(labels[0].parent_id, Some(4711));
    }

    #[test]
    fn does_not_mistake_a_sublabel_for_the_labels_own_name() {
        // `<sublabels><label>` carries a name at depth two; the label's own
        // name is at depth one.
        let xml = "<labels><label><id>1</id><name>Planet E</name><sublabels><label id=\"86537\">Antidote</label></sublabels></label></labels>";
        let labels = labels_of(xml);
        assert_eq!(labels[0].name.as_deref(), Some("Planet E"));
        assert_eq!(labels[0].parent_id, None);
    }

    #[test]
    fn leaves_contact_information_out() {
        // Contact blocks carry postal addresses and personal e-mail of small
        // label owners. The importer has no field for them on purpose.
        let xml = concat!(
            "<labels><label><id>1</id><name>Planet E</name>",
            "<contactinfo>P.O. Box 27218, Detroit</contactinfo>",
            "<profile>Carl Craig's techno label.</profile></label></labels>"
        );
        let labels = labels_of(xml);
        assert_eq!(labels[0].profile.as_deref(), Some("Carl Craig's techno label."));
    }
}
