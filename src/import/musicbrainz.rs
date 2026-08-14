//! Imports the canonical universe from a MusicBrainz full export.
//!
//! The archive (mbdump.tar.bz2, ~7 GB compressed) holds one file per table in
//! PostgreSQL's COPY TEXT format under `mbdump/`. Only twelve of those tables
//! matter here; the rest -- recordings, tracks, edit history -- are skipped
//! without being decompressed into memory.
//!
//! The archive is read in a **single pass**. bzip2 has no random access, so
//! seeking back for a second table would mean decompressing gigabytes again;
//! instead every needed file is handled as it goes by, and tar order decides
//! nothing. Lookup tables (types, areas, link types) are small enough to hold
//! in memory, so the two big tables can be resolved and streamed straight into
//! Postgres as they are read.
//!
//! The comments here quote upstream table and column names constantly -- they
//! are the contract this file is written against, since the dump carries no
//! headers -- so bare `snake_case` names read as prose rather than as code.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream table and column names throughout")]

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bzip2::read::MultiBzDecoder;
use clap::Args as ClapArgs;
use sqlx::PgPool;

use super::copy_text::{Reader, Row};

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 8192;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to mbdump.tar.bz2 from a MusicBrainz full export.
    #[arg(long, value_name = "FILE")]
    pub dump: PathBuf,

    /// The export version, as published in the fullexport LATEST file (for
    /// example 20260813-220122). Recorded so the database can say which
    /// universe it holds. Defaults to the TIMESTAMP file inside the dump.
    // Spelled --dump-version so it cannot be mistaken for the flag that
    // prints the program's own version.
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,
}

/// Everything gathered from the archive during the single pass.
#[derive(Default)]
struct Tables {
    artist_types: HashMap<i32, String>,
    release_group_types: HashMap<i32, String>,
    areas: HashMap<i32, String>,
    /// link.id -> link_type.id
    links: HashMap<i32, i32>,
    /// link_type.id -> name, for artist->url link types only.
    url_link_types: HashMap<i32, String>,
    /// url.id -> the address itself.
    urls: HashMap<i32, String>,
    artists: Vec<Artist>,
    release_groups: Vec<ReleaseGroup>,
    /// (artist_id, link_id, url_id), resolved after the pass.
    artist_url_links: Vec<(i32, i32, i32)>,
    /// artist_credit.id -> the first credited artist id.
    credit_artists: HashMap<i32, i32>,
    timestamp: Option<String>,
}

struct Artist {
    id: i32,
    mbid: uuid::Uuid,
    name: String,
    sort_name: String,
    type_id: Option<i32>,
    area_id: Option<i32>,
    begin_year: Option<i16>,
    end_year: Option<i16>,
    ended: bool,
    comment: Option<String>,
}

struct ReleaseGroup {
    id: i32,
    mbid: uuid::Uuid,
    name: String,
    type_id: Option<i32>,
    credit_id: Option<i32>,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    let file = File::open(&args.dump).with_context(|| format!("cannot open {}", args.dump.display()))?;
    let size = file.metadata().map_or(0, |m| m.len());
    tracing::info!(dump = %args.dump.display(), size_mb = size / 1_048_576, "reading the MusicBrainz export");

    let tables = read_archive(file)?;

    let version = args
        .version
        .clone()
        .or_else(|| tables.timestamp.clone())
        .context("the dump carries no TIMESTAMP file; pass --version with the fullexport timestamp")?;

    tracing::info!(
        version = %version,
        artists = tables.artists.len(),
        release_groups = tables.release_groups.len(),
        artist_urls = tables.artist_url_links.len(),
        "archive read; writing to PostgreSQL"
    );

    write(pool, &tables, &version).await
}

