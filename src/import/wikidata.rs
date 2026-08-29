//! Imports biographical facts and influence links from the Wikidata JSON dump.
//!
//! Wikidata is the only bulk source for two things the sky needs and neither
//! MusicBrainz nor Discogs carries: **who influenced whom** (MusicBrainz has no
//! such relationship at all) and **where an act actually formed** (MusicBrainz
//! records a country, Wikidata a city).
//!
//! The dump is one bz2 stream holding a JSON array with **one entity per
//! line**, about 103 GB compressed and 25 million entities. It is read in a
//! single streaming pass and never stored: measured against the live dump at
//! about 700 entities a second, a full pass takes ten hours, and nothing but
//! the extracted facts survives it.
//!
//! Two things make one pass sufficient:
//!
//! - **Facts are references, not words.** Place of birth is `Q24826`, not
//!   "Liverpool", so the labels for referenced items have to come from
//!   somewhere. Keeping every label as it goes by does not fit: measured on a
//!   full run, 25 million labels cost 3.1 GB at 27% of the file, which
//!   extrapolates to 10-11 GB -- more memory than this machine has. (An
//!   earlier note here claimed half a gigabyte; that number came from a
//!   400,000-entity `--limit` run and did not survive contact with the whole
//!   dump.)
//!
//!   So the file is read twice. The first pass collects facts and notes which
//!   item ids they point at -- tens of thousands, not tens of millions. The
//!   second reads labels for exactly those. Two passes over a local file cost
//!   time, which is cheap here; the alternative costs memory the machine does
//!   not have, and fails on the ninth hour.
//! - **The enwiki article title is taken now**, while the dump is open, because
//!   the prose import needs it and it exists nowhere else in the canon.
//!
//! **Having a MusicBrainz id does not make an entity a musician.** The first
//! `P434` in the dump belongs to a 17th-century painter with music written
//! about him. The filter that matters is the canon itself: only entities whose
//! MBID is already an artist here are kept.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream property and file names throughout")]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bzip2::read::MultiBzDecoder;
use clap::Args as ClapArgs;
use serde::Deserialize;
use sqlx::PgPool;

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

/// Properties read out of each entity. Named rather than inlined so the code
/// reads as the facts it collects instead of as P-numbers.
const P_MBID: &str = "P434";
const P_INFLUENCED_BY: &str = "P737";
const P_FORMATION_PLACE: &str = "P740";
const P_BIRTH_PLACE: &str = "P19";
const P_INCEPTION: &str = "P571";
const P_GENRE: &str = "P136";
const P_LABEL: &str = "P264";
const P_COUNTRY: &str = "P495";

/// The official dump, used when no source is given.
const DEFAULT_URL: &str = "https://dumps.wikimedia.org/wikidatawiki/entities/latest-all.json.bz2";

#[derive(ClapArgs)]
pub struct Args {
    /// Path to latest-all.json.bz2 from dumps.wikimedia.org. Mutually
    /// exclusive with --url.
    #[arg(long, value_name = "FILE", conflicts_with = "url")]
    pub dump: Option<PathBuf>,

    /// Read the dump straight from this URL instead of a local file, so the
    /// 103 GB never touches disk. Defaults to the official latest-all dump
    /// when neither --dump nor --url is given.
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// The dump version, recorded so the database can say which vintage of
    /// facts it holds. The `latest-` dumps carry no version in their name, so
    /// this defaults to `latest`.
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,

    /// Stop after this many entities. For trying the pipeline against the real
    /// dump without spending ten hours on it.
    #[arg(long, value_name = "N")]
    pub limit: Option<u64>,
}

/// One entity, decoded only as far as the fact pass cares about.
///
/// No `labels` here: naming the items facts point at is the second pass's job,
/// and a field decoded only to be dropped is paid for 123 million times.
#[derive(Deserialize)]
struct Entity {
    id: String,
    #[serde(default)]
    claims: HashMap<String, Vec<Statement>>,
    #[serde(default)]
    sitelinks: HashMap<String, Sitelink>,
}

