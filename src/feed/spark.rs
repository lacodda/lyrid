//! The Parquet half of a ListenBrainz dump: listens already matched to
//! artists, read a column at a time.
//!
//! ListenBrainz writes each day twice. The JSON copy carries listens as they
//! were submitted, and almost none of them say which artist they are by -- the
//! matching happens later, inside ListenBrainz. The "spark" copy is the one its
//! own statistics are computed from, and carries the match: on 2026-10-15,
//! three listens in four played that day named their artists' MusicBrainz ids.
//!
//! Only four of its eleven columns are read, through a reader that holds the
//! file's footer and the byte ranges of those columns and nothing else -- see
//! `remote.rs` for why the rest is never fetched.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use bytes::{Buf, Bytes};
use parquet::basic::{LogicalType, TimeUnit, Type as PhysicalType};
use parquet::file::reader::{ChunkReader, FileReader, Length, SerializedFileReader};
use parquet::record::Field;
use parquet::schema::types::{ColumnDescriptor, Type};
use uuid::Uuid;

use super::remote::{Entry, Remote};

/// One listen, reduced to what is counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listen {
    /// When ListenBrainz received it, in seconds since the epoch.
    pub created: i64,
    /// When it was played, as the listener's player said.
    pub listened: i64,
    /// ListenBrainz's number for the listener.
    pub listener: i64,
    /// The credited artists. Empty when ListenBrainz has not matched it.
    pub artists: Vec<Uuid>,
}

/// The columns read, by their names in the dump.
const LISTENED_AT: &str = "listened_at";
const CREATED: &str = "created";
const USER_ID: &str = "user_id";
const ARTISTS: &str = "artist_credit_mbids";

/// Reads every listen of one Parquet file inside the tar, handing each to
/// `each`. Returns how many rows the file held.
pub fn read_listens(remote: &Remote, entry: &Entry, mut each: impl FnMut(Listen)) -> Result<u64> {
    let sparse = Sparse::open(remote, entry)?;
    let reader = SerializedFileReader::new(sparse.clone()).with_context(|| format!("{} is not a Parquet file", entry.name))?;
    let metadata = reader.metadata();
    let schema = metadata.file_metadata().schema_descr();

    // Each column's place among the leaves, and how many of its units make a
    // second. The dump's shape is checked here, once, rather than discovered
    // row by row: a column renamed upstream is a failed run with a reason,
    // not a day counted as silent.
    let leaf = |name: &str| -> Result<usize> {
        schema
            .columns()
            .iter()
            .position(|column| column.path().parts().first().map(String::as_str) == Some(name))
            .with_context(|| format!("{} has no column {name}: ListenBrainz changed the dump", entry.name))
    };
    let leaves = [leaf(LISTENED_AT)?, leaf(CREATED)?, leaf(USER_ID)?, leaf(ARTISTS)?];
    let listened_unit = per_second(&schema.column(leaves[0])).with_context(|| format!("{LISTENED_AT} in {} is not a timestamp", entry.name))?;
    let created_unit = per_second(&schema.column(leaves[1])).with_context(|| format!("{CREATED} in {} is not a timestamp", entry.name))?;

    // The projection the rows are read through: the four columns' fields,
    // under the file's own root.
    let root = metadata.file_metadata().schema();
    let fields: Vec<Arc<Type>> = root
        .get_fields()
        .iter()
        .filter(|field| [LISTENED_AT, CREATED, USER_ID, ARTISTS].contains(&field.name()))
        .cloned()
        .collect();
    let projection = Type::group_type_builder(root.name())
        .with_fields(fields)
        .build()
        .context("cannot describe the columns to read")?;

    let mut rows = 0u64;
    for group in 0..reader.num_row_groups() {
        // This row group's four columns, fetched where they lie; the previous
        // group's are dropped first, so a day is never held whole in memory.
        sparse.forget_columns();
        let group_meta = metadata.row_group(group);
        for &column in &leaves {
            let (start, len) = group_meta.column(column).byte_range();
            sparse.fetch(start, usize::try_from(len).context("a column larger than memory")?)?;
        }

        let rows_of_group = reader
            .get_row_group(group)
            .with_context(|| format!("cannot open row group {group} of {}", entry.name))?;
        for row in rows_of_group
            .get_row_iter(Some(projection.clone()))
            .with_context(|| format!("cannot read row group {group} of {}", entry.name))?
        {
            let row = row.with_context(|| format!("a row of {} is unreadable", entry.name))?;
            rows += 1;
            let mut listen = Listen {
                created: 0,
                listened: 0,
                listener: 0,
                artists: Vec::new(),
            };
            let mut complete = [false; 3];
            for (name, field) in row.get_column_iter() {
                match name.as_str() {
                    LISTENED_AT => {
                        if let Some(seconds) = seconds(field, listened_unit) {
                            listen.listened = seconds;
                            complete[0] = true;
                        }
                    }
                    CREATED => {
                        if let Some(seconds) = seconds(field, created_unit) {
                            listen.created = seconds;
                            complete[1] = true;
                        }
                    }
                    USER_ID => {
                        if let Field::Long(user) = field {
                            listen.listener = *user;
                            complete[2] = true;
                        }
                    }
                    ARTISTS => {
                        if let Field::ListInternal(list) = field {
                            listen.artists = list
                                .elements()
                                .iter()
                                .filter_map(|element| match element {
                                    Field::Str(text) => Uuid::parse_str(text).ok(),
                                    _ => None,
                                })
                                .collect();
                        }
                    }
                    _ => {}
                }
            }
            // The three are required by the dump's schema; a row without one
            // is not a listen anyone can count.
            if complete.iter().all(|&known| known) {
                each(listen);
            }
        }
    }
    Ok(rows)
}

