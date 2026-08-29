//! Imports Wikipedia lead paragraphs for the artists in the canon.
//!
//! The article dump is 27 GB and the encyclopaedia has seven million articles;
//! the canon wants a few hundred thousand of them. Reading the whole archive
//! to find them would be the wrong shape, so this import uses the
//! **multistream index** instead:
//!
//! - the index (284 MB compressed) is a plain list of
//!   `byte_offset:page_id:title` lines,
//! - the archive is many independent bzip2 streams concatenated, each holding
//!   about a hundred articles,
//! - so an article can be reached by seeking to its stream's offset and
//!   decompressing that stream alone.
//!
//! Measured: one article costs about 1.9 MB of transfer rather than 27 GB. The
//! import groups wanted articles by their stream so a stream holding several
//! of them is fetched once.
//!
//! **The index and the archive must come from the same dated run.** The
//! `latest-*` symlinks are rebuilt at different moments, so their offsets
//! disagree; passing a dated prefix like `20260801` is what keeps them in step.
//!
//! Which articles to fetch comes from `artist_wikidata.enwiki_title`, captured
//! during the Wikidata pass so this import needs no second look at that dump.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream file names throughout")]

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bzip2::read::MultiBzDecoder;
use clap::Args as ClapArgs;
use sqlx::PgPool;

use super::wikitext;

/// How many rows to accumulate before sending a batch to Postgres.
const BATCH: usize = 1024;

/// The licence the text arrives under, stored with every extract.
const LICENCE: &str = "CC BY-SA 4.0";

#[derive(ClapArgs)]
pub struct Args {
    /// Path to enwiki-<date>-pages-articles-multistream.xml.bz2.
    #[arg(long, value_name = "FILE")]
    pub dump: PathBuf,

    /// Path to the matching enwiki-<date>-pages-articles-multistream-index.txt.bz2.
    /// It must come from the same dated run as the dump: offsets from a
    /// different run point into the wrong streams.
    #[arg(long, value_name = "FILE")]
    pub index: PathBuf,

    /// The dump version recorded with every extract, so a claim can be dated.
    /// Defaults to the date in the dump's filename.
    #[arg(long = "dump-version", value_name = "VERSION")]
    pub version: Option<String>,

    /// Import at most this many articles. For trying the pipeline without
    /// waiting for the whole canon.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
}

/// One artist's prose, ready to be written.
struct Prose {
    artist_id: i32,
    title: String,
    extract: String,
    revision_id: Option<i64>,
    source_chars: i32,
    extract_chars: i32,
}

pub async fn run(pool: &PgPool, args: &Args) -> Result<()> {
    let wanted = load_wanted(pool).await?;
    if wanted.is_empty() {
        bail!(
            "no artist has a Wikipedia article title: run `lyrid import wikidata` first \
             (the titles come from its sitelinks)"
        );
    }
    tracing::info!(wanted = wanted.len(), "looking for articles named by the canon");

    let version = args
        .version
        .clone()
        .or_else(|| version_from_filename(&args.dump))
        .context("cannot tell the dump version from the filename; pass --dump-version")?;

    let offsets = read_index(args, &wanted)?;
    if offsets.is_empty() {
        bail!("none of the canon's article titles appear in the index: are the index and dump from the same run?");
    }

    let prose = read_articles(args, &offsets, &wanted)?;

    // Every located article failing to appear at its offset is the signature
    // of an index and a dump from different runs -- the offsets point into the
    // wrong streams. Writing that result would silently replace good prose
    // with nothing, so it fails instead.
    if prose.is_empty() {
        bail!(
            "the index located {} article(s) but none was found at its offset: \
             the index and the dump are probably from different dated runs, \
             whose offsets do not match",
            offsets.values().map(Vec::len).sum::<usize>()
        );
    }

    tracing::info!(articles = prose.len(), "articles read; writing to PostgreSQL");

    write(pool, &prose, &version).await
}

