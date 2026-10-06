//! Imports star spectra from the AcousticBrainz dumps.
//!
//! AcousticBrainz ran the Essentia extractor over listeners' own audio files
//! from 2015 to 2022, and published what it measured before it closed: a
//! frozen dump, CC0 throughout. Two parts of it are read here, both keyed by
//! recording MBID and submission number:
//!
//! - the **high-level** archives -- thirty `tar.zst` files of one JSON
//!   document per submission, holding what Essentia's trained models made of
//!   the audio: danceable or not, acoustic, aggressive, happy, sad, sung;
//! - the **rhythm** CSV from the low-level features, for the tempo.
//!
//! A star's spectrum is its recordings' measurements brought together, so the
//! recordings are resolved to artists through a MusicBrainz full export (see
//! `recordings`). Every recording weighs the same: submissions are averaged
//! into their recording first, recordings into their artist second -- a hit
//! submitted two hundred times is still one song.
//!
//! **What is not trusted.** Some submissions carry a collapsed model output:
//! the same value, to five decimals, from tens of thousands of different
//! files -- "relaxed" at 0.80882, "electronic" at 0.97939. The features those
//! models read were broken for that submission, and a model fed broken
//! features answers the same thing whatever the music. Measured on the
//! dump's 100,000-submission sample, a quarter of submissions carry at least
//! one collapsed output, most often from lossy files; Pink Floyd's own were
//! 85% collapsed, which unfiltered made them an electronic instrumental act.
//! Such a submission is skipped whole: when some features are broken, the
//! rest are not to be vouched for. Tempo is not affected -- the rhythm
//! extractor reads the signal differently, and the BPM of collapsed
//! submissions is distributed exactly as the others' -- so a recording's tempo
//! uses all of its submissions. See ADR 0018.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream file, field and model names throughout")]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use serde::Deserialize;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use super::recordings;
use super::special::special_purpose;
use crate::spectrum::{BANDS, QUANTILES};

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

/// The fewest recordings a spectrum is drawn from. One recording is one
/// song's mood, not a star's; the schema holds the same floor as a check.
const MIN_RECORDINGS: usize = 3;

/// How far from a collapsed value an output may stand and still count as
/// collapsed. The collapse is not exact to the last digit -- outputs within a
/// few hundred-thousandths of it trail off on either side, far more of them
/// than the music around them -- and ±0.0001 takes that trail while leaving
/// the honest values near it alone.
const COLLAPSE_TOLERANCE: f32 = 0.0001;

#[derive(ClapArgs)]
pub struct Args {
    /// Directory holding the AcousticBrainz dumps: the thirty
    /// acousticbrainz-highlevel-json-<date>-<n>.tar.zst archives and
    /// acousticbrainz-lowlevel-features-<date>-rhythm.tar.zst.
    #[arg(long, value_name = "DIR")]
    pub dir: PathBuf,

    /// Path to mbdump.tar.bz2 from a MusicBrainz full export, which says
    /// which artist each recording belongs to. Any recent export will do.
    #[arg(long, value_name = "FILE")]
    pub musicbrainz: PathBuf,

    /// The dump version. Defaults to the date in the archive names.
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    let dumps = Dumps::find(&args.dir)?;
    let version = args.version.clone().unwrap_or_else(|| dumps.version.clone());

    let canon = load_canon(pool).await?;
    if canon.is_empty() {
        bail!("no artists in the canon: run `lyrid import musicbrainz` first");
    }
    tracing::info!(archives = dumps.highlevel.len(), artists = canon.len(), "reading the high-level archives");

    let (mut heard, read) = read_highlevel(&dumps.highlevel)?;
    tracing::info!(
        submissions = read.submissions,
        collapsed = read.collapsed,
        undecided = read.undecided,
        unreadable = read.unreadable,
        recordings = heard.len(),
        "high-level archives read; reading tempo"
    );
    // The list of collapses was drawn from this dump; a spike it does not
    // explain means a broken model is still being averaged into the spectra.
    for (output, value, submissions) in unexplained(&read) {
        tracing::warn!(output, value, submissions, "a value no music produces is not in the list of collapses");
    }

    let tempos = read_rhythm(&dumps.rhythm, &mut heard)?;
    // A recording is measured when it has both halves: a mood that could be
    // trusted and a tempo.
    heard.retain(|_, recording| !recording.tempos.is_empty());
    tracing::info!(rows = tempos, recordings = heard.len(), "tempo read; resolving recordings to artists");

    let wanted: HashSet<Uuid> = heard.keys().copied().collect();
    let resolved = recordings::artists_of(&args.musicbrainz, &wanted)?;
    tracing::info!(
        export = resolved.timestamp.as_deref().unwrap_or("unknown"),
        direct = resolved.direct,
        redirected = resolved.redirected,
        unknown = resolved.unknown,
        "recordings resolved"
    );

    let spectra = spectra(&heard, &resolved.artists, &canon);
    tracing::info!(artists = spectra.len(), "spectra measured; writing to PostgreSQL");

    write(pool, &spectra, &version).await
}

