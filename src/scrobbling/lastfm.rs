//! A Last.fm history, brought in a file.
//!
//! Last.fm has no export of its own; people get their scrobbles out with a
//! tool, and the tools write CSV. Two shapes are read: a file with a header
//! row naming its columns (`uts`, `utc_time`, `artist`, `artist_mbid`, `track`
//! and their spellings across exporters), and the four bare columns of
//! lastfm-to-csv -- artist, album, track, date. A row lyrid cannot read costs
//! that row and is counted; a file with no time or no artist to read is
//! refused whole.
//!
//! What a file brings opens stars and counts towards what is familiar, and
//! pays no light (ADR 0020): it is whatever its owner says it is. The file
//! itself is not kept -- only the rows the ledger keeps for any listen.

use std::collections::{HashMap, HashSet};

use sqlx::{Connection, PgConnection};
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time};
use uuid::Uuid;

/// The namespace the ids of listens from a file are derived in. Fixed: the
/// same scrobble must get the same id on every upload, for ever.
const NAMESPACE: Uuid = Uuid::from_u128(0x6c79_7269_642d_6c61_7374_666d_2d76_3031);

/// The earliest moment a scrobble can carry: Last.fm began in 2002, and an
/// earlier time is a clock that was never set.
const EARLIEST: i64 = 1_009_843_200;

/// Rows per INSERT: three arrays, so the count is free of Postgres's limit on
/// parameters, and a history of three hundred thousand is thirty statements.
const CHUNK: usize = 10_000;

/// One scrobble as the file tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrobble {
    pub at: OffsetDateTime,
    pub artist: String,
    /// The artist's MusicBrainz id, where the exporter wrote one.
    pub artist_mbid: Option<Uuid>,
    pub track: String,
}

impl Scrobble {
    /// The recording's id in the ledger: derived from the names, so that the
    /// same scrobble in the same file, or in next year's export, is the same
    /// row.
    pub fn recording(&self) -> Uuid {
        let name = format!("{}\u{1f}{}", self.artist.to_lowercase(), self.track.to_lowercase());
        Uuid::new_v5(&NAMESPACE, name.as_bytes())
    }
}

/// What reading a file came to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub scrobbles: Vec<Scrobble>,
    /// Rows that were not a scrobble: no time, a time outside the possible,
    /// no artist.
    pub unreadable: usize,
}

/// Why a file was not read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not text that splits into rows and columns.
    NotCsv,
    /// Rows and columns, but none of them a time or none an artist.
    NoColumns,
}

/// Where the columns that matter are.
#[derive(Debug, Clone, Copy)]
struct Columns {
    at: usize,
    artist: usize,
    artist_mbid: Option<usize>,
    track: Option<usize>,
}

/// A column name with everything but letters and digits taken out, lower
/// case: `Artist MBID`, `artist_mbid` and `artistMbid` are one name.
fn fold(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// The first of `names` the header has, by position.
fn find(header: &[String], names: &[&str]) -> Option<usize> {
    names.iter().find_map(|name| header.iter().position(|column| column == name))
}

impl Columns {
    /// The columns a header names, or `None` when it names no time or no
    /// artist -- which is also how a file with no header looks.
    fn named(header: &csv::StringRecord) -> Option<Self> {
        let header: Vec<String> = header.iter().map(fold).collect();
        Some(Self {
            // A number before a sentence: `uts` is exact, `utc_time` is a
            // rendering of it.
            at: find(
                &header,
                &[
                    "uts",
                    "utstimestamp",
                    "timestamp",
                    "listenedat",
                    "playedat",
                    "date",
                    "datetime",
                    "utctime",
                    "time",
                ],
            )?,
            artist: find(&header, &["artist", "artistname"])?,
            artist_mbid: find(&header, &["artistmbid", "artistmbids"]),
            track: find(&header, &["track", "trackname", "title", "song", "name"]),
        })
    }

    /// lastfm-to-csv's bare four: artist, album, track, date.
    const BARE: Self = Self {
        at: 3,
        artist: 0,
        artist_mbid: None,
        track: Some(2),
    };
}

/// Reads a file's scrobbles.
pub fn parse(text: &str) -> Result<Parsed, Refusal> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(text.as_bytes());
    let mut records = reader.records();

    let first = match records.next() {
        Some(Ok(record)) => record,
        Some(Err(_)) => return Err(Refusal::NotCsv),
        None => return Err(Refusal::NoColumns),
    };
    let mut parsed = Parsed::default();
    let columns = match Columns::named(&first) {
        Some(columns) => columns,
        // No header: the bare shape, if the first row reads as one -- and
        // then the first row is a scrobble like the rest.
        None => match scrobble(&first, Columns::BARE) {
            Some(scrobble) => {
                parsed.scrobbles.push(scrobble);
                Columns::BARE
            }
            None => return Err(Refusal::NoColumns),
        },
    };

    for record in records {
        match record.ok().and_then(|record| scrobble(&record, columns)) {
            Some(scrobble) => parsed.scrobbles.push(scrobble),
            None => parsed.unreadable += 1,
        }
    }
    Ok(parsed)
}