/// Article title -> artist id, for every artist that names one.
async fn load_wanted(pool: &PgPool) -> Result<HashMap<String, i32>> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT enwiki_title, artist_id FROM artist_wikidata
         WHERE enwiki_title IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .context("failed to read article titles from the canon")?;
    Ok(rows.into_iter().collect())
}

/// The date out of `enwiki-20260801-pages-articles-multistream.xml.bz2`.
fn version_from_filename(path: &std::path::Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let after = name.strip_prefix("enwiki-")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    (digits.len() == 8).then_some(digits)
}

/// Where each wanted article lives: stream offset -> the titles in it.
///
/// Grouped by offset because one stream holds about a hundred articles, and
/// several of the canon's artists may share one.
fn read_index(args: &Args, wanted: &HashMap<String, i32>) -> Result<HashMap<u64, Vec<String>>> {
    let file = File::open(&args.index).with_context(|| format!("cannot open {}", args.index.display()))?;
    tracing::info!(index = %args.index.display(), "reading the multistream index");

    let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, file));
    let reader = BufReader::with_capacity(1 << 20, decoder);

    let mut offsets: HashMap<u64, Vec<String>> = HashMap::new();
    let mut found = 0usize;
    let mut lines = 0u64;

    for line in reader.lines() {
        let line = line.context("failed to read the index")?;
        lines += 1;

        // `offset:page_id:title`, and a title may itself contain colons.
        let mut parts = line.splitn(3, ':');
        let (Some(offset), Some(_page), Some(title)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if !wanted.contains_key(title) {
            continue;
        }
        let Ok(offset) = offset.parse::<u64>() else { continue };

        offsets.entry(offset).or_default().push(title.to_string());
        found += 1;
        if args.limit.is_some_and(|limit| found >= limit) {
            tracing::info!(found, "stopping at the requested limit");
            break;
        }
    }

    tracing::info!(index_lines = lines, articles_located = found, streams = offsets.len(), "index read");
    Ok(offsets)
}

/// Reads each stream that holds a wanted article and extracts the leads.
fn read_articles(args: &Args, offsets: &HashMap<u64, Vec<String>>, wanted: &HashMap<String, i32>) -> Result<Vec<Prose>> {
    let mut file = File::open(&args.dump).with_context(|| format!("cannot open {}", args.dump.display()))?;
    let mut prose = Vec::new();
    let mut streams_read = 0usize;
    let mut without_lead = 0usize;
    let mut panicked = 0usize;

    for (offset, titles) in offsets {
        file.seek(SeekFrom::Start(*offset)).with_context(|| format!("cannot seek to {offset}"))?;

        // One stream, not the rest of the archive: the decoder is given a
        // reader that stops at the end of this bzip2 member.
        let mut xml = String::new();
        let taken = (&mut file).take(stream_budget());
        // A stream ends on its own; MultiBzDecoder would run into the next one,
        // so any error past the first member is the end of what was wanted.
        bzip2::read::BzDecoder::new(taken).read_to_string(&mut xml).ok();
        streams_read += 1;

        for title in titles {
            let Some(&artist_id) = wanted.get(title.as_str()) else { continue };
            let Some(article) = find_article(&xml, title) else { continue };

            let source_chars = i32::try_from(article.text.chars().count()).unwrap_or(i32::MAX);

            // One unparseable article must not cost the whole run.
            //
            // The parser walks wikitext written by hand by thousands of
            // editors, and a full pass takes hours: twice already a fixed byte
            // window landed inside a multi-byte character and a panic threw
            // away everything read so far. Both were fixed and are covered by
            // tests, but the dump refreshes monthly and the next article is
            // written by someone new. So a panic here loses one article and is
            // counted, rather than losing the pass.
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wikitext::lead(&article.text)));
            let Ok(parsed) = attempt else {
                tracing::warn!(title = %title, "the parser panicked on this article; skipping it");
                panicked += 1;
                continue;
            };

            match parsed {
                Some(extract) => prose.push(Prose {
                    artist_id,
                    title: title.clone(),
                    extract_chars: i32::try_from(extract.chars().count()).unwrap_or(i32::MAX),
                    extract,
                    revision_id: article.revision_id,
                    source_chars,
                }),
                // A redirect or an article of nothing but templates: recorded
                // as a miss rather than stored as empty prose.
                None => without_lead += 1,
            }
        }
    }

    if panicked > 0 {
        // Loud, because a rising count means the parser has met a shape it
        // does not handle -- and the titles are in the log to reproduce it.
        tracing::warn!(panicked, "articles the parser could not read; their prose is missing");
    }
    tracing::info!(
        streams_read,
        extracted = prose.len(),
        skipped_without_lead = without_lead,
        panicked,
        "articles read"
    );
    Ok(prose)
}