/// Reads every needed table out of the archive in one pass.
fn read_archive(file: File) -> Result<Tables> {
    // The export is a concatenation of bzip2 streams for large tables, so a
    // single-stream decoder would stop early and silently truncate the import.
    let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, file));
    let mut archive = tar::Archive::new(decoder);
    let mut tables = Tables::default();

    for entry in archive.entries().context("the dump is not a readable tar archive")? {
        let entry = entry.context("failed to read an archive entry")?;
        let path = entry.path().context("archive entry has an unreadable path")?.into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // The timestamp file sits at the archive root and names the export.
        if name == "TIMESTAMP" {
            let mut text = String::new();
            let mut entry = entry;
            entry.read_to_string(&mut text).ok();
            tables.timestamp = Some(text.trim().to_string());
            continue;
        }

        // Table files live in mbdump/; anything else is metadata.
        if path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) != Some("mbdump") {
            continue;
        }

        let reader = BufReader::with_capacity(1 << 20, entry);
        match name {
            "artist" => read_artists(reader, &mut tables)?,
            "artist_type" => read_names(reader, &mut tables.artist_types)?,
            "release_group" => read_release_groups(reader, &mut tables)?,
            "release_group_primary_type" => read_names(reader, &mut tables.release_group_types)?,
            "area" => read_areas(reader, &mut tables)?,
            "artist_credit_name" => read_credit_names(reader, &mut tables)?,
            "url" => read_urls(reader, &mut tables)?,
            "link" => read_links(reader, &mut tables)?,
            "link_type" => read_link_types(reader, &mut tables)?,
            "l_artist_url" => read_artist_url_links(reader, &mut tables)?,
            _ => {}
        }
    }

    if tables.artists.is_empty() {
        bail!("no artists found in the archive: is this mbdump.tar.bz2 from a full export?");
    }
    Ok(tables)
}

/// Runs `handle` over every row of a COPY TEXT table.
fn each_row<R: BufRead>(reader: R, mut handle: impl FnMut(&Row)) -> Result<()> {
    let mut rows = Reader::new(reader);
    while let Some(row) = rows.next_row().context("failed to read a dump row")? {
        handle(&row);
    }
    Ok(())
}

/// The `(id, name)` shape shared by artist_type and release_group_primary_type.
fn read_names<R: BufRead>(reader: R, out: &mut HashMap<i32, String>) -> Result<()> {
    each_row(reader, |row| {
        if let (Some(id), Some(name)) = (row.parse::<i32>(0), row.get(1)) {
            out.insert(id, name.to_string());
        }
    })
}

/// artist: id, gid, name, sort_name, begin_date_year, begin_date_month,
/// begin_date_day, end_date_year, end_date_month, end_date_day, type, area,
/// gender, comment, edits_pending, last_updated, ended, begin_area, end_area
fn read_artists<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        let (Some(id), Some(mbid), Some(name), Some(sort_name)) = (row.parse::<i32>(0), row.parse::<uuid::Uuid>(1), row.get(2), row.get(3)) else {
            return;
        };
        tables.artists.push(Artist {
            id,
            mbid,
            name: name.to_string(),
            sort_name: sort_name.to_string(),
            begin_year: row.parse(4),
            end_year: row.parse(7),
            type_id: row.parse(10),
            area_id: row.parse(11),
            comment: row.get(13).filter(|c| !c.is_empty()).map(str::to_string),
            ended: row.get(16) == Some("t"),
        });
    })
}

/// area: id, gid, name, type, ... -- only the name is needed.
fn read_areas<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if let (Some(id), Some(name)) = (row.parse::<i32>(0), row.get(2)) {
            tables.areas.insert(id, name.to_string());
        }
    })
}

/// release_group: id, gid, name, artist_credit, type, comment, ...
fn read_release_groups<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        let (Some(id), Some(mbid), Some(name)) = (row.parse::<i32>(0), row.parse::<uuid::Uuid>(1), row.get(2)) else {
            return;
        };
        tables.release_groups.push(ReleaseGroup {
            id,
            mbid,
            name: name.to_string(),
            credit_id: row.parse(3),
            type_id: row.parse(4),
        });
    })
}

/// artist_credit_name: artist_credit, position, artist, name, join_phrase.
/// Only position 0 matters: the sky places a release group in the system of
/// the artist credited first.
fn read_credit_names<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if row.parse::<i32>(1) != Some(0) {
            return;
        }
        if let (Some(credit), Some(artist)) = (row.parse::<i32>(0), row.parse::<i32>(2)) {
            tables.credit_artists.insert(credit, artist);
        }
    })
}

/// url: id, gid, url, edits_pending, last_updated
fn read_urls<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if let (Some(id), Some(url)) = (row.parse::<i32>(0), row.get(2)) {
            tables.urls.insert(id, url.to_string());
        }
    })
}