/// The files of one AcousticBrainz dump.
struct Dumps {
    /// High-level archives in their numbered order.
    highlevel: Vec<PathBuf>,
    rhythm: PathBuf,
    /// The dump date the file names carry: "20220623".
    version: String,
}

impl Dumps {
    fn find(dir: &Path) -> Result<Self> {
        let mut highlevel: Vec<(u32, PathBuf)> = Vec::new();
        let mut rhythm: Vec<(String, PathBuf)> = Vec::new();
        let mut versions: HashSet<String> = HashSet::new();

        for entry in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
            let path = entry.context("failed to list the dump directory")?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if let Some((version, part)) = highlevel_name(name) {
                versions.insert(version.to_string());
                highlevel.push((part, path));
            } else if let Some(version) = rhythm_name(name) {
                versions.insert(version.to_string());
                rhythm.push((version.to_string(), path));
            }
        }

        if versions.len() > 1 {
            bail!("the directory mixes dumps of different dates: {versions:?}");
        }
        let [(version, rhythm)] = <[_; 1]>::try_from(rhythm)
            .map_err(|found| anyhow::anyhow!("expected one acousticbrainz-lowlevel-features-<date>-rhythm.tar.zst, found {}", found.len()))?;
        if highlevel.is_empty() {
            bail!("no acousticbrainz-highlevel-json-<date>-<n>.tar.zst in {}", dir.display());
        }

        // The archives are numbered from 0. A gap is a download that did not
        // finish, and importing around it would publish spectra measured on
        // part of the data as if they were the whole.
        highlevel.sort_by_key(|(part, _)| *part);
        for (expected, (part, _)) in (0u32..).zip(&highlevel) {
            if *part != expected {
                bail!("high-level archive {expected} is missing: the archives must run 0, 1, 2... without a gap");
            }
        }

        Ok(Self {
            highlevel: highlevel.into_iter().map(|(_, path)| path).collect(),
            rhythm,
            version,
        })
    }
}

/// `acousticbrainz-highlevel-json-20220623-7.tar.zst` -> ("20220623", 7).
fn highlevel_name(name: &str) -> Option<(&str, u32)> {
    let rest = name.strip_prefix("acousticbrainz-highlevel-json-")?.strip_suffix(".tar.zst")?;
    let (version, part) = rest.split_once('-')?;
    Some((version, part.parse().ok()?))
}

/// `acousticbrainz-lowlevel-features-20220623-rhythm.tar.zst` -> "20220623".
fn rhythm_name(name: &str) -> Option<&str> {
    name.strip_prefix("acousticbrainz-lowlevel-features-")?.strip_suffix("-rhythm.tar.zst")
}

async fn load_canon(pool: &PgPool) -> Result<HashSet<i32>> {
    // MusicBrainz's special purpose artists are credits, not acts: the mood
    // of every compilation ever measured is not a star's spectrum.
    let ids: Vec<i32> = sqlx::query_scalar("SELECT id FROM artist WHERE mbid <> ALL($1)")
        .bind(special_purpose())
        .fetch_all(pool)
        .await
        .context("failed to read the canon's artists")?;
    Ok(ids.into_iter().collect())
}

/// The model outputs read from a document, in one fixed order: the nine a
/// spectrum is made of, then gender, which is read only as a sign of health.
const OUTPUTS: usize = 10;
/// How many of them a spectrum keeps: all but gender.
const MOODS: usize = 9;
const DANCEABLE: usize = 0;
const ACOUSTIC: usize = 1;
const ELECTRONIC: usize = 2;
const AGGRESSIVE: usize = 3;
const HAPPY: usize = 4;
const PARTY: usize = 5;
const RELAXED: usize = 6;
const SAD: usize = 7;
const VOICE: usize = 8;
const GENDER: usize = 9;

/// The outputs' names, for the import's report.
const OUTPUT_NAMES: [&str; OUTPUTS] = [
    "danceable",
    "acoustic",
    "electronic",
    "aggressive",
    "happy",
    "party",
    "relaxed",
    "sad",
    "voice",
    "gender",
];

/// One recording, as far as it has been read.
#[derive(Default)]
struct Recording {
    /// Submissions whose model outputs could be trusted.
    heard: u32,
    /// Running sums of those outputs, in `MOODS` order.
    sums: [f32; MOODS],
    /// The BPM of every submission of the recording.
    tempos: Vec<f32>,
}

impl Recording {
    fn absorb(&mut self, other: Self) {
        self.heard += other.heard;
        for (sum, more) in self.sums.iter_mut().zip(other.sums) {
            *sum += more;
        }
        self.tempos.extend(other.tempos);
    }
}

/// What the high-level pass counted, so the import reports its own losses.
struct ReadStats {
    submissions: u64,
    collapsed: u64,
    undecided: u64,
    unreadable: u64,
    /// Every readable output, binned to five decimals, one histogram per
    /// output: what finds a collapse the list below does not know about.
    histograms: Vec<Vec<u32>>,
}