/// How much of the archive to hand the decoder for one stream.
///
/// A multistream member holds about a hundred articles; this is a generous
/// ceiling on that, so the decoder always sees a whole member and stops there
/// on its own.
const fn stream_budget() -> u64 {
    8 << 20
}

/// One article's text and revision, pulled out of a stream's XML.
struct Article {
    text: String,
    revision_id: Option<i64>,
}

/// Finds one article by title in a decompressed stream.
fn find_article(xml: &str, title: &str) -> Option<Article> {
    // The XML in the dump escapes the title the same way it escapes text.
    let needle = format!("<title>{}</title>", escape(title));
    let start = xml.find(&needle)?;
    let page = &xml[start..];
    let end = page.find("</page>").unwrap_or(page.len());
    let page = &page[..end];

    let text_start = page.find("<text")?;
    let text_open_end = page[text_start..].find('>')? + text_start + 1;
    let text_end = page[text_open_end..].find("</text>")? + text_open_end;

    let revision_id = page
        .find("<revision>")
        .and_then(|at| page[at..].find("<id>").map(|i| at + i + 4))
        .and_then(|from| page[from..].find("</id>").map(|len| &page[from..from + len]))
        .and_then(|digits| digits.parse().ok());

    Some(Article {
        text: unescape(&page[text_open_end..text_end]),
        revision_id,
    })
}