/// One entity as the label pass sees it.
///
/// Separate from `Entity` on purpose: `claims` and `sitelinks` are almost the
/// whole weight of a line, and the second pass wants neither. Skipping them
/// keeps a 123-million-entity pass from paying for data it discards.
#[derive(Deserialize)]
struct LabelOnly {
    id: String,
    #[serde(default)]
    labels: HashMap<String, LabelValue>,
}

#[derive(Deserialize)]
struct LabelValue {
    value: String,
}

#[derive(Deserialize)]
struct Sitelink {
    title: String,
}

#[derive(Deserialize)]
struct Statement {
    mainsnak: Snak,
}

#[derive(Deserialize)]
struct Snak {
    #[serde(default)]
    datavalue: Option<DataValue>,
}

#[derive(Deserialize)]
struct DataValue {
    value: serde_json::Value,
}

impl DataValue {
    /// The Q-number this value points at, for `wikibase-entityid` values.
    fn entity_qid(&self) -> Option<i32> {
        self.value.get("id").and_then(serde_json::Value::as_str).and_then(qid_number)
    }

    /// The plain string of an external-id or string value.
    fn string(&self) -> Option<&str> {
        self.value.as_str()
    }

    /// The year of a `time` value.
    ///
    /// Wikidata writes times as `+1991-09-24T00:00:00Z`, and the leading sign
    /// is part of the format rather than an oddity: years before the common
    /// era are negative, and a parser that assumes a digit first reads 1991 as
    /// nothing.
    fn year(&self) -> Option<i16> {
        let time = self.value.get("time")?.as_str()?;
        let (sign, rest) = time.split_at(1);
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let year: i32 = digits.parse().ok()?;
        let signed = if sign == "-" { -year } else { year };
        i16::try_from(signed).ok()
    }
}

/// `Q11649` -> `11649`. Anything else -- lexemes (`L…`), properties (`P…`),
/// forms and senses -- is not an item and yields nothing.
fn qid_number(id: &str) -> Option<i32> {
    id.strip_prefix('Q')?.parse().ok()
}

/// What one pass over the dump collects.
#[derive(Default)]
struct Harvest {
    /// MBID -> the facts found for it.
    by_mbid: HashMap<String, Facts>,
    /// Every English label seen, so referenced items can be named without a
    /// second pass. Trimmed to the referenced ones before writing.
    labels: HashMap<i32, String>,
    /// Entities read, for the progress line.
    entities: u64,
    /// Entities carrying a MusicBrainz id -- most of which are not musicians.
    with_mbid: u64,
}

/// Everything worth keeping about one Wikidata item.
#[derive(Default)]
struct Facts {
    qid: i32,
    enwiki_title: Option<String>,
    origin_qid: Option<i32>,
    origin_is_birth: bool,
    inception_year: Option<i16>,
    country_qid: Option<i32>,
    genres: Vec<i32>,
    labels: Vec<i32>,
    /// Q-numbers of the items this one was influenced by, resolved to artists
    /// after the pass: the influencing entity may not have been read yet.
    influenced_by: Vec<i32>,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    // The canon decides who counts. Without artists there is nothing to attach
    // a biography to, and every fact in the dump would be dropped.
    let canon = load_canon(pool).await?;
    if canon.is_empty() {
        bail!("no artists in the canon: run `lyrid import musicbrainz` first");
    }
    tracing::info!(artists = canon.len(), "resolving Wikidata against the canon");

    let harvest = read_dump(args, &canon)?;

    let version = args.version.clone().unwrap_or_else(|| "latest".to_string());
    tracing::info!(
        entities = harvest.entities,
        with_mbid = harvest.with_mbid,
        matched = harvest.by_mbid.len(),
        "dump read; writing to PostgreSQL"
    );

    write(pool, &canon, &harvest, &version).await
}

/// MBID -> artist id, for every artist in the canon.
async fn load_canon(pool: &PgPool) -> Result<HashMap<String, i32>> {
    let rows: Vec<(uuid::Uuid, i32)> = sqlx::query_as("SELECT mbid, id FROM artist")
        .fetch_all(pool)
        .await
        .context("failed to read the canon")?;
    Ok(rows.into_iter().map(|(mbid, id)| (mbid.to_string(), id)).collect())
}

