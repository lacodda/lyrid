//! Imports genres, styles, labels and label rosters from the Discogs monthly
//! XML dumps.
//!
//! Four files, each one gzipped XML document, read in one streaming pass
//! apiece, labels first because the others are asked for names it points at:
//!
//! - `labels` — the stations of the map: imprints, their descriptions, and
//!   which imprint owns which.
//! - `artists` — to learn which Discogs ids exist, so a genre can never be
//!   attached through a dangling reference, and to name the artists a label's
//!   description points at by id.
//! - `masters` — where the genres actually live. Discogs puts genre and style
//!   on releases, never on artists, so an artist's genres are aggregated from
//!   their discography.
//! - `releases` — who released on which label, and when: the only place
//!   Discogs joins a label to an artist. Not for genres -- a master carries
//!   the same genres as its pressings -- but for rosters, which exist nowhere
//!   else. See `discogs_releases` and ADR 0017.
//!
//! All four come from one dump and are written in one transaction, so the
//! genres, the labels and the rosters in the canon always share a version.
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

use super::discogs_releases;
use super::discogs_xml::{Attributes, Record, Records};
use crate::markup::{self, Kind};

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to discogs_<date>_masters.xml.gz, which carries the genres.
    #[arg(long, value_name = "FILE")]
    pub masters: PathBuf,

    /// Path to discogs_<date>_labels.xml.gz. Optional: without it the labels
    /// and their rosters are left as they were, and genres still import. With
    /// it, the releases and artists files are needed too -- a label without
    /// its roster, or with a description naming artists by bare id, is half a
    /// station.
    #[arg(long, value_name = "FILE", requires_all = ["releases", "artists"])]
    pub labels: Option<PathBuf>,

    /// Path to discogs_<date>_releases.xml.gz: who released on which label,
    /// and when. Read together with the labels it points at.
    #[arg(long, value_name = "FILE", requires = "labels")]
    pub releases: Option<PathBuf>,

    /// Path to discogs_<date>_artists.xml.gz. Checks that the ids the masters
    /// credit actually exist, and names the artists label descriptions point
    /// at by id.
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
    /// The master's own id -- needed only to name the masters a label
    /// description points at by id.
    id: Option<i32>,
    /// Discogs ids of the credited artists.
    artists: Vec<i32>,
    genres: Vec<String>,
    styles: Vec<String>,
    title: Option<String>,
}

impl Record for Master {
    const ELEMENT: &'static str = "master";

    fn open(attributes: Attributes<'_>) -> Self {
        Self {
            id: attributes.parse("id"),
            ..Self::default()
        }
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
            // The master's own title, at depth one: a video's title sits at
            // `["videos", "video", "title"]` and is somebody's upload.
            ["title"] if !text.is_empty() => self.title = Some(text.to_string()),
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
            // Line breaks arrive as `&#13;` plus a newline; one kind of break
            // is stored, so a page splitting paragraphs has one rule to follow.
            ["profile"] if !text.is_empty() => self.profile = Some(text.replace("\r\n", "\n").replace('\r', "\n")),
            // camelCase, alone among these element names; the id is on the
            // attribute while the text is the parent's name.
            ["parentLabel"] => self.parent_id = attributes.parse("id"),
            // `<sublabels><label>` names the other direction of the same
            // fact and is skipped: storing both invites them to disagree.
            _ => {}
        }
    }
}

/// One `<artist>`, read for its id and, when a label description points at
/// it, its name.
#[derive(Default)]
struct DiscogsArtist {
    id: Option<i32>,
    name: Option<String>,
}

impl Record for DiscogsArtist {
    const ELEMENT: &'static str = "artist";

    fn open(_: Attributes<'_>) -> Self {
        Self::default()
    }

    fn field(&mut self, path: &[&str], text: &str, _: Attributes<'_>) {
        // Only the record's own id and name, at depth one. `["members", "id"]`
        // and `["aliases", "name"]` are other artists.
        match path {
            ["id"] => self.id = text.parse().ok(),
            ["name"] if !text.is_empty() => self.name = Some(text.to_string()),
            _ => {}
        }
    }
}

/// What one pass over the masters file yields, per artist.
type GenreCounts = HashMap<i32, HashMap<(String, bool), i32>>;

/// The references label descriptions make by bare id, grouped by what they
/// point at, so each file's pass can pick up the names it holds.
#[derive(Default)]
struct Wanted {
    artists: HashSet<i32>,
    labels: HashSet<i32>,
    masters: HashSet<i32>,
    releases: HashSet<i32>,
}

