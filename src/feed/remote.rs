//! ListenBrainz's dump server, read a byte range at a time.
//!
//! A day of listening is a tar of two Parquet files, about two hundred
//! megabytes, of which lyrid counts four columns. Parquet keeps its table of
//! contents at the end of the file and each column in its own byte range, and
//! the dump server answers `Range` requests, so the columns are fetched where
//! they lie and the rest never crosses the network: about a third of the
//! file, and no copy of it on the stand's SD card.
//!
//! Blocking, like the rest of lyrid's outbound HTTP: the feeds run it on the
//! blocking pool, and nothing a visitor waits for goes through here.

use std::time::Duration;

use anyhow::{Context, Result, bail};

/// Where ListenBrainz publishes its dumps, unless `LYRID_DUMPS_URL` says
/// otherwise.
pub const DEFAULT_URL: &str = "https://data.metabrainz.org/pub/musicbrainz/listenbrainz";

/// The dump server, as far as the feeds use it.
#[derive(Clone)]
pub struct Dumps {
    base: String,
    agent: ureq::Agent,
}

/// One daily dump on offer: `listenbrainz-dump-2724-20261016-000002-incremental`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dump {
    /// ListenBrainz's running number for its dumps, shared by full and
    /// incremental ones -- which is why the incremental numbers have gaps.
    pub sequence: u32,
    /// The directory name, which is also what a feed run records.
    pub name: String,
}

impl Dumps {
    pub fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            // A range is a few megabytes at most; a minute without one is a
            // server that is not going to answer.
            .timeout_global(Some(Duration::from_secs(60)))
            // Statuses are read rather than thrown: a 200 where a 206 was
            // asked for is the one answer that must not be read as data.
            .http_status_as_error(false)
            .user_agent(concat!("lyrid/", env!("CARGO_PKG_VERSION"), " (+https://github.com/lacodda/lyrid)"))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }

    /// The incremental dumps the server offers now, oldest first.
    ///
    /// Read from the directory's index page: the server publishes no listing
    /// in any other form, and the names are the one thing on that page that
    /// is a contract.
    pub fn incremental(&self) -> Result<Vec<Dump>> {
        let url = format!("{}/incremental/", self.base);
        let mut response = self.agent.get(&url).call().with_context(|| format!("cannot reach {url}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            bail!("{url} answered {status}");
        }
        let page = response.body_mut().read_to_string().with_context(|| format!("cannot read {url}"))?;
        Ok(incremental_dumps(&page))
    }

    /// The Parquet copy of one dump: the same listens, already matched to
    /// artists by ListenBrainz.
    pub fn spark(&self, dump: &Dump) -> Result<Remote> {
        let stem = dump.name.strip_prefix("listenbrainz-dump-").unwrap_or(&dump.name);
        let url = format!("{}/incremental/{}/listenbrainz-spark-dump-{stem}.tar", self.base, dump.name);
        self.file(url)
    }

    fn file(&self, url: String) -> Result<Remote> {
        let response = self.agent.head(&url).call().with_context(|| format!("cannot reach {url}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            bail!("{url} answered {status}");
        }
        let len = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .with_context(|| format!("{url} does not say how long it is"))?;
        Ok(Remote {
            agent: self.agent.clone(),
            url,
            len,
        })
    }
}

/// The dumps named on an index page, oldest first, each once.
fn incremental_dumps(page: &str) -> Vec<Dump> {
    let mut dumps: Vec<Dump> = page
        .split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter_map(|href| {
            let name = href.trim_end_matches('/');
            Some(Dump {
                sequence: sequence_of(name)?,
                name: name.to_string(),
            })
        })
        .collect();
    dumps.sort_by_key(|dump| dump.sequence);
    dumps.dedup_by_key(|dump| dump.sequence);
    dumps
}

/// The running number of a daily dump, from its name:
/// `listenbrainz-dump-<sequence>-<yyyymmdd>-<hhmmss>-incremental`. `None` for
/// anything else, full dumps included.
pub fn sequence_of(name: &str) -> Option<u32> {
    let parts: Vec<&str> = name.strip_prefix("listenbrainz-dump-")?.strip_suffix("-incremental")?.split('-').collect();
    let [sequence, date, clock] = parts.as_slice() else {
        return None;
    };
    let digits = |text: &str, len: usize| text.len() == len && text.bytes().all(|b| b.is_ascii_digit());
    if !(digits(date, 8) && digits(clock, 6)) {
        return None;
    }
    sequence.parse().ok()
}

/// A file on the dump server, read by byte ranges.
#[derive(Clone)]
pub struct Remote {
    agent: ureq::Agent,
    url: String,
    len: u64,
}

impl Remote {
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// `len` bytes from `at`.
    pub fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let last = at + len as u64 - 1;
        if last >= self.len {
            bail!("asked {} for bytes {at}-{last}, past its end at {}", self.url, self.len);
        }
        let mut response = self
            .agent
            .get(&self.url)
            .header("Range", format!("bytes={at}-{last}"))
            .call()
            .with_context(|| format!("cannot reach {}", self.url))?;
        let status = response.status().as_u16();
        // A 200 is the whole file: a server that ignores ranges. Reading it
        // as the range would count the first bytes of the file as whatever
        // was asked for, so it is refused outright.
        if status != 206 {
            bail!("{} answered {status} to a range request; the feed reads dumps by ranges", self.url);
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(len as u64 + 1)
            .read_to_vec()
            .with_context(|| format!("cannot read bytes {at}-{last} of {}", self.url))?;
        if body.len() != len {
            bail!("{} sent {} bytes for a range of {len}", self.url, body.len());
        }
        Ok(body)
    }
}

/// One file inside a tar, by where its bytes are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// Where the file's bytes begin, from the start of the tar.
    pub offset: u64,
    pub size: u64,
}