/// Opens the dump, from a file or straight from the network.
fn open_dump(args: &Args) -> Result<Box<dyn BufRead>> {
    if let Some(path) = &args.dump {
        let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let size = file.metadata().map_or(0, |m| m.len());
        tracing::info!(dump = %path.display(), size_gb = size / 1_073_741_824, "reading the Wikidata dump");
        let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 22, file));
        return Ok(Box::new(BufReader::with_capacity(1 << 22, decoder)));
    }

    let url = args.url.as_deref().unwrap_or(DEFAULT_URL);
    tracing::info!(url, "streaming the Wikidata dump; nothing is written to disk");
    let response = ureq::get(url).call().with_context(|| format!("cannot fetch {url}"))?;
    let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 22, response.into_body().into_reader()));
    Ok(Box::new(BufReader::with_capacity(1 << 22, decoder)))
}

/// Reads the dump: facts first, then the labels those facts refer to.
fn read_dump(args: &Args, canon: &HashMap<String, i32>) -> Result<Harvest> {
    let mut harvest = read_facts(args, canon)?;
    harvest.labels = read_labels(args, &wanted_qids(&harvest))?;
    Ok(harvest)
}

/// First pass: every fact this import keeps, and nothing else.
fn read_facts(args: &Args, canon: &HashMap<String, i32>) -> Result<Harvest> {
    let reader = open_dump(args)?;
    let mut harvest = Harvest::default();

    for line in reader.lines() {
        let line = line.context("failed to read a dump line")?;
        harvest.entities += take_line(&line, canon, &mut harvest.by_mbid, &mut harvest.with_mbid);

        if harvest.entities % 1_000_000 == 0 && harvest.entities > 0 {
            tracing::info!(entities = harvest.entities, matched = harvest.by_mbid.len(), "still reading facts");
        }
        if args.limit.is_some_and(|limit| harvest.entities >= limit) {
            tracing::info!(entities = harvest.entities, "stopping at the requested limit");
            break;
        }
    }

    tracing::info!(entities = harvest.entities, matched = harvest.by_mbid.len(), "facts read");
    Ok(harvest)
}

/// Which item ids the harvested facts point at and therefore need naming.
///
/// The artists themselves are named by MusicBrainz, so only the things facts
/// *refer to* are wanted: places, countries, genres, record labels. Influence
/// targets are resolved through the canon rather than by label, but their
/// q-numbers are cheap to carry and one missing name is worse than one extra.
fn wanted_qids(harvest: &Harvest) -> HashSet<i32> {
    let mut wanted = HashSet::new();
    for facts in harvest.by_mbid.values() {
        wanted.extend(facts.origin_qid);
        wanted.extend(facts.country_qid);
        wanted.extend(facts.genres.iter().copied());
        wanted.extend(facts.labels.iter().copied());
    }
    wanted
}

/// Second pass: the English label of each wanted item, and no others.
///
/// This is why the dump is read twice rather than held in memory. The wanted
/// set is tens of thousands of items; every label in the file is tens of
/// millions, which does not fit.
fn read_labels(args: &Args, wanted: &HashSet<i32>) -> Result<HashMap<i32, String>> {
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }

    tracing::info!(wanted = wanted.len(), "reading labels for the items the facts point at");
    let reader = open_dump(args)?;
    let mut labels = HashMap::with_capacity(wanted.len());
    let mut entities = 0u64;

    for line in reader.lines() {
        let line = line.context("failed to read a dump line")?;
        entities += take_label(&line, wanted, &mut labels);

        if entities > 0 && entities.is_multiple_of(5_000_000) {
            tracing::info!(entities, labels = labels.len(), "still reading labels");
        }
        // Every wanted label found: the rest of the file has nothing left to
        // give, and stopping saves hours on a run whose references happen to
        // sit early in the file.
        if labels.len() == wanted.len() {
            tracing::info!(labels = labels.len(), "every wanted label found");
            break;
        }
        if args.limit.is_some_and(|limit| entities >= limit) {
            break;
        }
    }

    tracing::info!(labels = labels.len(), wanted = wanted.len(), "labels read");
    Ok(labels)
}