/// link: id, link_type, ...
fn read_links<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if let (Some(id), Some(link_type)) = (row.parse::<i32>(0), row.parse::<i32>(1)) {
            tables.links.insert(id, link_type);
        }
    })
}

/// link_type: id, parent, child_order, gid, entity_type0, entity_type1, name, ...
/// Only artist->url types are kept; the rest describe relationships this
/// import does not carry.
fn read_link_types<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if row.get(4) != Some("artist") || row.get(5) != Some("url") {
            return;
        }
        if let (Some(id), Some(name)) = (row.parse::<i32>(0), row.get(6)) {
            tables.url_link_types.insert(id, name.to_string());
        }
    })
}

/// l_artist_url: id, link, entity0 (artist), entity1 (url), ...
fn read_artist_url_links<R: BufRead>(reader: R, tables: &mut Tables) -> Result<()> {
    each_row(reader, |row| {
        if let (Some(link), Some(artist), Some(url)) = (row.parse::<i32>(1), row.parse::<i32>(2), row.parse::<i32>(3)) {
            tables.artist_url_links.push((artist, link, url));
        }
    })
}

/// Writes the whole universe in one transaction: an interrupted import leaves
/// the previous canon intact rather than a half-replaced one.
async fn write(pool: &PgPool, tables: &Tables, version: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('musicbrainz', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    // Re-importing replaces the canon wholesale. TRUNCATE rather than DELETE:
    // this is millions of rows, and nothing references them from outside.
    //
    // CASCADE reaches the similarity tables too, which is correct: edges point
    // at artist ids from a particular dump, and a new dump renumbers nothing
    // but may drop artists. Similarity is re-imported after the canon.
    sqlx::query("TRUNCATE artist, release_group, artist_url, artist_credit RESTART IDENTITY CASCADE")
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous canon")?;

    let mut written: i64 = 0;
    written += write_artists(&mut tx, tables).await?;
    written += write_artist_credits(&mut tx, tables).await?;
    written += write_release_groups(&mut tx, tables).await?;
    written += write_artist_urls(&mut tx, tables).await?;

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, rows = written, "import complete");
    Ok(())
}

/// Persists the credit-to-artist mapping, which datasets keyed on
/// artist_credit -- ListenBrainz's similarity dump among them -- need long
/// after this import has finished.
async fn write_artist_credits(tx: &mut sqlx::PgTransaction<'_>, tables: &Tables) -> Result<i64> {
    let known: std::collections::HashSet<i32> = tables.artists.iter().map(|a| a.id).collect();
    let resolved: Vec<(i32, i32)> = tables
        .credit_artists
        .iter()
        .filter(|(_, artist)| known.contains(artist))
        .map(|(&credit, &artist)| (credit, artist))
        .collect();

    let mut written = 0i64;
    for chunk in resolved.chunks(BATCH) {
        let credits: Vec<i32> = chunk.iter().map(|(c, _)| *c).collect();
        let artists: Vec<i32> = chunk.iter().map(|(_, a)| *a).collect();

        sqlx::query(
            "INSERT INTO artist_credit (id, artist_id)
             SELECT * FROM UNNEST($1::int[], $2::int[])",
        )
        .bind(&credits)
        .bind(&artists)
        .execute(&mut **tx)
        .await
        .context("failed to write artist credits")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "artist credits written");
    Ok(written)
}

async fn write_artists(tx: &mut sqlx::PgTransaction<'_>, tables: &Tables) -> Result<i64> {
    let mut written = 0i64;
    for chunk in tables.artists.chunks(BATCH) {
        let ids: Vec<i32> = chunk.iter().map(|a| a.id).collect();
        let mbids: Vec<uuid::Uuid> = chunk.iter().map(|a| a.mbid).collect();
        let names: Vec<&str> = chunk.iter().map(|a| a.name.as_str()).collect();
        let sort_names: Vec<&str> = chunk.iter().map(|a| a.sort_name.as_str()).collect();
        let kinds: Vec<Option<&str>> = chunk
            .iter()
            .map(|a| a.type_id.and_then(|t| tables.artist_types.get(&t)).map(String::as_str))
            .collect();
        let areas: Vec<Option<&str>> = chunk.iter().map(|a| a.area_id.and_then(|t| tables.areas.get(&t)).map(String::as_str)).collect();
        let begins: Vec<Option<i16>> = chunk.iter().map(|a| a.begin_year).collect();
        let ends: Vec<Option<i16>> = chunk.iter().map(|a| a.end_year).collect();
        let ended: Vec<bool> = chunk.iter().map(|a| a.ended).collect();
        let comments: Vec<Option<&str>> = chunk.iter().map(|a| a.comment.as_deref()).collect();

        sqlx::query(
            "INSERT INTO artist (id, mbid, name, sort_name, kind, area, begin_year, end_year, ended, comment)
             SELECT * FROM UNNEST($1::int[], $2::uuid[], $3::text[], $4::text[], $5::text[], $6::text[], $7::smallint[], $8::smallint[], $9::bool[], $10::text[])",
        )
        .bind(&ids)
        .bind(&mbids)
        .bind(&names)
        .bind(&sort_names)
        .bind(&kinds)
        .bind(&areas)
        .bind(&begins)
        .bind(&ends)
        .bind(&ended)
        .bind(&comments)
        .execute(&mut **tx)
        .await
        .context("failed to write artists")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "artists written");
    Ok(written)
}