/// The files of a tar on the server, found by reading its headers alone.
///
/// An uncompressed tar is a header of 512 bytes before each file, and the
/// file padded to a multiple of 512: so the next header's place is known from
/// this one, and the files in between are stepped over without a byte of them
/// being fetched.
pub fn entries(remote: &Remote) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut at = 0u64;
    while at + 512 <= remote.len() {
        let block = remote.read(at, 512)?;
        // Two zero blocks end an archive; one is enough to know.
        if block.iter().all(|&b| b == 0) {
            break;
        }
        let header = tar::Header::from_byte_slice(&block);
        let size = header
            .entry_size()
            .with_context(|| format!("a header at {at} of {} is unreadable", remote.url()))?;
        let name = header
            .path()
            .with_context(|| format!("a header at {at} of {} names no file", remote.url()))?
            .to_string_lossy()
            .into_owned();
        if header.entry_type().is_file() {
            entries.push(Entry { name, offset: at + 512, size });
        }
        at += 512 + size.div_ceil(512) * 512;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_daily_dumps_on_the_index_page() {
        // As the server writes it on 2026-10-16, trimmed.
        let page = r#"<html><body><h1>Index of /pub/musicbrainz/listenbrainz/incremental/</h1><hr><pre><a href="../">../</a>
<a href="listenbrainz-dump-2725-20261016-000003-incremental/">listenbrainz-dump-2725-20261016-000003-incremental/</a> 16-Oct-2026 00:00       -
<a href="listenbrainz-dump-2709-20261009-000003-incremental/">listenbrainz-dump-2709-20261009-000003-incremental/</a> 09-Oct-2026 00:00       -
<a href="listenbrainz-dump-2724-20261016-000002-incremental/">listenbrainz-dump-2724-20261016-000002-incremental/</a> 16-Oct-2026 00:05       -
<a href="listenbrainz-dump-2692-20261001-000003-full/">a full dump is not a daily one</a>
<a href="listenbrainz-dump-x-20261016-000002-incremental/">a name that is not one</a>
</pre><hr></body></html>"#;
        let dumps = incremental_dumps(page);
        assert_eq!(
            dumps.iter().map(|dump| dump.sequence).collect::<Vec<_>>(),
            vec![2709, 2724, 2725],
            "oldest first, and only the daily ones"
        );
        assert_eq!(dumps[1].name, "listenbrainz-dump-2724-20261016-000002-incremental");
    }

    #[test]
    fn a_page_with_no_dumps_offers_none() {
        assert_eq!(incremental_dumps("<html><body>moved</body></html>"), Vec::<Dump>::new());
    }
}
