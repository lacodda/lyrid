//! Which artist a MusicBrainz recording belongs to, read from a full export.
//!
//! The canon keeps no recordings: the sky is made of artists, and three dozen
//! million rows of tracks would be weight with no reader. Some datasets are
//! keyed by recording all the same -- AcousticBrainz measured audio files, and
//! a file is a recording -- so they need the step from a recording MBID to a
//! star. This module takes it, from the same `mbdump.tar.bz2` the canon is
//! built from, and holds nothing past the import that asked.
//!
//! Three tables, read in one pass because bzip2 cannot seek:
//!
//! - `recording` -- the recording's MBID and its artist credit;
//! - `recording_gid_redirect` -- MBIDs that have since been merged into
//!   another recording, which a dataset frozen years ago is full of;
//! - `artist_credit_name` -- who a credit names first.
//!
//! The first credited artist is the one a recording belongs to, by the same
//! rule the canon places a release group in a system: a credit may name
//! several, and "A feat. B" is A's track.
//!
//! The dump need not be the one the canon was built from. MusicBrainz keeps
//! only the two latest full exports, so the dump at hand is usually newer; an
//! artist id is stable for as long as the artist exists, and one that has
//! since been merged away is simply not found in the canon by whoever asks.
#![allow(clippy::doc_markdown, reason = "documentation quotes upstream table and column names throughout")]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use bzip2::read::MultiBzDecoder;
use uuid::Uuid;

use super::copy_text::Reader;

/// What resolving a set of recordings came to.
pub struct Resolved {
    /// Recording MBID -> the MusicBrainz id of its first credited artist.
    pub artists: HashMap<Uuid, i32>,
    /// The export's TIMESTAMP, which names the version the answers come from.
    pub timestamp: Option<String>,
    /// Found under their own MBID.
    pub direct: usize,
    /// Found only through a merge redirect.
    pub redirected: usize,
    /// Not in the export at all: deleted since the dataset was made.
    pub unknown: usize,
}

/// Resolves `wanted` recording MBIDs to the artists they are credited to.
pub fn artists_of(dump: &Path, wanted: &HashSet<Uuid>) -> Result<Resolved> {
    let file = File::open(dump).with_context(|| format!("cannot open {}", dump.display()))?;
    // A full export is a concatenation of bzip2 streams; a single-stream
    // decoder would stop at the first and silently resolve nothing past it.
    let decoder = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, file));
    let mut archive = tar::Archive::new(decoder);
    let mut pass = Pass::new(wanted);
    let mut seen = Seen::default();

    for entry in archive.entries().context("the MusicBrainz dump is not a readable tar archive")? {
        let mut entry = entry.context("failed to read an archive entry")?;
        let path = entry.path().context("archive entry has an unreadable path")?.into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if name == "TIMESTAMP" {
            let mut text = String::new();
            entry.read_to_string(&mut text).ok();
            pass.timestamp = Some(text.trim().to_string());
            continue;
        }
        if path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) != Some("mbdump") {
            continue;
        }

        let reader = BufReader::with_capacity(1 << 20, entry);
        match name {
            "recording" => {
                tracing::info!("reading recordings");
                pass.read_recordings(reader)?;
                seen.recordings = true;
            }
            "recording_gid_redirect" => {
                pass.read_redirects(reader)?;
                seen.redirects = true;
            }
            "artist_credit_name" => {
                pass.read_credit_names(reader)?;
                seen.credits = true;
            }
            _ => {}
        }
    }

    // Each table missing would resolve fewer recordings without saying why:
    // no redirects loses every merged recording, no credits loses all of them.
    if !(seen.recordings && seen.redirects && seen.credits) {
        bail!("the dump lacks recording, recording_gid_redirect or artist_credit_name: is this mbdump.tar.bz2 from a full export?");
    }
    Ok(pass.resolve())
}

#[derive(Default)]
struct Seen {
    recordings: bool,
    redirects: bool,
    credits: bool,
}