/// Handles one line of the label pass, returning how many entities it held.
///
/// Deliberately does not parse the whole entity: a label pass that paid full
/// deserialisation for 123 million items would cost more than the memory it
/// saves. The id and the English label are enough.
fn take_label(line: &str, wanted: &HashSet<i32>, labels: &mut HashMap<i32, String>) -> u64 {
    let trimmed = line.trim().trim_end_matches(',');
    if trimmed.is_empty() || trimmed == "[" || trimmed == "]" {
        return 0;
    }
    let Ok(entity) = serde_json::from_str::<LabelOnly>(trimmed) else {
        return 0;
    };
    let Some(qid) = qid_number(&entity.id) else {
        return 1;
    };
    if wanted.contains(&qid)
        && let Some(label) = entity.labels.get("en")
    {
        labels.insert(qid, label.value.clone());
    }
    1
}

/// Handles one line of the dump, returning how many entities it contained
/// (one, or zero for the array brackets and unparseable lines).
///
/// Separate from the reading loop so the tests exercise the shipped rules
/// rather than a reimplementation of them.
fn take_line(line: &str, canon: &HashMap<String, i32>, by_mbid: &mut HashMap<String, Facts>, with_mbid: &mut u64) -> u64 {
    // The file is a JSON array: the first and last lines are its brackets, and
    // every entity line carries a trailing comma.
    let trimmed = line.trim().trim_end_matches(',');
    if trimmed.is_empty() || trimmed == "[" || trimmed == "]" {
        return 0;
    }

    // A malformed line is upstream data, not a reason to abandon a pass that
    // takes ten hours.
    let Ok(entity) = serde_json::from_str::<Entity>(trimmed) else {
        return 0;
    };

    let Some(qid) = qid_number(&entity.id) else {
        // Lexemes and properties share the file with items; they are counted
        // as read but carry nothing this import wants.
        return 1;
    };

    let Some(mbid_statements) = entity.claims.get(P_MBID) else {
        return 1;
    };
    *with_mbid += 1;

    // The canon is the real filter: a painter with a MusicBrainz id is not a
    // star in this sky.
    let Some(mbid) = mbid_statements
        .iter()
        .filter_map(|s| s.mainsnak.datavalue.as_ref())
        .filter_map(DataValue::string)
        .find(|mbid| canon.contains_key(*mbid))
    else {
        return 1;
    };

    let mut facts = Facts {
        qid,
        enwiki_title: entity.sitelinks.get("enwiki").map(|s| s.title.clone()),
        ..Facts::default()
    };

    // Where the act comes from. A group's formation place answers this better
    // than a person's birthplace, so it wins when both are present.
    if let Some(place) = first_entity(&entity, P_FORMATION_PLACE) {
        facts.origin_qid = Some(place);
        facts.origin_is_birth = false;
    } else if let Some(place) = first_entity(&entity, P_BIRTH_PLACE) {
        facts.origin_qid = Some(place);
        facts.origin_is_birth = true;
    }

    facts.country_qid = first_entity(&entity, P_COUNTRY);
    facts.inception_year = entity
        .claims
        .get(P_INCEPTION)
        .and_then(|statements| statements.first())
        .and_then(|s| s.mainsnak.datavalue.as_ref())
        .and_then(DataValue::year);
    facts.genres = all_entities(&entity, P_GENRE);
    facts.labels = all_entities(&entity, P_LABEL);
    facts.influenced_by = all_entities(&entity, P_INFLUENCED_BY);

    by_mbid.insert(mbid.to_string(), facts);
    1
}

/// The first item a property points at.
fn first_entity(entity: &Entity, property: &str) -> Option<i32> {
    entity
        .claims
        .get(property)?
        .iter()
        .filter_map(|s| s.mainsnak.datavalue.as_ref())
        .find_map(DataValue::entity_qid)
}

/// Every item a property points at.
fn all_entities(entity: &Entity, property: &str) -> Vec<i32> {
    entity
        .claims
        .get(property)
        .map(|statements| {
            statements
                .iter()
                .filter_map(|s| s.mainsnak.datavalue.as_ref())
                .filter_map(DataValue::entity_qid)
                .collect()
        })
        .unwrap_or_default()
}