/// How many of a timestamp column's units make a second, or `None` for a
/// column that is not a timestamp.
///
/// The dumps write their times as INT96, the legacy timestamp Spark still
/// writes by default (measured on the dumps of 2026-09-29 and 2026-10-15):
/// twelve bytes of Julian day and nanoseconds, with no annotation at all. The
/// record reader turns those into milliseconds by itself. An INT64 annotated
/// as a timestamp is read too, in whatever unit it names, so the day the
/// dumps move to the modern type is not a day the feed stops.
fn per_second(column: &ColumnDescriptor) -> Option<i64> {
    if column.physical_type() == PhysicalType::INT96 {
        return Some(1_000);
    }
    match column.logical_type_ref() {
        Some(LogicalType::Timestamp(timestamp)) => Some(match timestamp.unit {
            TimeUnit::MILLIS => 1_000,
            TimeUnit::MICROS => 1_000_000,
            TimeUnit::NANOS => 1_000_000_000,
        }),
        _ => None,
    }
}

/// A timestamp field in whole seconds. The record reader hands back INT96 as
/// milliseconds, and nanoseconds as a plain number, because the old converted
/// types it maps from had no word for them.
fn seconds(field: &Field, per_second: i64) -> Option<i64> {
    match field {
        Field::Long(value) => Some(value.div_euclid(per_second)),
        Field::TimestampMicros(value) => Some(value.div_euclid(1_000_000)),
        Field::TimestampMillis(value) => Some(value.div_euclid(1_000)),
        _ => None,
    }
}

/// A Parquet file of which only some byte ranges are held.
///
/// Positions are the file's own, from its first byte; the tar it sits in is
/// the remote's business. The footer is held for as long as the reader is,
/// the columns of one row group at a time. Asking for anything else is an
/// error rather than a fetch: a reader that wanders off the columns it was
/// told about would quietly turn a third of the file into all of it.
#[derive(Clone)]
struct Sparse {
    inner: Arc<Inner>,
}

struct Inner {
    /// Owned: the reader lives inside a `SerializedFileReader`, which wants
    /// it for as long as it likes.
    remote: Remote,
    /// Where the file's bytes begin inside the tar.
    remote_offset: u64,
    len: u64,
    footer: (u64, Bytes),
    columns: Mutex<BTreeMap<u64, Bytes>>,
}

impl Sparse {
    fn open(remote: &Remote, entry: &Entry) -> Result<Self> {
        if entry.size < 12 {
            bail!("{} is too short to be a Parquet file", entry.name);
        }
        // The last eight bytes are the footer's length and the magic "PAR1".
        let tail = remote.read(entry.offset + entry.size - 8, 8)?;
        if &tail[4..] != b"PAR1" {
            bail!("{} does not end like a Parquet file", entry.name);
        }
        let footer_len = u64::from(u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]));
        let footer_start = entry
            .size
            .checked_sub(8 + footer_len)
            .with_context(|| format!("{} says its footer is longer than the file", entry.name))?;
        let footer = remote.read(
            entry.offset + footer_start,
            usize::try_from(footer_len + 8).context("a footer larger than memory")?,
        )?;

        Ok(Self {
            inner: Arc::new(Inner {
                remote: remote.clone(),
                remote_offset: entry.offset,
                len: entry.size,
                footer: (footer_start, Bytes::from(footer)),
                columns: Mutex::new(BTreeMap::new()),
            }),
        })
    }

    /// Fetches `len` bytes from `start` of the file and holds them.
    fn fetch(&self, start: u64, len: usize) -> Result<()> {
        let bytes = self.inner.remote.read(self.inner.remote_offset + start, len)?;
        self.inner
            .columns
            .lock()
            .expect("the column map is never poisoned")
            .insert(start, Bytes::from(bytes));
        Ok(())
    }

    fn forget_columns(&self) {
        self.inner.columns.lock().expect("the column map is never poisoned").clear();
    }

    /// The held bytes from `start` to the end of the range holding it.
    fn from(&self, start: u64) -> parquet::errors::Result<Bytes> {
        let (footer_start, footer) = &self.inner.footer;
        if start >= *footer_start && start <= footer_start + footer.len() as u64 {
            let skip = usize::try_from(start - footer_start).expect("inside the footer");
            return Ok(footer.slice(skip..));
        }
        let columns = self.inner.columns.lock().expect("the column map is never poisoned");
        if let Some((held_start, held)) = columns.range(..=start).next_back()
            && start <= held_start + held.len() as u64
        {
            let skip = usize::try_from(start - held_start).expect("inside the range");
            return Ok(held.slice(skip..));
        }
        Err(parquet::errors::ParquetError::General(format!(
            "byte {start} of the file was not fetched: only the footer and the counted columns are"
        )))
    }
}

impl Length for Sparse {
    fn len(&self) -> u64 {
        self.inner.len
    }
}

impl ChunkReader for Sparse {
    type T = bytes::buf::Reader<Bytes>;

    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        Ok(self.from(start)?.reader())
    }

    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        let held = self.from(start)?;
        if held.len() < length {
            return Err(parquet::errors::ParquetError::General(format!(
                "bytes {start}..{} of the file were not fetched",
                start + length as u64
            )));
        }
        Ok(held.slice(..length))
    }
}