impl Default for ReadStats {
    fn default() -> Self {
        Self {
            submissions: 0,
            collapsed: 0,
            undecided: 0,
            unreadable: 0,
            histograms: vec![vec![0; BINS]; OUTPUTS],
        }
    }
}

impl ReadStats {
    fn absorb(&mut self, other: &Self) {
        self.submissions += other.submissions;
        self.collapsed += other.collapsed;
        self.undecided += other.undecided;
        self.unreadable += other.unreadable;
        for (mine, theirs) in self.histograms.iter_mut().zip(&other.histograms) {
            for (bin, more) in mine.iter_mut().zip(theirs) {
                *bin += more;
            }
        }
    }

    fn record(&mut self, outputs: &[f32; OUTPUTS]) {
        for (histogram, &p) in self.histograms.iter_mut().zip(outputs) {
            histogram[bin(p)] += 1;
        }
    }
}

/// One high-level document, with only the outputs read here. Every other
/// model -- the genre classifiers among them, which AcousticBrainz's own
/// maintainers found too weak to use -- is skipped by the parser unread.
#[derive(Deserialize)]
struct Document {
    highlevel: Option<HighLevel>,
}

#[derive(Deserialize)]
struct HighLevel {
    danceability: Model<Danceability>,
    mood_acoustic: Model<Acoustic>,
    mood_electronic: Model<Electronic>,
    mood_aggressive: Model<Aggressive>,
    mood_happy: Model<Happy>,
    mood_party: Model<Party>,
    mood_relaxed: Model<Relaxed>,
    mood_sad: Model<Sad>,
    voice_instrumental: Model<Voice>,
    /// Read for one reason only: its collapsed value is one of the signs that
    /// a submission's features were broken. Never stored, never shown -- what
    /// a model guesses about a singer's sex is not a fact about anyone.
    gender: Model<Gender>,
}

#[derive(Deserialize)]
struct Model<T> {
    all: T,
}

#[derive(Deserialize)]
struct Danceability {
    danceable: f32,
}
#[derive(Deserialize)]
struct Acoustic {
    acoustic: f32,
}
#[derive(Deserialize)]
struct Electronic {
    electronic: f32,
}
#[derive(Deserialize)]
struct Aggressive {
    aggressive: f32,
}
#[derive(Deserialize)]
struct Happy {
    happy: f32,
}
#[derive(Deserialize)]
struct Party {
    party: f32,
}
#[derive(Deserialize)]
struct Relaxed {
    relaxed: f32,
}
#[derive(Deserialize)]
struct Sad {
    sad: f32,
}
#[derive(Deserialize)]
struct Voice {
    voice: f32,
}
#[derive(Deserialize)]
struct Gender {
    female: f32,
}

/// Reads one high-level document's outputs, or `None` when it is not a
/// document of the expected shape or an output is not a probability.
fn read_outputs(json: &[u8]) -> Option<[f32; OUTPUTS]> {
    let Document { highlevel: Some(models) } = serde_json::from_slice::<Document>(json).ok()? else {
        return None;
    };
    let mut outputs = [0.0; OUTPUTS];
    outputs[DANCEABLE] = models.danceability.all.danceable;
    outputs[ACOUSTIC] = models.mood_acoustic.all.acoustic;
    outputs[ELECTRONIC] = models.mood_electronic.all.electronic;
    outputs[AGGRESSIVE] = models.mood_aggressive.all.aggressive;
    outputs[HAPPY] = models.mood_happy.all.happy;
    outputs[PARTY] = models.mood_party.all.party;
    outputs[RELAXED] = models.mood_relaxed.all.relaxed;
    outputs[SAD] = models.mood_sad.all.sad;
    outputs[VOICE] = models.voice_instrumental.all.voice;
    outputs[GENDER] = models.gender.all.female;
    outputs.iter().all(|p| (0.0..=1.0).contains(p)).then_some(outputs)
}

/// The values the models collapse to when the features they read are broken,
/// as `(output, value)`: single values held by thousands of unrelated
/// submissions, where the music around them varies smoothly. Found by
/// binning every output of the dump to five decimals; the import repeats
/// that search and warns about any collapse missing here.
///
/// Party collapses too (at 0.00002), but only ever together with one of
/// these, so it adds nothing as a sign and would cost the honest near-zero
/// values it sits among.
const COLLAPSED: [(usize, f32); 5] = [
    (RELAXED, 0.808_82),
    (ELECTRONIC, 0.979_39),
    (ACOUSTIC, 0.095_59),
    (SAD, 0.352_21),
    (GENDER, 0.622_13),
];

/// Whether a submission's outputs can be trusted.
#[derive(Debug, PartialEq)]
enum Verdict {
    Trusted,
    /// A model answered with its collapsed value.
    Collapsed,
    /// A model answered exactly one half: both of its classes equally likely,
    /// which is the uniform answer a model gives when it has nothing to go on,
    /// not a reading of the music. Five submissions in a hundred have one.
    Undecided,
}