/// Writes everything in one transaction.
async fn write(pool: &PgPool, canon: &HashMap<String, i32>, harvest: &Harvest, version: &str) -> Result<()> {
    // artist id -> facts, resolved from the MBIDs the pass keyed on.
    let resolved: Vec<(i32, &Facts)> = harvest
        .by_mbid
        .iter()
        .filter_map(|(mbid, facts)| canon.get(mbid).map(|artist| (*artist, facts)))
        .collect();

    // Influence links two artists, so the far end has to be looked up by its
    // Q-number.
    let artist_of_qid: HashMap<i32, i32> = resolved.iter().map(|(artist, facts)| (facts.qid, *artist)).collect();

    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('wikidata', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    sqlx::query("TRUNCATE artist_wikidata, artist_fact, artist_wikidata_genre, artist_wikidata_label, artist_influence, wikidata_item CASCADE")
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous Wikidata facts")?;

    let mut written: i64 = 0;
    written += write_links(&mut tx, &resolved).await?;
    written += write_facts(&mut tx, &resolved).await?;
    written += write_multi(&mut tx, &resolved).await?;
    written += write_influence(&mut tx, &resolved, &artist_of_qid).await?;
    written += write_item_labels(&mut tx, &resolved, harvest).await?;

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, rows = written, "Wikidata import complete");
    Ok(())
}

async fn write_links(tx: &mut sqlx::PgTransaction<'_>, resolved: &[(i32, &Facts)]) -> Result<i64> {
    let mut written = 0i64;
    for chunk in resolved.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let qids: Vec<i32> = chunk.iter().map(|(_, f)| f.qid).collect();
        let titles: Vec<Option<&str>> = chunk.iter().map(|(_, f)| f.enwiki_title.as_deref()).collect();

        sqlx::query(
            "INSERT INTO artist_wikidata (artist_id, qid, enwiki_title)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::text[])
             ON CONFLICT (artist_id) DO NOTHING",
        )
        .bind(&artists)
        .bind(&qids)
        .bind(&titles)
        .execute(&mut **tx)
        .await
        .context("failed to write Wikidata links")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "Wikidata links written");
    Ok(written)
}

async fn write_facts(tx: &mut sqlx::PgTransaction<'_>, resolved: &[(i32, &Facts)]) -> Result<i64> {
    let mut written = 0i64;
    for chunk in resolved.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let origins: Vec<Option<i32>> = chunk.iter().map(|(_, f)| f.origin_qid).collect();
        // NULL rather than false where there is no origin at all: "not a
        // birthplace" would be a claim, and there is nothing to claim it about.
        let is_birth: Vec<Option<bool>> = chunk.iter().map(|(_, f)| f.origin_qid.map(|_| f.origin_is_birth)).collect();
        let years: Vec<Option<i16>> = chunk.iter().map(|(_, f)| f.inception_year).collect();
        let countries: Vec<Option<i32>> = chunk.iter().map(|(_, f)| f.country_qid).collect();

        sqlx::query(
            "INSERT INTO artist_fact (artist_id, origin_qid, origin_is_birth, inception_year, country_qid)
             SELECT * FROM UNNEST($1::int[], $2::int[], $3::bool[], $4::smallint[], $5::int[])
             ON CONFLICT (artist_id) DO NOTHING",
        )
        .bind(&artists)
        .bind(&origins)
        .bind(&is_birth)
        .bind(&years)
        .bind(&countries)
        .execute(&mut **tx)
        .await
        .context("failed to write artist facts")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "artist facts written");
    Ok(written)
}

/// Genres and labels: the two many-valued facts, written the same way.
async fn write_multi(tx: &mut sqlx::PgTransaction<'_>, resolved: &[(i32, &Facts)]) -> Result<i64> {
    let genre_rows: Vec<(i32, i32)> = resolved
        .iter()
        .flat_map(|(artist, facts)| facts.genres.iter().map(move |qid| (*artist, *qid)))
        .collect();
    let label_rows: Vec<(i32, i32)> = resolved
        .iter()
        .flat_map(|(artist, facts)| facts.labels.iter().map(move |qid| (*artist, *qid)))
        .collect();

    // Written as two literal statements rather than one generated from a table
    // name: sqlx rejects dynamic SQL strings on purpose, and the duplication is
    // cheaper than teaching a helper to be safe.
    let mut written = 0i64;
    for chunk in genre_rows.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let qids: Vec<i32> = chunk.iter().map(|(_, q)| *q).collect();

        sqlx::query(
            "INSERT INTO artist_wikidata_genre (artist_id, genre_qid)
             SELECT * FROM UNNEST($1::int[], $2::int[])
             ON CONFLICT DO NOTHING",
        )
        .bind(&artists)
        .bind(&qids)
        .execute(&mut **tx)
        .await
        .context("failed to write Wikidata genres")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }

    for chunk in label_rows.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let qids: Vec<i32> = chunk.iter().map(|(_, q)| *q).collect();

        sqlx::query(
            "INSERT INTO artist_wikidata_label (artist_id, label_qid)
             SELECT * FROM UNNEST($1::int[], $2::int[])
             ON CONFLICT DO NOTHING",
        )
        .bind(&artists)
        .bind(&qids)
        .execute(&mut **tx)
        .await
        .context("failed to write Wikidata labels")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "Wikidata genres and labels written");
    Ok(written)
}