impl Wanted {
    fn of(labels: &[Label]) -> Self {
        let mut wanted = Self::default();
        for (kind, id) in labels.iter().filter_map(|l| l.profile.as_deref()).flat_map(markup::unnamed) {
            let set = match kind {
                Kind::Artist => &mut wanted.artists,
                Kind::Label => &mut wanted.labels,
                Kind::Master => &mut wanted.masters,
                Kind::Release => &mut wanted.releases,
            };
            set.insert(id);
        }
        wanted
    }
}

/// Names found for the references in label descriptions.
#[derive(Default)]
struct Names {
    artists: HashMap<i32, String>,
    labels: HashMap<i32, String>,
    masters: HashMap<i32, String>,
    releases: HashMap<i32, String>,
}

/// Everything read out of the dump files, ready to write.
struct Read {
    counts: GenreCounts,
    labels: Vec<Label>,
    rosters: Option<discogs_releases::Rosters>,
}

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
    let latest = latest_year(&version)?;

    // Only the ids that some canonical artist actually points at are worth
    // counting; the rest of Discogs is millions of artists this sky has never
    // heard of.
    let credited: HashSet<i32> = mapping.keys().copied().collect();

    // Labels first: their descriptions say which names the other files must
    // be asked for.
    let mut labels = match &args.labels {
        Some(path) => read_labels(path)?,
        None => Vec::new(),
    };
    let wanted = Wanted::of(&labels);
    let mut names = Names {
        labels: labels
            .iter()
            .filter(|l| l.id.is_some_and(|id| wanted.labels.contains(&id)))
            .filter_map(|l| Some((l.id?, l.name.clone()?)))
            .collect(),
        ..Names::default()
    };

    let known_artists = match &args.artists {
        Some(path) => {
            let (known, artist_names) = read_artists(path, &credited, &wanted.artists)?;
            names.artists = artist_names;
            Some(known)
        }
        None => None,
    };

    let (counts, titles) = read_masters(&args.masters, &credited, known_artists.as_ref(), &wanted.masters)?;
    names.masters = titles;

    let rosters = match &args.releases {
        Some(path) => {
            let rosters = discogs_releases::read(path, &credited, &wanted.releases, latest)?;
            names.releases.clone_from(&rosters.titles);
            Some(rosters)
        }
        None => None,
    };

    name_references(&mut labels, &names);

    write(pool, &mapping, Read { counts, labels, rosters }, &version).await
}

/// The last year a release in this dump can carry: the year after the dump's
/// own, since a label announces what it will issue next January.
fn latest_year(version: &str) -> Result<i16> {
    version
        .get(..4)
        .and_then(|year| year.parse::<i16>().ok())
        .map(|year| year + 1)
        .with_context(|| format!("the dump version {version:?} does not start with a year"))
}

/// Rewrites every label description so its id-only references carry names.
fn name_references(labels: &mut [Label], names: &Names) {
    let mut unnamed = 0usize;
    for label in labels.iter_mut() {
        let Some(profile) = label.profile.as_deref() else {
            continue;
        };
        let rewritten = markup::named(profile, |kind, id| {
            let table = match kind {
                Kind::Artist => &names.artists,
                Kind::Label => &names.labels,
                Kind::Master => &names.masters,
                Kind::Release => &names.releases,
            };
            table.get(&id).map(String::as_str)
        });
        unnamed += markup::unnamed(&rewritten).count();
        label.profile = Some(rewritten);
    }
    tracing::info!(left_unnamed = unnamed, "label descriptions named");
}

/// Which Discogs id each canonical artist points at, taken from the URL
/// relationships the MusicBrainz import stored.
async fn load_discogs_ids(pool: &PgPool) -> Result<HashMap<i32, Vec<i32>>> {
    // The URL is matched in SQL rather than by pulling every row into Rust:
    // there are millions of URL rows and only the Discogs artist ones matter.
    // `/artist/` specifically -- the same relationship kind also points at
    // label and release pages on some artists.
    //
    // MusicBrainz's special purpose artists are left out: "Various Artists"
    // links to Discogs's "Various", and joining through it would hand one
    // star the genres and the label of every compilation there is.
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT u.artist_id, u.url FROM artist_url u
         JOIN artist a ON a.id = u.artist_id
         WHERE u.kind = 'discogs' AND u.url LIKE '%discogs.com/artist/%'
           AND a.mbid <> ALL($1)",
    )
    .bind(super::special::special_purpose())
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
pub(super) fn open_dump(path: &Path) -> Result<impl BufRead> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let size = file.metadata().map_or(0, |m| m.len());
    tracing::info!(dump = %path.display(), size_mb = size / 1_048_576, "reading a Discogs dump");
    let decoder = MultiGzDecoder::new(BufReader::with_capacity(1 << 20, file));
    Ok(BufReader::with_capacity(1 << 20, decoder))
}