/// A submission is trusted only when every model really answered.
///
/// One failed model condemns the whole submission rather than only its own
/// output: the outputs come from one analysis of one file, and when part of
/// it is known to be broken, the rest is not to be vouched for.
fn judge(outputs: &[f32; OUTPUTS]) -> Verdict {
    if COLLAPSED.iter().any(|&(output, value)| (outputs[output] - value).abs() <= COLLAPSE_TOLERANCE) {
        return Verdict::Collapsed;
    }
    // Exactly: the uniform answer is one half to the last bit, and a near
    // half is an answer.
    if outputs.contains(&0.5) {
        return Verdict::Undecided;
    }
    Verdict::Trusted
}

/// Bins of five decimals over 0..=1.
const BINS: usize = 100_001;

fn bin(p: f32) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a probability times 100,000, clamped into the bins"
    )]
    let index = (p.clamp(0.0, 1.0) * 100_000.0).round() as usize;
    index.min(BINS - 1)
}

/// Values no music produces: a bin holding at least one submission in ten
/// thousand and twenty times what the bins around it hold.
///
/// The neighbours are read from 11 to 60 bins away on either side, skipping
/// the trail a collapse leaves next to itself. Both ends of the scale are
/// left out -- models saturate at 0 and 1 for honest reasons -- and so is the
/// exact half, which is counted as undecided on its own.
fn spikes(histogram: &[u32], total: u64) -> Vec<(usize, u32)> {
    const TRAIL: usize = 10;
    const REACH: usize = 60;
    let floor = (total / 10_000).max(100);
    let mut found = Vec::new();
    for index in REACH..histogram.len() - REACH {
        let count = histogram[index];
        if u64::from(count) < floor || index == BINS / 2 {
            continue;
        }
        // One report per collapse: its trail is not a second one.
        if histogram[index - TRAIL..=index + TRAIL].iter().any(|&near| near > count) {
            continue;
        }
        let around: u64 = (index - REACH..index - TRAIL)
            .chain(index + TRAIL + 1..=index + REACH)
            .map(|i| u64::from(histogram[i]))
            .sum();
        // A hundred neighbour bins: twenty times their mean is a fifth of
        // their sum.
        if u64::from(count) * 5 > around {
            found.push((index, count));
        }
    }
    found
}

/// Spikes in the outputs that the list of collapses does not explain.
fn unexplained(stats: &ReadStats) -> Vec<(&'static str, f32, u32)> {
    let mut out = Vec::new();
    for (output, histogram) in stats.histograms.iter().enumerate() {
        for (index, count) in spikes(histogram, stats.submissions) {
            #[allow(clippy::cast_precision_loss, reason = "a bin index below 100,001")]
            let value = index as f32 / 100_000.0;
            let known = COLLAPSED.iter().any(|&(o, v)| o == output && (v - value).abs() <= COLLAPSE_TOLERANCE);
            if !known {
                out.push((OUTPUT_NAMES[output], value, count));
            }
        }
    }
    out
}

/// The recording MBID a high-level entry is about, from its path:
/// `.../highlevel/0e/1/0e11c0fd-a1da-4b88-a438-7ef55c5809ec-0.json`.
///
/// Read from the path rather than from the tags inside the document: the
/// path is AcousticBrainz's own key, the tags are whatever the submitter's
/// files said.
fn recording_of(path: &Path) -> Option<Uuid> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    Uuid::parse_str(stem.get(..36)?).ok()
}

/// Reads every high-level archive, several at a time.
///
/// Each archive is a separate zstd stream with its own share of the
/// submissions, so they decode in parallel and their counts are merged as
/// they finish. A recording's submissions may sit in more than one archive;
/// the merge adds them up.
fn read_highlevel(archives: &[PathBuf]) -> Result<(HashMap<Uuid, Recording>, ReadStats)> {
    let workers = std::thread::available_parallelism().map_or(4, NonZero::get).min(archives.len());
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel::<Result<(HashMap<Uuid, Recording>, ReadStats)>>();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || {
                while let Some(path) = archives.get(next.fetch_add(1, Ordering::Relaxed)) {
                    // A closed channel means the reader gave up on an error;
                    // there is nobody left to hand a result to.
                    if sender.send(read_highlevel_archive(path)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);

        let mut heard: HashMap<Uuid, Recording> = HashMap::new();
        let mut stats = ReadStats::default();
        for (done, part) in (1..).zip(receiver) {
            let (part, part_stats) = part?;
            stats.absorb(&part_stats);
            for (mbid, recording) in part {
                heard.entry(mbid).or_default().absorb(recording);
            }
            tracing::info!(archives = done, of = archives.len(), recordings = heard.len(), "high-level archive read");
        }
        Ok((heard, stats))
    })
}