/// The single pass, table by table, in whatever order the archive holds them.
struct Pass<'a> {
    wanted: &'a HashSet<Uuid>,
    /// recording.id -> artist_credit, for every recording: a redirect names
    /// its target by row id, and the redirect table may come before or after
    /// the recordings, so any of them may turn out to be needed. Indexed
    /// densely, because the ids are dense and a map of forty million entries
    /// would cost several times as much; 0 is "no such recording".
    credit_of: Vec<i32>,
    /// Wanted MBID -> recording.id, for MBIDs that are a recording's own.
    direct: HashMap<Uuid, i32>,
    /// Wanted MBID -> recording.id it was merged into.
    redirect: HashMap<Uuid, i32>,
    /// artist_credit.id -> the artist credited first.
    first_artist: HashMap<i32, i32>,
    timestamp: Option<String>,
}

impl<'a> Pass<'a> {
    fn new(wanted: &'a HashSet<Uuid>) -> Self {
        Self {
            wanted,
            credit_of: Vec::new(),
            direct: HashMap::new(),
            redirect: HashMap::new(),
            first_artist: HashMap::new(),
            timestamp: None,
        }
    }

    /// recording: id, gid, name, artist_credit, length, comment, edits_pending,
    /// last_updated, video
    fn read_recordings<R: BufRead>(&mut self, reader: R) -> Result<()> {
        let mut rows = Reader::new(reader);
        while let Some(row) = rows.next_row().context("failed to read a recording row")? {
            let (Some(id), Some(gid), Some(credit)) = (row.parse::<i32>(0), row.get(1), row.parse::<i32>(3)) else {
                continue;
            };
            let Ok(index) = usize::try_from(id) else { continue };
            if index >= self.credit_of.len() {
                self.credit_of.resize(index + 1, 0);
            }
            self.credit_of[index] = credit;

            if let Ok(gid) = Uuid::parse_str(gid)
                && self.wanted.contains(&gid)
            {
                self.direct.insert(gid, id);
            }
        }
        Ok(())
    }

    /// recording_gid_redirect: gid, new_id, created
    fn read_redirects<R: BufRead>(&mut self, reader: R) -> Result<()> {
        let mut rows = Reader::new(reader);
        while let Some(row) = rows.next_row().context("failed to read a redirect row")? {
            let (Some(gid), Some(new_id)) = (row.get(0), row.parse::<i32>(1)) else {
                continue;
            };
            if let Ok(gid) = Uuid::parse_str(gid)
                && self.wanted.contains(&gid)
            {
                self.redirect.insert(gid, new_id);
            }
        }
        Ok(())
    }

    /// artist_credit_name: artist_credit, position, artist, name, join_phrase
    fn read_credit_names<R: BufRead>(&mut self, reader: R) -> Result<()> {
        let mut rows = Reader::new(reader);
        while let Some(row) = rows.next_row().context("failed to read an artist credit row")? {
            if row.parse::<i32>(1) != Some(0) {
                continue;
            }
            if let (Some(credit), Some(artist)) = (row.parse::<i32>(0), row.parse::<i32>(2)) {
                self.first_artist.insert(credit, artist);
            }
        }
        Ok(())
    }