async fn write_release_groups(tx: &mut sqlx::PgTransaction<'_>, tables: &Tables) -> Result<i64> {
    // A release group whose credited artist is missing from the dump would
    // violate the foreign key; such rows are dropped rather than imported
    // detached from any system.
    let known: std::collections::HashSet<i32> = tables.artists.iter().map(|a| a.id).collect();
    let resolved: Vec<(&ReleaseGroup, i32)> = tables
        .release_groups
        .iter()
        .filter_map(|rg| {
            let artist = rg.credit_id.and_then(|c| tables.credit_artists.get(&c)).copied()?;
            known.contains(&artist).then_some((rg, artist))
        })
        .collect();

    let mut written = 0i64;
    for chunk in resolved.chunks(BATCH) {
        let ids: Vec<i32> = chunk.iter().map(|(rg, _)| rg.id).collect();
        let mbids: Vec<uuid::Uuid> = chunk.iter().map(|(rg, _)| rg.mbid).collect();
        let names: Vec<&str> = chunk.iter().map(|(rg, _)| rg.name.as_str()).collect();
        let types: Vec<Option<&str>> = chunk
            .iter()
            .map(|(rg, _)| rg.type_id.and_then(|t| tables.release_group_types.get(&t)).map(String::as_str))
            .collect();
        let artists: Vec<i32> = chunk.iter().map(|(_, artist)| *artist).collect();

        sqlx::query(
            "INSERT INTO release_group (id, mbid, name, primary_type, artist_id)
             SELECT * FROM UNNEST($1::int[], $2::uuid[], $3::text[], $4::text[], $5::int[])",
        )
        .bind(&ids)
        .bind(&mbids)
        .bind(&names)
        .bind(&types)
        .bind(&artists)
        .execute(&mut **tx)
        .await
        .context("failed to write release groups")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, dropped = tables.release_groups.len() - resolved.len(), "release groups written");
    Ok(written)
}

async fn write_artist_urls(tx: &mut sqlx::PgTransaction<'_>, tables: &Tables) -> Result<i64> {
    let known: std::collections::HashSet<i32> = tables.artists.iter().map(|a| a.id).collect();
    let resolved: Vec<(i32, &str, &str)> = tables
        .artist_url_links
        .iter()
        .filter_map(|&(artist, link, url)| {
            let link_type = tables.links.get(&link)?;
            let kind = tables.url_link_types.get(link_type)?;
            let address = tables.urls.get(&url)?;
            known.contains(&artist).then_some((artist, kind.as_str(), address.as_str()))
        })
        .collect();

    let mut written = 0i64;
    for chunk in resolved.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|(a, _, _)| *a).collect();
        let kinds: Vec<&str> = chunk.iter().map(|(_, k, _)| *k).collect();
        let urls: Vec<&str> = chunk.iter().map(|(_, _, u)| *u).collect();

        sqlx::query(
            "INSERT INTO artist_url (artist_id, kind, url)
             SELECT * FROM UNNEST($1::int[], $2::text[], $3::text[])
             ON CONFLICT DO NOTHING",
        )
        .bind(&artists)
        .bind(&kinds)
        .bind(&urls)
        .execute(&mut **tx)
        .await
        .context("failed to write artist URLs")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }
    tracing::info!(rows = written, "artist URLs written");
    Ok(written)
}