fn scrobble(record: &csv::StringRecord, columns: Columns) -> Option<Scrobble> {
    let field = |index: usize| record.get(index).map(str::trim).unwrap_or_default();
    let at = moment(field(columns.at))?;
    let artist = field(columns.artist);
    if artist.is_empty() {
        return None;
    }
    // The first id of a list: the credit's lead, which is who the name is.
    let artist_mbid = columns.artist_mbid.and_then(|index| {
        field(index)
            .split([',', ';', ' '])
            .find(|id| !id.is_empty())
            .and_then(|id| Uuid::parse_str(id).ok())
    });
    Some(Scrobble {
        at,
        artist: artist.to_string(),
        artist_mbid,
        track: columns.track.map(field).unwrap_or_default().to_string(),
    })
}

/// A time as the exporters write it: Unix seconds or milliseconds, Last.fm's
/// own `31 Jan 2020 12:34`, or ISO 8601 -- all of them in UTC, which is what
/// Last.fm reports. `None` for anything else, for zero (a track that was
/// playing when the file was made) and for a moment Last.fm cannot have seen.
pub fn moment(text: &str) -> Option<OffsetDateTime> {
    let text = text.trim();
    let at = if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
        let number: i64 = text.parse().ok()?;
        let seconds = if text.len() >= 13 { number / 1000 } else { number };
        OffsetDateTime::from_unix_timestamp(seconds).ok()?
    } else if let Some(at) = last_fm(text) {
        at
    } else {
        iso(text)?
    };
    let seconds = at.unix_timestamp();
    let latest = OffsetDateTime::now_utc().unix_timestamp() + 24 * 60 * 60;
    (EARLIEST..=latest).contains(&seconds).then_some(at)
}

/// `31 Jan 2020 12:34`, with or without a comma before the time.
fn last_fm(text: &str) -> Option<OffsetDateTime> {
    let parts: Vec<&str> = text.split([' ', ',']).filter(|part| !part.is_empty()).collect();
    let [day, month, year, clock] = parts.as_slice() else {
        return None;
    };
    let month = match month.to_ascii_lowercase().get(..3)? {
        "jan" => Month::January,
        "feb" => Month::February,
        "mar" => Month::March,
        "apr" => Month::April,
        "may" => Month::May,
        "jun" => Month::June,
        "jul" => Month::July,
        "aug" => Month::August,
        "sep" => Month::September,
        "oct" => Month::October,
        "nov" => Month::November,
        "dec" => Month::December,
        _ => return None,
    };
    let date = Date::from_calendar_date(year.parse().ok()?, month, day.parse().ok()?).ok()?;
    Some(PrimitiveDateTime::new(date, clock_of(clock)?).assume_utc())
}

/// `2020-01-31T12:34:56Z`, `2020-01-31 12:34:56`, `2020-01-31T12:34:56+03:00`.
fn iso(text: &str) -> Option<OffsetDateTime> {
    let (date, rest) = text.split_once(['T', ' '])?;
    let mut numbers = date.split('-');
    let year: i32 = numbers.next()?.parse().ok()?;
    let month: u8 = numbers.next()?.parse().ok()?;
    let day: u8 = numbers.next()?.parse().ok()?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;

    // The clock, then an offset if there is one: `Z`, or a sign and hours.
    let (clock, offset) = match rest.find(['Z', 'z', '+', '-']) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    let at = PrimitiveDateTime::new(date, clock_of(clock)?).assume_utc();
    let offset_seconds = match offset {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            let digits: String = offset[1..].chars().filter(char::is_ascii_digit).collect();
            let hours: i64 = digits.get(..2)?.parse().ok()?;
            let minutes: i64 = digits.get(2..4).map_or(Some(0), |minutes| minutes.parse().ok())?;
            sign * (hours * 3600 + minutes * 60)
        }
    };
    Some(at - time::Duration::seconds(offset_seconds))
}