    fn resolve(self) -> Resolved {
        let mut resolved = Resolved {
            artists: HashMap::with_capacity(self.wanted.len()),
            timestamp: self.timestamp,
            direct: 0,
            redirected: 0,
            unknown: 0,
        };

        for gid in self.wanted {
            // An MBID is a recording's own or a redirect, never both: merging
            // a recording turns its MBID into a redirect to the survivor.
            let (id, redirected) = match (self.direct.get(gid), self.redirect.get(gid)) {
                (Some(&id), _) => (id, false),
                (None, Some(&id)) => (id, true),
                (None, None) => {
                    resolved.unknown += 1;
                    continue;
                }
            };
            let credit = usize::try_from(id).ok().and_then(|index| self.credit_of.get(index)).copied().unwrap_or(0);
            let Some(&artist) = self.first_artist.get(&credit) else {
                resolved.unknown += 1;
                continue;
            };

            resolved.artists.insert(*gid, artist);
            if redirected {
                resolved.redirected += 1;
            } else {
                resolved.direct += 1;
            }
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEPT: &str = "11111111-1111-1111-1111-111111111111";
    const MERGED: &str = "22222222-2222-2222-2222-222222222222";
    const SURVIVOR: &str = "33333333-3333-3333-3333-333333333333";
    const GONE: &str = "44444444-4444-4444-4444-444444444444";
    const UNASKED: &str = "55555555-5555-5555-5555-555555555555";

    fn uuid(text: &str) -> Uuid {
        Uuid::parse_str(text).unwrap()
    }

    /// Runs the three readers over literal COPY TEXT in the given table
    /// order, the way the archive would hand them over.
    fn resolve(order: &[&str], wanted: &[&str]) -> Resolved {
        let wanted: HashSet<Uuid> = wanted.iter().map(|w| uuid(w)).collect();
        let mut pass = Pass::new(&wanted);
        let recordings = format!(
            "10\t{KEPT}\tA song\t100\t\\N\t\\N\t0\t\\N\tf\n30\t{SURVIVOR}\tB song\t200\t\\N\t\\N\t0\t\\N\tf\n40\t{UNASKED}\tC\t100\t\\N\t\\N\t0\t\\N\tf\n"
        );
        let redirects = format!("{MERGED}\t30\t2020-01-01\n");
        // Credit 100 names artist 7 first and artist 8 second; credit 200
        // names artist 9 alone.
        let credits = "100\t1\t8\tB\t\n100\t0\t7\tA\t feat. \n200\t0\t9\tC\t\n";
        for table in order {
            match *table {
                "recording" => pass.read_recordings(recordings.as_bytes()).unwrap(),
                "recording_gid_redirect" => pass.read_redirects(redirects.as_bytes()).unwrap(),
                "artist_credit_name" => pass.read_credit_names(credits.as_bytes()).unwrap(),
                other => panic!("no reader for {other}"),
            }
        }
        pass.resolve()
    }

    #[test]
    fn a_recording_belongs_to_the_artist_credited_first() {
        let resolved = resolve(&["recording", "recording_gid_redirect", "artist_credit_name"], &[KEPT]);
        assert_eq!(resolved.artists.get(&uuid(KEPT)), Some(&7), "credit 100 names artist 7 at position 0");
        assert_eq!(resolved.direct, 1);
    }

    #[test]
    fn a_merged_recording_is_followed_to_the_one_it_was_merged_into() {
        let resolved = resolve(&["recording", "recording_gid_redirect", "artist_credit_name"], &[MERGED]);
        assert_eq!(
            resolved.artists.get(&uuid(MERGED)),
            Some(&9),
            "the redirect leads to recording 30, credited to 9"
        );
        assert_eq!(resolved.redirected, 1);
    }

    #[test]
    fn the_order_of_the_tables_in_the_archive_changes_nothing() {
        let wanted = [KEPT, MERGED];
        let forward = resolve(&["recording", "recording_gid_redirect", "artist_credit_name"], &wanted);
        let backward = resolve(&["artist_credit_name", "recording_gid_redirect", "recording"], &wanted);
        assert_eq!(forward.artists, backward.artists);
        assert_eq!(forward.artists.len(), 2);
    }

    #[test]
    fn a_recording_the_export_no_longer_has_is_counted_rather_than_guessed() {
        let resolved = resolve(&["recording", "recording_gid_redirect", "artist_credit_name"], &[KEPT, GONE]);
        assert_eq!(resolved.unknown, 1);
        assert!(!resolved.artists.contains_key(&uuid(GONE)));
    }

    #[test]
    fn only_the_recordings_asked_for_are_resolved() {
        let resolved = resolve(&["recording", "recording_gid_redirect", "artist_credit_name"], &[KEPT]);
        assert!(!resolved.artists.contains_key(&uuid(UNASKED)));
        assert!(!resolved.artists.contains_key(&uuid(SURVIVOR)), "the survivor's own MBID was not asked for");
    }
}