/// Influence edges, keeping only the ones whose far end is also in the canon.
async fn write_influence(tx: &mut sqlx::PgTransaction<'_>, resolved: &[(i32, &Facts)], artist_of_qid: &HashMap<i32, i32>) -> Result<i64> {
    let mut dropped = 0u64;
    let mut rows: Vec<(i32, i32)> = Vec::new();
    for (artist, facts) in resolved {
        for qid in &facts.influenced_by {
            match artist_of_qid.get(qid) {
                // An influence pointing at a painter or a novelist is true and
                // undrawable; it is dropped rather than stored dangling.
                None => dropped += 1,
                // Wikidata does contain self-influence typos.
                Some(other) if other == artist => dropped += 1,
                Some(other) => rows.push((*artist, *other)),
            }
        }
    }

    let mut written = 0i64;
    for chunk in rows.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _)| *a).collect();
        let influences: Vec<i32> = chunk.iter().map(|(_, i)| *i).collect();

        sqlx::query(
            "INSERT INTO artist_influence (artist_id, influence_id)
             SELECT * FROM UNNEST($1::int[], $2::int[])
             ON CONFLICT DO NOTHING",
        )
        .bind(&artists)
        .bind(&influences)
        .execute(&mut **tx)
        .await
        .context("failed to write influence edges")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, dropped_outside_canon = dropped, "influence edges written");
    Ok(written)
}