/// `12:34` or `12:34:56`, a fraction of a second ignored.
fn clock_of(text: &str) -> Option<Time> {
    let text = text.split('.').next()?;
    let mut parts = text.split(':');
    let hour: u8 = parts.next()?.parse().ok()?;
    let minute: u8 = parts.next()?.parse().ok()?;
    let second: u8 = parts.next().map_or(Some(0), |second| second.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Time::from_hms(hour, minute, second).ok()
}

/// Which of the artists sharing a name the name means.
///
/// A name in a Last.fm file means what Last.fm means by it: the best-known
/// artist of that name. So the brightest one, if it is at least twice as
/// bright as the next -- measured on 2026-10-13 on the full canon, 4,703 of
/// the 20,000 brightest artists share their name with someone, and for 4,486
/// of them the brightest is twice the next. Closer than that is a coin flip,
/// and a wrong star is worse than none: the listen is kept, unnamed.
pub fn pick(candidates: &[(Uuid, f32)]) -> Option<Uuid> {
    match candidates {
        [] => None,
        [(only, _)] => Some(*only),
        _ => {
            let mut ranked = candidates.to_vec();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let (best, best_weight) = ranked[0];
            let second = ranked[1].1;
            (best_weight > 0.0 && best_weight >= second * 2.0).then_some(best)
        }
    }
}

/// What bringing a file in came to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Imported {
    /// Scrobbles read from the file.
    pub scrobbles: usize,
    /// Rows the file had that were not a scrobble.
    pub unreadable: usize,
    /// Listens written to the ledger.
    pub listens: usize,
    /// Of those, the ones whose artist lyrid could name.
    pub named: usize,
    /// Artists heard for the first time.
    pub opened: usize,
}

/// Writes a file's scrobbles into the ledger, once.
///
/// A scrobble the ledger already holds -- the same file uploaded again -- is
/// refused by the key; one at the very second ListenBrainz reports a listen is
/// that listen told twice, and ListenBrainz's telling stands.
pub async fn import(conn: &mut PgConnection, user_id: i32, parsed: Parsed) -> sqlx::Result<Imported> {
    let artists = name_artists(&mut *conn, &parsed.scrobbles).await?;
    let mut rows: Vec<(OffsetDateTime, Uuid, Option<Uuid>)> = parsed
        .scrobbles
        .iter()
        .map(|scrobble| {
            let artist = scrobble
                .artist_mbid
                .filter(|mbid| artists.known.contains(mbid))
                .or_else(|| artists.by_name.get(&scrobble.artist.to_lowercase()).copied());
            (scrobble.at, scrobble.recording(), artist)
        })
        .collect();
    rows.sort();
    rows.dedup_by_key(|row| (row.0, row.1));

    let mut tx = conn.begin().await?;
    super::hold_ledger(&mut tx, user_id).await?;

    let named: Vec<Uuid> = rows.iter().filter_map(|row| row.2).collect::<HashSet<_>>().into_iter().collect();
    let ever: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT artist FROM listen, unnest(artist_mbids) AS artist
         WHERE user_id = $1 AND artist = ANY($2)",
    )
    .bind(user_id)
    .bind(&named)
    .fetch_all(&mut *tx)
    .await?;
    let ever: HashSet<Uuid> = ever.into_iter().collect();

    let mut imported = Imported {
        scrobbles: parsed.scrobbles.len(),
        unreadable: parsed.unreadable,
        ..Imported::default()
    };
    let mut opened = HashSet::new();
    for chunk in rows.chunks(CHUNK) {
        let moments: Vec<OffsetDateTime> = chunk.iter().map(|row| row.0).collect();
        let recordings: Vec<Uuid> = chunk.iter().map(|row| row.1).collect();
        let artists: Vec<Option<Uuid>> = chunk.iter().map(|row| row.2).collect();
        let written: Vec<Vec<Uuid>> = sqlx::query_scalar(
            "INSERT INTO listen (user_id, listened_at, recording, artist_mbids, seconds, light, source)
             SELECT $1, t.at, t.recording,
                    CASE WHEN t.artist IS NULL THEN '{}'::uuid[] ELSE ARRAY[t.artist] END,
                    NULL, NULL, 'lastfm'
             FROM unnest($2::timestamptz[], $3::uuid[], $4::uuid[]) AS t (at, recording, artist)
             WHERE NOT EXISTS (
                 SELECT 1 FROM listen l
                 WHERE l.user_id = $1 AND l.listened_at = t.at AND l.source = 'listenbrainz'
             )
             ON CONFLICT DO NOTHING
             RETURNING artist_mbids",
        )
        .bind(user_id)
        .bind(&moments)
        .bind(&recordings)
        .bind(&artists)
        .fetch_all(&mut *tx)
        .await?;
        for credit in written {
            imported.listens += 1;
            if let Some(artist) = credit.first() {
                imported.named += 1;
                if !ever.contains(artist) {
                    opened.insert(*artist);
                }
            }
        }
    }
    imported.opened = opened.len();

    tx.commit().await?;
    Ok(imported)
}