/// The XML escaping the dump applies to titles.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Undoes the dump's XML escaping. This is not the same as the entity decoding
/// the wikitext parser does: that one handles entities editors typed into the
/// article source, which survive this step.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Writes the prose, attribution included, in one transaction.
async fn write(pool: &PgPool, prose: &[Prose], version: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to open the import transaction")?;

    let import_id: i32 = sqlx::query_scalar(
        "INSERT INTO dump_import (source, version) VALUES ('wikipedia', $1)
         ON CONFLICT (source, version) DO UPDATE SET started_at = now(), finished_at = NULL, rows_imported = NULL
         RETURNING id",
    )
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .context("failed to record the import")?;

    sqlx::query("TRUNCATE artist_prose")
        .execute(&mut *tx)
        .await
        .context("failed to clear the previous prose")?;

    let mut written = 0i64;
    for chunk in prose.chunks(BATCH) {
        let artists: Vec<i32> = chunk.iter().map(|p| p.artist_id).collect();
        let extracts: Vec<&str> = chunk.iter().map(|p| p.extract.as_str()).collect();
        let titles: Vec<&str> = chunk.iter().map(|p| p.title.as_str()).collect();
        let urls: Vec<String> = chunk.iter().map(|p| article_url(&p.title)).collect();
        let licences: Vec<&str> = chunk.iter().map(|_| LICENCE).collect();
        let versions: Vec<&str> = chunk.iter().map(|_| version).collect();
        let revisions: Vec<Option<i64>> = chunk.iter().map(|p| p.revision_id).collect();
        let source_chars: Vec<i32> = chunk.iter().map(|p| p.source_chars).collect();
        let extract_chars: Vec<i32> = chunk.iter().map(|p| p.extract_chars).collect();

        sqlx::query(
            "INSERT INTO artist_prose
                 (artist_id, extract, source_title, source_url, licence, dump_version, revision_id, source_chars, extract_chars)
             SELECT * FROM UNNEST($1::int[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::bigint[], $8::int[], $9::int[])
             ON CONFLICT (artist_id) DO NOTHING",
        )
        .bind(&artists)
        .bind(&extracts)
        .bind(&titles)
        .bind(&urls)
        .bind(&licences)
        .bind(&versions)
        .bind(&revisions)
        .bind(&source_chars)
        .bind(&extract_chars)
        .execute(&mut *tx)
        .await
        .context("failed to write prose")?;
        written += i64::try_from(chunk.len()).unwrap_or(i64::MAX);
    }

    sqlx::query("UPDATE dump_import SET finished_at = now(), rows_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(written)
        .execute(&mut *tx)
        .await
        .context("failed to close the import record")?;

    tx.commit().await.context("failed to commit the import")?;
    tracing::info!(version, rows = written, "prose import complete");
    Ok(())
}

/// The canonical article address, which the licence requires be shown with the
/// text.
fn article_url(title: &str) -> String {
    format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_article_url_the_licence_requires() {
        assert_eq!(article_url("Nirvana (band)"), "https://en.wikipedia.org/wiki/Nirvana_(band)");
        assert_eq!(article_url("The Beatles"), "https://en.wikipedia.org/wiki/The_Beatles");
    }

    #[test]
    fn takes_the_version_from_the_dump_filename() {
        let path = std::path::Path::new("/dumps/enwiki-20260801-pages-articles-multistream.xml.bz2");
        assert_eq!(version_from_filename(path).as_deref(), Some("20260801"));
        assert_eq!(version_from_filename(std::path::Path::new("/dumps/enwiki-latest-pages.xml.bz2")), None);
    }

    fn page(title: &str, revision: &str, text: &str) -> String {
        format!(
            "<page>\n<title>{title}</title>\n<ns>0</ns>\n<id>21231</id>\n<revision>\n<id>{revision}</id>\n<text bytes=\"9\" xml:space=\"preserve\">{text}</text>\n</revision>\n</page>\n"
        )
    }

    #[test]
    fn finds_one_article_among_several_in_a_stream() {
        // A stream holds about a hundred articles; the wanted one is somewhere
        // inside.
        let xml = format!(
            "{}{}{}",
            page("Nirvana", "1", "A concept in Buddhism."),
            page("Nirvana (band)", "12345", "An American rock band."),
            page("Nirvana (UK band)", "3", "A British band.")
        );
        let article = find_article(&xml, "Nirvana (band)").unwrap();
        assert_eq!(article.text, "An American rock band.");
        assert_eq!(article.revision_id, Some(12345));
    }

    #[test]
    fn returns_nothing_when_the_article_is_not_in_the_stream() {
        let xml = page("Some other article", "1", "Text.");
        assert!(find_article(&xml, "Nirvana (band)").is_none());
    }

    #[test]
    fn undoes_the_dumps_escaping_but_leaves_wikitext_entities() {
        // The dump escapes `<ref>` as `&lt;ref&gt;`, which must come back so
        // the parser can strip it. But `&nbsp;`, typed by an editor, has to
        // survive this step for the wikitext parser to resolve.
        let xml = page("X", "7", "A band&lt;ref&gt;cite&lt;/ref&gt; with 300&amp;nbsp;million sales.");
        let article = find_article(&xml, "X").unwrap();
        assert!(article.text.contains("<ref>cite</ref>"), "got: {}", article.text);
        assert!(article.text.contains("&nbsp;"), "got: {}", article.text);
    }

    #[test]
    fn matches_a_title_containing_an_ampersand() {
        // The dump escapes titles too, so a raw comparison would miss these.
        let xml = page("Emerson, Lake &amp; Palmer", "9", "An English band.");
        let article = find_article(&xml, "Emerson, Lake & Palmer").unwrap();
        assert_eq!(article.text, "An English band.");
    }

    #[test]
    fn survives_a_page_without_a_revision_id() {
        let xml = "<page>\n<title>X</title>\n<text>Some text about a band.</text>\n</page>";
        let article = find_article(xml, "X").unwrap();
        assert_eq!(article.revision_id, None);
        assert_eq!(article.text, "Some text about a band.");
    }
}