/// Reads the artists file: which of the ids the canon points at exist, and
/// the names of the artists label descriptions point at.
fn read_artists(path: &Path, credited: &HashSet<i32>, to_name: &HashSet<i32>) -> Result<(HashSet<i32>, HashMap<i32, String>)> {
    let mut records = Records::<_, DiscogsArtist>::new(open_dump(path)?);
    let mut found = HashSet::with_capacity(credited.len());
    let mut names = HashMap::with_capacity(to_name.len());
    let mut total: u64 = 0;
    while let Some(record) = records.next_record()? {
        total += 1;
        let Some(id) = record.id else {
            continue;
        };
        if credited.contains(&id) {
            found.insert(id);
        }
        if let Some(name) = record.name
            && to_name.contains(&id)
        {
            names.insert(id, name);
        }
    }
    tracing::info!(records = total, linked_found = found.len(), named = names.len(), "artists file read");
    Ok((found, names))
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

/// Reads the masters file, counting genres and styles per Discogs artist and
/// keeping the titles of the masters label descriptions point at.
fn read_masters(path: &Path, wanted: &HashSet<i32>, known: Option<&HashSet<i32>>, to_title: &HashSet<i32>) -> Result<(GenreCounts, HashMap<i32, String>)> {
    let mut records = Records::<_, Master>::new(open_dump(path)?);
    let mut counts: GenreCounts = HashMap::new();
    let mut titles = HashMap::with_capacity(to_title.len());
    let mut total: u64 = 0;
    let mut credited: u64 = 0;

    while let Some(master) = records.next_record()? {
        total += 1;
        credited += tally(&master, wanted, known, &mut counts);
        if let (Some(id), Some(title)) = (master.id, master.title)
            && to_title.contains(&id)
        {
            titles.insert(id, title);
        }
    }

    tracing::info!(
        records = total,
        artists_with_genres = counts.len(),
        credits_counted = credited,
        titles = titles.len(),
        "masters file read"
    );
    Ok((counts, titles))
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
/// previous genres, labels and rosters intact rather than a half-replaced set.
async fn write(pool: &PgPool, mapping: &HashMap<i32, Vec<i32>>, read: Read, version: &str) -> Result<()> {
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
    written += write_genres(&mut tx, mapping, &read.counts).await?;
    if !read.labels.is_empty() {
        // Clears the rosters with the labels they hang from: `label_artist`
        // and `label_year` reference `label` and go with it.
        written += write_labels(&mut tx, &read.labels).await?;
        if let Some(rosters) = &read.rosters {
            let held: HashSet<i32> = read.labels.iter().filter_map(|l| l.id).collect();
            written += write_rosters(&mut tx, rosters, mapping, &held).await?;
        }
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

/// Writes the rosters, and the chronology of every label that has one.
async fn write_rosters(
    tx: &mut sqlx::PgTransaction<'_>,
    rosters: &discogs_releases::Rosters,
    mapping: &HashMap<i32, Vec<i32>>,
    held: &HashSet<i32>,
) -> Result<i64> {
    let rows = discogs_releases::roster_rows(&rosters.spans, mapping, held);
    let mut written = 0i64;
    for chunk in rows.chunks(BATCH) {
        let labels: Vec<i32> = chunk.iter().map(|(label, _, _)| *label).collect();
        let artists: Vec<i32> = chunk.iter().map(|(_, artist, _)| *artist).collect();
        let releases: Vec<i32> = chunk.iter().map(|(_, _, span)| span.releases).collect();
        let firsts: Vec<Option<i16>> = chunk.iter().map(|(_, _, span)| span.first).collect();
        let lasts: Vec<Option<i16>> = chunk.iter().map(|(_, _, span)| span.last).collect();
        sqlx::query(
            "INSERT INTO label_artist (label_id, artist_id, releases, first_year, last_year)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::int[], $4::smallint[], $5::smallint[])",
        )
        .bind(&labels)
        .bind(&artists)
        .bind(&releases)
        .bind(&firsts)
        .bind(&lasts)
        .execute(&mut **tx)
        .await
        .context("failed to write label rosters")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }

    let stations: HashSet<i32> = rows.iter().map(|(label, _, _)| *label).collect();
    tracing::info!(rows = written, stations = stations.len(), "label rosters written");

    let years = discogs_releases::year_rows(&rosters.years, &stations);
    let mut year_written = 0i64;
    for chunk in years.chunks(BATCH) {
        let labels: Vec<i32> = chunk.iter().map(|(label, _, _)| *label).collect();
        let years: Vec<i16> = chunk.iter().map(|(_, year, _)| *year).collect();
        let releases: Vec<i32> = chunk.iter().map(|(_, _, releases)| *releases).collect();
        sqlx::query(
            "INSERT INTO label_year (label_id, year, releases)
             SELECT * FROM UNNEST($1::int[], $2::smallint[], $3::int[])",
        )
        .bind(&labels)
        .bind(&years)
        .bind(&releases)
        .execute(&mut **tx)
        .await
        .context("failed to write label chronologies")?;
        year_written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = year_written, "label chronologies written");
    Ok(written + year_written)
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

    #[test]
    fn stores_one_kind_of_line_break_in_a_description() {
        let xml = "<labels><label><id>1</id><name>L</name><profile>One.&#13;\n&#13;\nTwo.\rThree.</profile></label></labels>";
        assert_eq!(labels_of(xml)[0].profile.as_deref(), Some("One.\n\nTwo.\nThree."));
    }

    #[test]
    fn a_master_title_is_its_own_and_not_a_videos() {
        let xml = concat!(
            "<masters><master id=\"12\"><title>Nevermind</title>",
            "<videos><video src=\"x\"><title>Nirvana - Lithium (Live)</title></video></videos></master></masters>"
        );
        let mut records = Records::<_, Master>::new(xml.as_bytes());
        let master = records.next_record().unwrap().unwrap();
        assert_eq!((master.id, master.title.as_deref()), (Some(12), Some("Nevermind")));
    }

    #[test]
    fn an_artists_name_is_its_own_and_not_an_aliases() {
        let xml = concat!(
            "<artists><artist><id>674</id><name>Stephan Grieder</name>",
            "<aliases><name id=\"9\">Other Name</name></aliases></artist></artists>"
        );
        let mut records = Records::<_, DiscogsArtist>::new(xml.as_bytes());
        let artist = records.next_record().unwrap().unwrap();
        assert_eq!((artist.id, artist.name.as_deref()), (Some(674), Some("Stephan Grieder")));
    }

    #[test]
    fn asks_each_file_for_the_names_descriptions_point_at() {
        let labels = labels_of(concat!(
            "<labels><label><id>5</id><name>Svek</name>",
            "<profile>Started by [a674], run with [a=Jesper], see [l2] and [m12] and [r1].</profile></label></labels>"
        ));
        let wanted = Wanted::of(&labels);
        assert_eq!(wanted.artists, HashSet::from([674]));
        assert_eq!(wanted.labels, HashSet::from([2]));
        assert_eq!(wanted.masters, HashSet::from([12]));
        assert_eq!(wanted.releases, HashSet::from([1]));
    }

    #[test]
    fn names_the_references_it_found_names_for() {
        let mut labels = labels_of("<labels><label><id>5</id><name>Svek</name><profile>By [a674] and [a239].</profile></label></labels>");
        let names = Names {
            artists: HashMap::from([(674, "Stephan Grieder".to_string())]),
            ..Names::default()
        };
        name_references(&mut labels, &names);
        assert_eq!(labels[0].profile.as_deref(), Some("By [a674=Stephan Grieder] and [a239]."));
    }

    #[test]
    fn a_release_may_carry_the_year_after_its_dump_and_no_later() {
        assert_eq!(latest_year("20260801").unwrap(), 2027);
        assert!(latest_year("latest").is_err());
    }

    #[test]
    fn labels_come_with_their_releases_and_artists_or_not_at_all() {
        use clap::Parser;

        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            args: Args,
        }

        let base = ["lyrid", "--masters", "m.xml.gz"];
        let with = |extra: &[&'static str]| Cli::try_parse_from(base.iter().chain(extra)).map(|_| ());
        assert!(with(&[]).is_ok(), "genres alone still import");
        assert!(with(&["--labels", "l", "--releases", "r", "--artists", "a"]).is_ok());
        assert!(with(&["--labels", "l"]).is_err(), "a label without its roster is half a station");
        assert!(with(&["--labels", "l", "--releases", "r"]).is_err(), "descriptions would name nobody");
        assert!(with(&["--releases", "r", "--artists", "a"]).is_err(), "rosters need the labels they hang from");
    }
}