fn read_highlevel_archive(path: &Path) -> Result<(HashMap<Uuid, Recording>, ReadStats)> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(file).with_context(|| format!("{} is not a zstd stream", path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut heard: HashMap<Uuid, Recording> = HashMap::new();
    let mut stats = ReadStats::default();
    let mut json = Vec::new();

    for entry in archive.entries().with_context(|| format!("{} is not a readable tar archive", path.display()))? {
        let mut entry = entry.with_context(|| format!("failed to read an entry of {}", path.display()))?;
        let Some(mbid) = recording_of(&entry.path().context("archive entry has an unreadable path")?) else {
            continue;
        };
        json.clear();
        entry
            .read_to_end(&mut json)
            .with_context(|| format!("failed to read a document from {}", path.display()))?;

        stats.submissions += 1;
        let Some(outputs) = read_outputs(&json) else {
            stats.unreadable += 1;
            continue;
        };
        stats.record(&outputs);
        match judge(&outputs) {
            Verdict::Trusted => {
                let recording = heard.entry(mbid).or_default();
                recording.heard += 1;
                // Nine sums for ten outputs: the zip stops before gender.
                for (sum, p) in recording.sums.iter_mut().zip(outputs) {
                    *sum += p;
                }
            }
            Verdict::Collapsed => stats.collapsed += 1,
            Verdict::Undecided => stats.undecided += 1,
        }
    }
    Ok((heard, stats))
}

/// Reads the tempo of every submission of the recordings already heard.
/// Returns how many rows it took.
fn read_rhythm(path: &Path, heard: &mut HashMap<Uuid, Recording>) -> Result<u64> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(file).with_context(|| format!("{} is not a zstd stream", path.display()))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().with_context(|| format!("{} is not a readable tar archive", path.display()))? {
        let entry = entry.context("failed to read an archive entry")?;
        if entry.path().ok().and_then(|p| p.extension().map(|e| e == "csv")) != Some(true) {
            continue;
        }
        return read_rhythm_csv(BufReader::with_capacity(1 << 20, entry), heard);
    }
    bail!("{} holds no CSV", path.display())
}

/// The rhythm CSV: `mbid,submission_offset,bpm,...`, with a header, CRLF
/// line ends and no quoting -- every field is a number or an MBID.
fn read_rhythm_csv<R: BufRead>(mut reader: R, heard: &mut HashMap<Uuid, Recording>) -> Result<u64> {
    let mut line = String::new();
    reader.read_line(&mut line).context("failed to read the rhythm header")?;
    // Located by name rather than assumed: a column added upstream should
    // fail loudly here, not quietly read the wrong number as a tempo.
    let header: Vec<&str> = line.trim_end().split(',').collect();
    let (Some(mbid_at), Some(bpm_at)) = (header.iter().position(|c| *c == "mbid"), header.iter().position(|c| *c == "bpm")) else {
        bail!("the rhythm CSV has no mbid or bpm column: {}", line.trim_end());
    };

    let mut taken = 0u64;
    loop {
        line.clear();
        if reader.read_line(&mut line).context("failed to read a rhythm row")? == 0 {
            break;
        }
        let fields: Vec<&str> = line.trim_end().split(',').collect();
        let (Some(mbid), Some(bpm)) = (fields.get(mbid_at), fields.get(bpm_at)) else {
            continue;
        };
        let Ok(mbid) = Uuid::parse_str(mbid) else { continue };
        let Some(recording) = heard.get_mut(&mbid) else { continue };
        // A tempo of zero is silence, or a file the extractor could not beat
        // track; neither is a tempo.
        if let Ok(bpm) = bpm.parse::<f32>()
            && bpm.is_finite()
            && bpm > 0.0
        {
            recording.tempos.push(bpm);
            taken += 1;
        }
    }
    Ok(taken)
}

/// A star's spectrum, ready to write.
#[derive(Debug, PartialEq)]
struct Spectrum {
    artist: i32,
    recordings: usize,
    tempo: f32,
    /// Mean over the recordings, in `MOODS` order.
    moods: [f32; MOODS],
}

/// Brings the recordings together into their artists.
fn spectra(heard: &HashMap<Uuid, Recording>, artists: &HashMap<Uuid, i32>, canon: &HashSet<i32>) -> Vec<Spectrum> {
    struct Gathered {
        tempos: Vec<f32>,
        sums: [f64; MOODS],
    }

    let mut gathered: HashMap<i32, Gathered> = HashMap::new();
    for (mbid, recording) in heard {
        if recording.heard == 0 || recording.tempos.is_empty() {
            continue;
        }
        let Some(&artist) = artists.get(mbid) else { continue };
        if !canon.contains(&artist) {
            continue;
        }
        let Some(tempo) = median(recording.tempos.clone()) else { continue };

        let into = gathered.entry(artist).or_insert_with(|| Gathered {
            tempos: Vec::new(),
            sums: [0.0; MOODS],
        });
        into.tempos.push(tempo);
        for (sum, recording_sum) in into.sums.iter_mut().zip(recording.sums) {
            // The recording's own mean: each recording weighs one, however
            // many times it was submitted.
            *sum += f64::from(recording_sum) / f64::from(recording.heard);
        }
    }

    let mut spectra: Vec<Spectrum> = gathered
        .into_iter()
        .filter(|(_, g)| g.tempos.len() >= MIN_RECORDINGS)
        .filter_map(|(artist, g)| {
            let recordings = g.tempos.len();
            let count = f64::from(u32::try_from(recordings).ok()?);
            #[allow(clippy::cast_possible_truncation, reason = "a mean of probabilities is a probability, well inside f32")]
            let moods = g.sums.map(|sum| (sum / count) as f32);
            Some(Spectrum {
                artist,
                recordings,
                tempo: median(g.tempos)?,
                moods,
            })
        })
        .collect();
    spectra.sort_by_key(|s| s.artist);
    spectra
}