/// Forgets every listen a file brought. What ListenBrainz said stays.
pub async fn forget(conn: &mut PgConnection, user_id: i32) -> sqlx::Result<u64> {
    let mut tx = conn.begin().await?;
    super::hold_ledger(&mut tx, user_id).await?;
    let gone = sqlx::query("DELETE FROM listen WHERE user_id = $1 AND source = 'lastfm'")
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(gone)
}

/// The canon's answer about a file's artists.
struct Named {
    /// The ids the file carries that the canon knows.
    known: HashSet<Uuid>,
    /// Each name, lower case, and the artist it means, where it means one.
    by_name: HashMap<String, Uuid>,
}

async fn name_artists(conn: &mut PgConnection, scrobbles: &[Scrobble]) -> sqlx::Result<Named> {
    let ids: Vec<Uuid> = scrobbles
        .iter()
        .filter_map(|scrobble| scrobble.artist_mbid)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let known: Vec<Uuid> = sqlx::query_scalar("SELECT mbid FROM artist WHERE mbid = ANY($1)")
        .bind(&ids)
        .fetch_all(&mut *conn)
        .await?;
    let known: HashSet<Uuid> = known.into_iter().collect();

    // Names only for the scrobbles whose id did not settle it: an id the
    // canon knows is better than any reading of a name.
    let names: Vec<String> = scrobbles
        .iter()
        .filter(|scrobble| !scrobble.artist_mbid.is_some_and(|mbid| known.contains(&mbid)))
        .map(|scrobble| scrobble.artist.to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let candidates: Vec<(String, Uuid, f32)> = sqlx::query_as(
        "SELECT lower(a.name), a.mbid, COALESCE(pr.weight, 0)
         FROM artist a
         LEFT JOIN artist_prominence pr
             ON pr.artist_id = a.id
            AND pr.metric_id = (SELECT metric_id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         WHERE lower(a.name) = ANY($1)",
    )
    .bind(&names)
    .fetch_all(&mut *conn)
    .await?;

    let mut grouped: HashMap<String, Vec<(Uuid, f32)>> = HashMap::new();
    for (name, mbid, weight) in candidates {
        grouped.entry(name).or_default().push((mbid, weight));
    }
    let by_name = grouped
        .into_iter()
        .filter_map(|(name, candidates)| pick(&candidates).map(|mbid| (name, mbid)))
        .collect();
    Ok(Named { known, by_name })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> i64 {
        moment(text).unwrap_or_else(|| panic!("{text} is a moment")).unix_timestamp()
    }

    #[test]
    fn every_way_the_exporters_write_a_time_is_the_same_moment() {
        // 2020-01-31 12:34:00 UTC
        let expected = 1_580_474_040;
        assert_eq!(at("1580474040"), expected);
        assert_eq!(at("1580474040000"), expected, "milliseconds");
        assert_eq!(at("31 Jan 2020 12:34"), expected, "Last.fm's own");
        assert_eq!(at("31 Jan 2020, 12:34"), expected);
        assert_eq!(at("2020-01-31T12:34:00Z"), expected);
        assert_eq!(at("2020-01-31 12:34:00"), expected, "no zone is UTC, as Last.fm reports");
        assert_eq!(at("2020-01-31T15:34:00+03:00"), expected);
        assert_eq!(at("2020-01-31T07:34:00-05:00"), expected);
    }

    #[test]
    fn a_time_that_cannot_be_a_scrobble_is_none() {
        assert_eq!(moment("0"), None, "the track that was playing when the file was made");
        assert_eq!(moment(""), None);
        assert_eq!(moment("1970-01-01 00:00:00"), None, "a clock never set");
        assert_eq!(moment("2999-01-01 00:00:00"), None, "the future");
        assert_eq!(moment("yesterday"), None);
        assert_eq!(moment("31 Foo 2020 12:34"), None);
        assert_eq!(moment("2020-02-31 12:00"), None, "no such day");
    }

    const ROYKSOPP: &str = "1c70a3fc-fa3c-4be1-8b55-c3192db8a884";

    #[test]
    fn a_file_with_a_header_is_read_by_its_column_names() {
        let file = format!(
            "\u{feff}uts,utc_time,artist,artist_mbid,album,album_mbid,track,track_mbid\n\
             1580474040,\"31 Jan 2020, 12:34\",Röyksopp,{ROYKSOPP},Melody A.M.,,Eple,\n\
             1580474300,\"31 Jan 2020, 12:38\",\"Simon & Garfunkel\",,\"Bridge Over Troubled Water\",,\"The Boxer, live\",\n\
             0,,Now Playing,,,,Something,\n"
        );
        let parsed = parse(&file).unwrap();
        assert_eq!(parsed.unreadable, 1, "the row with no time");
        assert_eq!(parsed.scrobbles.len(), 2);
        assert_eq!(parsed.scrobbles[0].artist_mbid, Some(Uuid::parse_str(ROYKSOPP).unwrap()));
        assert_eq!(parsed.scrobbles[0].track, "Eple");
        assert_eq!(parsed.scrobbles[1].artist, "Simon & Garfunkel");
        assert_eq!(parsed.scrobbles[1].track, "The Boxer, live", "a quoted comma is part of the name");
        assert_eq!(parsed.scrobbles[1].artist_mbid, None);
    }

    #[test]
    fn column_names_are_read_however_they_are_spelled() {
        let parsed = parse("Artist Name,Track Name,Played At\nAir,La femme d'argent,2020-01-31T12:34:00Z\n").unwrap();
        assert_eq!(parsed.scrobbles.len(), 1);
        assert_eq!(parsed.scrobbles[0].artist, "Air");
        assert_eq!(parsed.scrobbles[0].at.unix_timestamp(), 1_580_474_040);
    }

    #[test]
    fn the_bare_four_columns_are_read_from_the_first_row() {
        let parsed = parse("Röyksopp,Melody A.M.,Eple,31 Jan 2020 12:34\nAir,Moon Safari,Kelly Watch the Stars,31 Jan 2020 12:39\n").unwrap();
        assert_eq!(parsed.scrobbles.len(), 2, "no header: the first row is a scrobble too");
        assert_eq!(parsed.scrobbles[1].track, "Kelly Watch the Stars");
    }

    #[test]
    fn a_file_with_nothing_to_read_is_refused_whole() {
        assert_eq!(parse(""), Err(Refusal::NoColumns));
        assert_eq!(parse("name,colour\nplum,purple\n"), Err(Refusal::NoColumns));
    }

    #[test]
    fn the_same_scrobble_has_the_same_id_whatever_its_case() {
        let scrobble = |artist: &str, track: &str| Scrobble {
            at: OffsetDateTime::UNIX_EPOCH,
            artist: artist.to_string(),
            artist_mbid: None,
            track: track.to_string(),
        };
        assert_eq!(scrobble("Air", "Talisman").recording(), scrobble("AIR", "talisman").recording());
        assert_ne!(scrobble("Air", "Talisman").recording(), scrobble("Air", "Venus").recording());
        // The separator keeps "a" + "bc" apart from "ab" + "c".
        assert_ne!(scrobble("a", "bc").recording(), scrobble("ab", "c").recording());
    }

    #[test]
    fn a_name_means_its_best_known_artist_only_when_clearly_so() {
        let id = Uuid::from_u128;
        assert_eq!(pick(&[]), None);
        assert_eq!(pick(&[(id(1), 0.0)]), Some(id(1)), "the only artist of a name is that name");
        assert_eq!(pick(&[(id(1), 0.1), (id(2), 3.0)]), Some(id(2)), "Air is the French band");
        assert_eq!(pick(&[(id(1), 2.0), (id(2), 3.0)]), None, "two of a kind: a coin flip, so nobody");
        assert_eq!(pick(&[(id(1), 0.0), (id(2), 0.0)]), None, "nobody known by the name at all");
    }
}