/// Labels for the items the facts actually point at, and nothing else: the
/// pass collected every label it saw, but storing 25 million of them to name a
/// few thousand cities would be absurd.
async fn write_item_labels(tx: &mut sqlx::PgTransaction<'_>, resolved: &[(i32, &Facts)], harvest: &Harvest) -> Result<i64> {
    let mut referenced: HashSet<i32> = HashSet::new();
    for (_, facts) in resolved {
        referenced.extend(facts.origin_qid);
        referenced.extend(facts.country_qid);
        referenced.extend(facts.genres.iter().copied());
        referenced.extend(facts.labels.iter().copied());
    }

    let rows: Vec<(i32, Option<&str>)> = referenced.iter().map(|qid| (*qid, harvest.labels.get(qid).map(String::as_str))).collect();

    let mut written = 0i64;
    for chunk in rows.chunks(BATCH) {
        let qids: Vec<i32> = chunk.iter().map(|(q, _)| *q).collect();
        let labels: Vec<Option<&str>> = chunk.iter().map(|(_, l)| *l).collect();

        sqlx::query(
            "INSERT INTO wikidata_item (qid, label)
             SELECT * FROM UNNEST($1::int[], $2::text[])
             ON CONFLICT (qid) DO UPDATE SET label = EXCLUDED.label",
        )
        .bind(&qids)
        .bind(&labels)
        .execute(&mut **tx)
        .await
        .context("failed to write item labels")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "item labels written");
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MBID_A: &str = "5b11f4ce-a62d-471e-81fc-a69a8278c7da";
    const MBID_B: &str = "9282c8b4-ca0b-4c6b-b7e3-4f7762dfc4d6";

    fn canon() -> HashMap<String, i32> {
        HashMap::from([(MBID_A.to_string(), 1), (MBID_B.to_string(), 2)])
    }

    /// Runs lines through the fact pass's own rules.
    fn take(lines: &[&str]) -> (HashMap<String, Facts>, u64, u64) {
        let canon = canon();
        let (mut by_mbid, mut with_mbid, mut read) = (HashMap::new(), 0, 0);
        for line in lines {
            read += take_line(line, &canon, &mut by_mbid, &mut with_mbid);
        }
        (by_mbid, with_mbid, read)
    }

    /// Runs lines through the label pass, for a given set of wanted items.
    fn take_labels(lines: &[&str], wanted: &[i32]) -> HashMap<i32, String> {
        let wanted: HashSet<i32> = wanted.iter().copied().collect();
        let mut labels = HashMap::new();
        for line in lines {
            take_label(line, &wanted, &mut labels);
        }
        labels
    }

    /// A minimal entity in the dump's own shape.
    fn entity(qid: &str, body: &str) -> String {
        format!(r#"{{"type":"item","id":"{qid}",{body}}},"#)
    }

    fn mbid_claim(mbid: &str) -> String {
        format!(r#""P434":[{{"mainsnak":{{"datavalue":{{"value":"{mbid}","type":"string"}}}}}}]"#)
    }

    fn item_claim(property: &str, qid: i32) -> String {
        format!(
            r#""{property}":[{{"mainsnak":{{"datavalue":{{"value":{{"entity-type":"item","numeric-id":{qid},"id":"Q{qid}"}},"type":"wikibase-entityid"}}}}}}]"#
        )
    }

    #[test]
    fn skips_the_arrays_brackets() {
        let (facts, _, read) = take(&["[", "]", "  "]);
        assert!(facts.is_empty());
        assert_eq!(read, 0);
    }

    #[test]
    fn keeps_only_entities_the_canon_knows() {
        // Both carry a MusicBrainz id; only one is an artist here. This is the
        // painter case: having an MBID is not being a musician.
        let line_known = entity("Q1", &format!(r#""claims":{{{}}}"#, mbid_claim(MBID_A)));
        let line_stranger = entity("Q2", &format!(r#""claims":{{{}}}"#, mbid_claim("00000000-0000-0000-0000-000000000000")));

        let (facts, with_mbid, read) = take(&[&line_known, &line_stranger]);
        assert_eq!(read, 2);
        assert_eq!(with_mbid, 2, "both had an MBID");
        assert_eq!(facts.len(), 1, "only the canonical one was kept");
        assert!(facts.contains_key(MBID_A));
    }

    #[test]
    fn names_a_wanted_item_and_ignores_the_rest() {
        // A city is nobody's artist but is somebody's birthplace, so its name
        // is wanted. The other city is referenced by nothing, and keeping it
        // is what cost 10 GB before the pass was split in two.
        let wanted_city = entity("Q24826", r#""labels":{"en":{"language":"en","value":"Liverpool"}}"#);
        let other_city = entity("Q1297", r#""labels":{"en":{"language":"en","value":"Chicago"}}"#);

        let labels = take_labels(&[&wanted_city, &other_city], &[24826]);
        assert_eq!(labels.get(&24826).map(String::as_str), Some("Liverpool"));
        assert_eq!(labels.len(), 1, "an unreferenced item should not be named");
    }

    #[test]
    fn wants_the_items_the_facts_point_at() {
        // What the second pass has to look for: the places, country, genres
        // and record labels the harvested facts refer to -- and nothing for
        // the artists themselves, which MusicBrainz already names.
        let mut harvest = Harvest::default();
        harvest.by_mbid.insert(
            MBID_A.to_string(),
            Facts {
                qid: 11649,
                origin_qid: Some(24826),
                country_qid: Some(145),
                genres: vec![11399],
                labels: vec![2000],
                ..Facts::default()
            },
        );

        let wanted = wanted_qids(&harvest);
        assert!(wanted.contains(&24826) && wanted.contains(&145));
        assert!(wanted.contains(&11399) && wanted.contains(&2000));
        assert!(!wanted.contains(&11649), "the artist's own item is named by MusicBrainz");
    }

    #[test]
    fn prefers_formation_place_over_birth_place() {
        // A band formed in Aberdeen whose entity also carries a birthplace:
        // "formed in" is the better answer, and which one answered is recorded.
        let line = entity(
            "Q11649",
            &format!(
                r#""claims":{{{},{},{}}}"#,
                mbid_claim(MBID_A),
                item_claim(P_FORMATION_PLACE, 233_808),
                item_claim(P_BIRTH_PLACE, 24826)
            ),
        );
        let (facts, _, _) = take(&[&line]);
        let found = &facts[MBID_A];
        assert_eq!(found.origin_qid, Some(233_808));
        assert!(!found.origin_is_birth);
    }

    #[test]
    fn falls_back_to_birth_place_for_people() {
        let line = entity("Q1", &format!(r#""claims":{{{},{}}}"#, mbid_claim(MBID_A), item_claim(P_BIRTH_PLACE, 24826)));
        let (facts, _, _) = take(&[&line]);
        assert_eq!(facts[MBID_A].origin_qid, Some(24826));
        assert!(facts[MBID_A].origin_is_birth);
    }

    #[test]
    fn reads_the_year_out_of_a_signed_time_value() {
        // Wikidata writes "+1987-01-01T00:00:00Z"; the leading plus is part of
        // the format, and a parser expecting a digit reads nothing.
        let time = r#""P571":[{"mainsnak":{"datavalue":{"value":{"time":"+1987-01-01T00:00:00Z","precision":9},"type":"time"}}}]"#;
        let line = entity("Q1", &format!(r#""claims":{{{},{time}}}"#, mbid_claim(MBID_A)));
        let (facts, _, _) = take(&[&line]);
        assert_eq!(facts[MBID_A].inception_year, Some(1987));
    }

    #[test]
    fn keeps_the_enwiki_title_for_the_prose_import() {
        let line = entity(
            "Q11649",
            &format!(
                r#""claims":{{{}}},"sitelinks":{{"enwiki":{{"site":"enwiki","title":"Nirvana (band)","badges":[]}},"ruwiki":{{"site":"ruwiki","title":"Nirvana"}}}}"#,
                mbid_claim(MBID_A)
            ),
        );
        let (facts, _, _) = take(&[&line]);
        assert_eq!(facts[MBID_A].enwiki_title.as_deref(), Some("Nirvana (band)"));
    }

    #[test]
    fn collects_every_value_of_the_many_valued_properties() {
        let genres = format!(
            r#""P136":[{},{}]"#,
            r#"{"mainsnak":{"datavalue":{"value":{"entity-type":"item","id":"Q11399"},"type":"wikibase-entityid"}}}"#,
            r#"{"mainsnak":{"datavalue":{"value":{"entity-type":"item","id":"Q83440"},"type":"wikibase-entityid"}}}"#
        );
        let line = entity("Q1", &format!(r#""claims":{{{},{genres}}}"#, mbid_claim(MBID_A)));
        let (facts, _, _) = take(&[&line]);
        assert_eq!(facts[MBID_A].genres, vec![11399, 83440]);
    }

    #[test]
    fn survives_a_malformed_line() {
        let good = entity("Q1", &format!(r#""claims":{{{}}}"#, mbid_claim(MBID_A)));
        let (facts, _, read) = take(&["{not json", &good]);
        assert_eq!(read, 1, "the broken line is not counted as an entity");
        assert_eq!(facts.len(), 1);
    }

    #[test]
    fn ignores_snaks_with_no_value() {
        // "unknown value" statements carry no datavalue at all.
        let unknown = r#""P740":[{"mainsnak":{"snaktype":"somevalue","property":"P740"}}]"#;
        let line = entity("Q1", &format!(r#""claims":{{{},{unknown}}}"#, mbid_claim(MBID_A)));
        let (facts, _, _) = take(&[&line]);
        assert_eq!(facts[MBID_A].origin_qid, None);
    }

    #[test]
    fn ignores_entities_that_are_not_items() {
        // Lexemes share the file with items and have no Q-number.
        let lexeme = r#"{"type":"lexeme","id":"L1234","lemmas":{}},"#;
        let (facts, _, read) = take(&[lexeme]);
        assert_eq!(read, 1, "it was still a line of the dump");
        assert!(facts.is_empty());
        // The label pass has to skip it too, and for the same reason: no
        // Q-number to key it by.
        assert!(take_labels(&[lexeme], &[1234]).is_empty());
    }
}