/// The middle value, or the mean of the middle two.
fn median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        f32::midpoint(values[middle - 1], values[middle])
    } else {
        values[middle]
    })
}

async fn write(pool: &PgPool, spectra: &[Spectrum], version: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('acousticbrainz', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    // Re-importing replaces what this source owns and nothing else.
    sqlx::query("TRUNCATE artist_spectrum, spectrum_scale")
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous spectra")?;

    let mut written = 0i64;
    for chunk in spectra.chunks(BATCH) {
        let column = |index: usize| -> Vec<f32> { chunk.iter().map(|s| s.moods[index]).collect() };
        let recordings: Vec<i32> = chunk.iter().map(|s| i32::try_from(s.recordings).unwrap_or(i32::MAX)).collect();
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO artist_spectrum (artist_id, recordings, tempo, danceable, acoustic, electronic, aggressive, happy, party, relaxed, sad, voice)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::real[], $4::real[], $5::real[], $6::real[], $7::real[], $8::real[], $9::real[], $10::real[], $11::real[], $12::real[])",
        )
        .bind(chunk.iter().map(|s| s.artist).collect::<Vec<i32>>())
        .bind(recordings)
        .bind(chunk.iter().map(|s| s.tempo).collect::<Vec<f32>>())
        .bind(column(DANCEABLE))
        .bind(column(ACOUSTIC))
        .bind(column(ELECTRONIC))
        .bind(column(AGGRESSIVE))
        .bind(column(HAPPY))
        .bind(column(PARTY))
        .bind(column(RELAXED))
        .bind(column(SAD))
        .bind(column(VOICE))
        .execute(&mut *tx)
        .await
        .context("failed to write spectra")?;
    }

    // Each band's percentiles over every star just written. Computed here,
    // after the rows, from the rows -- including the bands the schema derives
    // -- so the scale is of exactly what the table holds.
    for band in BANDS {
        let sql = format!(
            "INSERT INTO spectrum_scale (band, quantiles)
             SELECT $1, (percentile_cont(ARRAY(SELECT n / {last}.0 FROM generate_series(0, {last}) AS n)::float8[])
                         WITHIN GROUP (ORDER BY {band}))::real[]
             FROM artist_spectrum
             HAVING count(*) > 0",
            last = QUANTILES - 1,
        );
        sqlx::query(AssertSqlSafe(sql))
            .bind(band)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("failed to scale the {band} band"))?;
    }

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, spectra = written, "AcousticBrainz import complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document of the dump's shape. `relaxed` and `female` are
    /// parameters: they are what the collapse checks read.
    fn document(relaxed: f32, female: f32) -> String {
        let model = |name: &str, positive: &str, p: f32| {
            format!(
                r#""{name}": {{"all": {{"{positive}": {p}, "not_{positive}": {}}}, "probability": 0.5, "value": "x"}}"#,
                1.0 - p
            )
        };
        format!(
            r#"{{"highlevel": {{{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, "genre_dortmund": {{"all": {{"rock": 1.0}}}}}}, "metadata": {{"tags": {{"artist": ["Somebody"]}}}}}}"#,
            model("danceability", "danceable", 0.1),
            model("mood_acoustic", "acoustic", 0.2),
            model("mood_electronic", "electronic", 0.3),
            model("mood_aggressive", "aggressive", 0.4),
            model("mood_happy", "happy", 0.45),
            model("mood_party", "party", 0.6),
            model("mood_relaxed", "relaxed", relaxed),
            model("mood_sad", "sad", 0.8),
            model("voice_instrumental", "voice", 0.9),
            model("gender", "female", female),
        )
    }

    fn verdict(relaxed: f32, female: f32) -> Verdict {
        judge(&read_outputs(document(relaxed, female).as_bytes()).expect("a readable document"))
    }

    #[test]
    fn reads_the_outputs_in_their_fixed_order() {
        let outputs = read_outputs(document(0.7, 0.3).as_bytes()).expect("an ordinary document is read");
        assert_eq!(outputs, [0.1, 0.2, 0.3, 0.4, 0.45, 0.6, 0.7, 0.8, 0.9, 0.3]);
        assert_eq!(judge(&outputs), Verdict::Trusted);
    }

    #[test]
    fn a_collapsed_output_condemns_the_whole_submission() {
        assert_eq!(verdict(0.808_82, 0.3), Verdict::Collapsed, "relaxed at its collapse");
        assert_eq!(verdict(0.7, 0.622_13), Verdict::Collapsed, "gender at its collapse");
        // The trail either side of the value is part of the collapse.
        assert_eq!(verdict(0.808_88, 0.3), Verdict::Collapsed);
        let mut sad = [0.3; OUTPUTS];
        sad[SAD] = 0.352_21;
        assert_eq!(judge(&sad), Verdict::Collapsed, "sad at its collapse");
    }

    #[test]
    fn a_value_near_but_outside_the_collapse_is_music() {
        assert_eq!(verdict(0.8086, 0.3), Verdict::Trusted);
        assert_eq!(verdict(0.7, 0.6225), Verdict::Trusted);
    }

    #[test]
    fn a_model_answering_exactly_one_half_has_not_answered() {
        assert_eq!(verdict(0.5, 0.3), Verdict::Undecided);
        assert_eq!(verdict(0.7, 0.5), Verdict::Undecided, "gender's non-answer counts too: it is the same analysis");
        assert_eq!(verdict(0.500_01, 0.3), Verdict::Trusted, "a near half is an answer");
    }

    #[test]
    fn a_document_without_its_models_is_unreadable_not_silent() {
        assert_eq!(read_outputs(br#"{"metadata": {}}"#), None);
        assert_eq!(read_outputs(br#"{"highlevel": null}"#), None);
        assert_eq!(read_outputs(br#"{"highlevel": {"danceability": {"all": {"danceable": 0.4}}}}"#), None);
        assert_eq!(read_outputs(b"Creative Commons Legal Code"), None);
        assert_eq!(read_outputs(document(1.5, 0.3).as_bytes()), None, "not a probability");
    }

    #[test]
    fn a_spike_among_smooth_neighbours_is_found_and_a_smooth_hill_is_not() {
        let mut histogram = vec![0u32; BINS];
        // A smooth hill around 0.3: every bin a little above the last.
        for (offset, bin) in histogram[29_000..31_000].iter_mut().enumerate() {
            *bin = 100 + u32::try_from(offset.min(2_000 - offset)).unwrap();
        }
        // A collapse at 0.70000, with the trail it leaves beside it.
        histogram[70_000] = 50_000;
        histogram[69_999] = 300;
        histogram[70_001] = 200;
        let found = spikes(&histogram, 1_000_000);
        assert_eq!(found, vec![(70_000, 50_000)]);
    }

    #[test]
    fn a_known_collapse_is_explained_and_an_unknown_one_is_reported() {
        let mut stats = ReadStats {
            submissions: 1_000_000,
            ..ReadStats::default()
        };
        stats.histograms[RELAXED][bin(0.808_82)] = 50_000;
        stats.histograms[HAPPY][bin(0.25)] = 50_000;
        assert_eq!(unexplained(&stats), vec![("happy", 0.25, 50_000)]);
    }

    #[test]
    fn the_ends_and_the_exact_half_are_not_spikes() {
        let mut histogram = vec![0u32; BINS];
        histogram[0] = 900_000;
        histogram[BINS - 1] = 900_000;
        histogram[BINS / 2] = 900_000;
        assert_eq!(spikes(&histogram, 1_000_000), Vec::new());
    }

    #[test]
    fn the_recording_comes_from_the_path() {
        let path = Path::new("acousticbrainz-highlevel-json-20220623/highlevel/0e/1/0e11c0fd-a1da-4b88-a438-7ef55c5809ec-12.json");
        assert_eq!(recording_of(path), Some(Uuid::parse_str("0e11c0fd-a1da-4b88-a438-7ef55c5809ec").unwrap()));
        assert_eq!(recording_of(Path::new("acousticbrainz-highlevel-json-20220623/COPYING")), None);
    }

    #[test]
    fn names_the_files_of_a_dump() {
        assert_eq!(highlevel_name("acousticbrainz-highlevel-json-20220623-17.tar.zst"), Some(("20220623", 17)));
        assert_eq!(highlevel_name("acousticbrainz-highlevel-json-20220623-17.tar.zst.part"), None);
        assert_eq!(rhythm_name("acousticbrainz-lowlevel-features-20220623-rhythm.tar.zst"), Some("20220623"));
        assert_eq!(rhythm_name("acousticbrainz-lowlevel-features-20220623-tonal.tar.zst"), None);
    }

    #[test]
    fn a_gap_in_the_archives_refuses_the_import() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "acousticbrainz-highlevel-json-20220623-0.tar.zst",
            "acousticbrainz-highlevel-json-20220623-2.tar.zst",
            "acousticbrainz-lowlevel-features-20220623-rhythm.tar.zst",
        ] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let error = Dumps::find(dir.path()).err().expect("archive 1 is missing");
        assert!(error.to_string().contains("archive 1 is missing"), "got: {error}");

        std::fs::write(dir.path().join("acousticbrainz-highlevel-json-20220623-1.tar.zst"), b"").unwrap();
        let dumps = Dumps::find(dir.path()).expect("0, 1, 2 is whole");
        assert_eq!(dumps.highlevel.len(), 3);
        assert_eq!(dumps.version, "20220623");
    }

    #[test]
    fn tempo_is_read_by_column_name_for_heard_recordings_only() {
        let kept = Uuid::from_u128(1);
        let unheard = Uuid::from_u128(2);
        let mut heard = HashMap::from([(kept, Recording::default())]);
        let csv = format!("mbid,submission_offset,bpm,danceability\r\n{kept},0,120.5,1.0\r\n{kept},1,0,1.0\r\n{unheard},0,90,1.0\r\n{kept},2,122.5,1.0\r\n");
        let taken = read_rhythm_csv(csv.as_bytes(), &mut heard).unwrap();
        assert_eq!(taken, 2, "the zero tempo is not a tempo");
        assert_eq!(heard[&kept].tempos, vec![120.5, 122.5]);
        assert!(!heard.contains_key(&unheard));
    }

    #[test]
    fn a_rhythm_csv_without_a_bpm_column_is_refused() {
        let mut heard = HashMap::new();
        assert!(read_rhythm_csv("mbid,submission_offset,tempo\r\n".as_bytes(), &mut heard).is_err());
    }

    fn recording(heard: u16, mean: f32, tempos: &[f32]) -> Recording {
        Recording {
            heard: u32::from(heard),
            sums: [mean * f32::from(heard); MOODS],
            tempos: tempos.to_vec(),
        }
    }

    #[test]
    fn every_recording_weighs_one_however_often_it_was_submitted() {
        let artist = 7;
        let heard = HashMap::from([
            // A hit submitted forty times, all saying 1.0 ...
            (Uuid::from_u128(1), recording(40, 1.0, &[100.0])),
            // ... and two songs submitted once each, saying 0.0.
            (Uuid::from_u128(2), recording(1, 0.0, &[110.0])),
            (Uuid::from_u128(3), recording(1, 0.0, &[120.0])),
        ]);
        let artists: HashMap<Uuid, i32> = heard.keys().map(|&mbid| (mbid, artist)).collect();
        let spectra = spectra(&heard, &artists, &HashSet::from([artist]));
        assert_eq!(spectra.len(), 1);
        let moods = spectra[0].moods;
        assert!(
            (moods[HAPPY] - 1.0 / 3.0).abs() < 1e-6,
            "three recordings, one of them happy: got {}",
            moods[HAPPY]
        );
        assert_eq!(spectra[0].recordings, 3);
    }

    #[test]
    fn tempo_is_the_median_of_each_recordings_median() {
        let artist = 7;
        let heard = HashMap::from([
            // One octave error among three submissions does not move it.
            (Uuid::from_u128(1), recording(1, 0.5, &[120.0, 240.0, 121.0])),
            (Uuid::from_u128(2), recording(1, 0.5, &[90.0])),
            (Uuid::from_u128(3), recording(1, 0.5, &[180.0])),
        ]);
        let artists: HashMap<Uuid, i32> = heard.keys().map(|&mbid| (mbid, artist)).collect();
        let spectra = spectra(&heard, &artists, &HashSet::from([artist]));
        assert_eq!(spectra[0].tempo, 121.0);
    }

    #[test]
    fn too_few_recordings_or_a_star_outside_the_canon_gives_no_spectrum() {
        let heard = HashMap::from([
            (Uuid::from_u128(1), recording(1, 0.5, &[100.0])),
            (Uuid::from_u128(2), recording(1, 0.5, &[100.0])),
            (Uuid::from_u128(3), recording(1, 0.5, &[100.0])),
            (Uuid::from_u128(4), recording(1, 0.5, &[100.0])),
            (Uuid::from_u128(5), recording(1, 0.5, &[100.0])),
        ]);
        // Two recordings for artist 1, three for artist 2 who is not a star.
        let artists = HashMap::from([
            (Uuid::from_u128(1), 1),
            (Uuid::from_u128(2), 1),
            (Uuid::from_u128(3), 2),
            (Uuid::from_u128(4), 2),
            (Uuid::from_u128(5), 2),
        ]);
        assert_eq!(spectra(&heard, &artists, &HashSet::from([1])), Vec::new());
        assert_eq!(
            spectra(&heard, &artists, &HashSet::from([1, 2])).len(),
            1,
            "artist 2 has enough once in the canon"
        );
    }

    #[test]
    fn a_recording_without_a_trusted_mood_is_not_counted() {
        let artist = 7;
        let heard = HashMap::from([
            (Uuid::from_u128(1), recording(1, 0.5, &[100.0])),
            (Uuid::from_u128(2), recording(1, 0.5, &[100.0])),
            // Tempo, but every submission collapsed.
            (Uuid::from_u128(3), recording(0, 0.0, &[100.0])),
        ]);
        let artists: HashMap<Uuid, i32> = heard.keys().map(|&mbid| (mbid, artist)).collect();
        assert!(
            spectra(&heard, &artists, &HashSet::from([artist])).is_empty(),
            "two measured recordings are under the floor"
        );
    }

    #[test]
    fn the_median_of_an_even_count_is_the_middle_pair_averaged() {
        assert_eq!(median(vec![4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(Vec::new()), None);
    }

    #[test]
    fn the_floor_matches_the_schema() {
        let migration = include_str!("../../migrations/0012_spectrum.sql");
        assert!(
            migration.contains(&format!("CHECK (recordings >= {MIN_RECORDINGS})")),
            "the importer's floor and the table's check must be one number"
        );
    }
}
