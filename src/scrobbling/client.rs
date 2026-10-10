//! The ListenBrainz API, as much of it as lyrid uses: one call to learn whose a
//! token is, and one to read a person's listens.
//!
//! Blocking, on purpose, like the rest of lyrid's outbound HTTP: callers on
//! the server run it on the blocking pool. Nothing here is on the path of
//! drawing the sky or opening a card -- a read is something the background
//! does, or something a person asked for by pressing a button -- which is what
//! keeps it inside ADR 0002.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::time::Duration;

use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// Where ListenBrainz answers, unless `LYRID_LISTENBRAINZ_URL` says otherwise.
pub const DEFAULT_URL: &str = "https://api.listenbrainz.org";

/// The most listens ListenBrainz hands out per request.
const PAGE: usize = 1000;

/// The span of time one request asks about, in seconds: three days.
///
/// Measured on 2026-10-10, not assumed. Asked for listens after a moment and
/// nothing else, ListenBrainz scans forward until it has filled the page or
/// reached the present -- so the last page of anyone who has not listened for
/// a while scans months of nothing, and was cut off by ListenBrainz's own edge
/// at 40 seconds, even for a page of ten. Bounded by `max_ts` the same request
/// answered in eight. Three days holds the usual read -- the two-day overlap
/// and what came since -- in one request.
pub(crate) const WINDOW: i64 = 3 * 24 * 60 * 60;

/// How many requests one read makes before stopping where it got to.
///
/// A routine read is one or two. A read that reaches this has months behind
/// it, and the cursor keeps its place for the next read to carry on from.
const MAX_REQUESTS: usize = 50;

/// The longest lyrid waits for ListenBrainz's rate limit to reset before
/// giving up on this read and trying again later.
const MAX_WAIT: Duration = Duration::from_secs(60);

/// Why a read or a check did not happen. Each is stored by its code, which the
/// interface turns into words in the reader's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// No answer, or an answer that was a server's error.
    Unreachable,
    /// ListenBrainz does not know the name: the account was renamed or closed.
    UnknownUser,
    /// ListenBrainz asked us to slow down for longer than we wait.
    Throttled,
    /// An answer that was not the shape ListenBrainz promises. Said out loud
    /// rather than read around: a format that changed under us is a reason to
    /// stop paying, not a reason to pay for half of every page.
    Malformed,
}

impl Failure {
    /// The code stored in `listenbrainz_link.failure`, whose check constraint
    /// names the same four.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::UnknownUser => "unknown_user",
            Self::Throttled => "throttled",
            Self::Malformed => "malformed",
        }
    }
}

/// One listen, reduced to what the game uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listen {
    pub listened_at: OffsetDateTime,
    pub recording_msid: Uuid,
    /// The credited artists, in credit order. Empty when ListenBrainz could
    /// not say who it was.
    pub artists: Vec<Uuid>,
    /// The recording's length as reported, in whole seconds.
    pub seconds: Option<i32>,
}

/// What one read brought back.
#[derive(Debug)]
pub struct Fetched {
    pub listens: Vec<Listen>,
    /// The moment up to which the listens are complete: when the read began,
    /// or as far as it got if it stopped early.
    pub through: OffsetDateTime,
    /// Why the read stopped before the present, when it did after getting
    /// somewhere. What it got is kept: a read that fails on its twentieth
    /// request has still read nineteen, and throwing them away would make a
    /// long read impossible on a day ListenBrainz is slow.
    pub stopped: Option<Failure>,
}

/// A client for one ListenBrainz.
#[derive(Clone)]
pub struct Client {
    base: String,
    agent: ureq::Agent,
    page: usize,
}

impl Client {
    pub fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            // Longer than ListenBrainz's own edge, which cuts a request off at
            // about forty seconds: a slow answer is ListenBrainz's to give up
            // on, not ours.
            .timeout_global(Some(Duration::from_secs(60)))
            // Statuses are read rather than thrown: a 429 carries the wait in
            // its headers, and a 404 means something different from a 500.
            .http_status_as_error(false)
            // ListenBrainz asks API users to say who they are.
            .user_agent(concat!("lyrid/", env!("CARGO_PKG_VERSION"), " (+https://github.com/lacodda/lyrid)"))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent,
            page: PAGE,
        }
    }

    /// A client that reads in pages of `page`, so paging is exercised by a
    /// test without a thousand listens of fixture.
    #[cfg(test)]
    pub fn with_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// Whose a token is: the name ListenBrainz knows its owner by, or `None`
    /// when it is not a token ListenBrainz recognises.
    ///
    /// The token is sent once, here, and is not kept by anything that calls
    /// this. Reading listens does not need it.
    pub fn whose(&self, token: &str) -> Result<Option<String>, Failure> {
        #[derive(Deserialize)]
        struct Validation {
            valid: bool,
            user_name: Option<String>,
        }

        let mut response = self
            .agent
            .get(format!("{}/1/validate-token", self.base))
            .header("Authorization", format!("Token {token}"))
            .call()
            .map_err(|error| unreachable(&error))?;

        let status = response.status().as_u16();
        if status >= 500 {
            return Err(Failure::Unreachable);
        }
        if status == 429 {
            return Err(Failure::Throttled);
        }
        let body = response.body_mut().read_to_string().map_err(|error| unreachable(&error))?;

        // ListenBrainz answers a token it does not know with 200 and
        // `valid: false`; anything else outside 2xx is not an answer about
        // the token at all.
        if !(200..300).contains(&status) {
            return Err(Failure::Malformed);
        }
        let validation: Validation = serde_json::from_str(&body).map_err(|_| Failure::Malformed)?;
        match (validation.valid, validation.user_name) {
            (true, Some(name)) if !name.trim().is_empty() => Ok(Some(name)),
            (true, _) => Err(Failure::Malformed),
            (false, _) => Ok(None),
        }
    }

    /// Every listen `name` has from `from` until now, oldest first.
    ///
    /// Read window by window, each bounded on both sides, and paged forwards
    /// within a window. Asked with `min_ts`, ListenBrainz returns the *oldest*
    /// listens after it, newest first within the page -- measured on
    /// 2026-10-10 with and without `max_ts` -- so a full page means the window
    /// holds more after its newest listen, and a short one means the window is
    /// done.
    pub fn listens(&self, name: &str, from: OffsetDateTime) -> Result<Fetched, Failure> {
        let began = OffsetDateTime::now_utc();
        let end = began.unix_timestamp();
        let mut listens = Vec::new();
        let mut seen = HashSet::new();
        // Both bounds are exclusive. `after` is one second before what is
        // wanted, so a listen at exactly `from` is read rather than lost
        // between two reads; `before` is one past the window's last second.
        let start = from.unix_timestamp() - 1;
        let mut after = start;
        let mut before = (after + 1 + WINDOW).min(end + 1);

        for _ in 0..MAX_REQUESTS {
            let page = match self.page_between(name, after, before) {
                Ok(page) => page,
                // Nothing gained yet: the read failed.
                Err(failure) if after == start => return Err(failure),
                // Somewhere gained: keep it, and say why it stopped.
                Err(failure) => {
                    return Ok(Fetched {
                        listens,
                        through: moment(after, began),
                        stopped: Some(failure),
                    });
                }
            };
            let full = page.len() >= self.page;
            let newest = page.iter().map(|listen| listen.listened_at.unix_timestamp()).max();

            for listen in page {
                // A listen sitting on a page boundary can come back on both
                // pages; it is one listen.
                if seen.insert((listen.listened_at, listen.recording_msid)) {
                    listens.push(listen);
                }
            }

            if let (true, Some(newest)) = (full, newest) {
                // More in this window. The next page starts a second before
                // this one's newest listen, not at it: a second listen in
                // that same second would otherwise fall between the pages.
                // What comes back twice is dropped by `seen`. Only a page
                // filled entirely by one second moves past it, because
                // nothing else would.
                after = if newest - 1 > after { newest - 1 } else { newest };
                continue;
            }

            // This window is done.
            if before > end {
                return Ok(Fetched {
                    listens,
                    through: began,
                    stopped: None,
                });
            }
            after = before - 1;
            before = (after + 1 + WINDOW).min(end + 1);
        }

        // Stopped at the request limit: complete only up to where it got.
        Ok(Fetched {
            listens,
            through: moment(after, began),
            stopped: None,
        })
    }

    /// One page of the listens between `after` and `before`, both exclusive,
    /// waiting out the rate limit once.
    fn page_between(&self, name: &str, after: i64, before: i64) -> Result<Vec<Listen>, Failure> {
        let url = format!("{}/1/user/{}/listens", self.base, encode_path(name));
        let mut waited = false;
        loop {
            let mut response = self
                .agent
                .get(&url)
                .query("min_ts", after.to_string())
                .query("max_ts", before.to_string())
                .query("count", self.page.to_string())
                .call()
                .map_err(|error| unreachable(&error))?;

            let status = response.status().as_u16();
            let wait = reset_in(response.headers());

            if status == 429 {
                match wait {
                    Some(wait) if !waited && wait <= MAX_WAIT => {
                        std::thread::sleep(wait);
                        waited = true;
                        continue;
                    }
                    _ => return Err(Failure::Throttled),
                }
            }
            if status == 404 {
                return Err(Failure::UnknownUser);
            }
            if status >= 500 {
                tracing::warn!(status, "ListenBrainz answered a read with a server error");
                return Err(Failure::Unreachable);
            }
            if !(200..300).contains(&status) {
                tracing::warn!(status, "ListenBrainz answered a read with a status lyrid does not expect");
                return Err(Failure::Malformed);
            }

            let body = response.body_mut().read_to_string().map_err(|error| unreachable(&error))?;
            let page = parse_page(&body)?;

            // Spent the last request of this window: wait for the next one
            // before the caller asks again, rather than collecting a 429.
            if remaining(response.headers()) == Some(0)
                && let Some(wait) = wait
            {
                std::thread::sleep(wait.min(MAX_WAIT));
            }
            return Ok(page);
        }
    }
}

/// A Unix second as a moment, or `fallback` for one out of range.
fn moment(seconds: i64, fallback: OffsetDateTime) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(seconds).unwrap_or(fallback)
}

/// A request that got no usable answer, logged with its cause.
///
/// The stored code says only "unreachable", which is all the person needs;
/// whoever runs the stand needs to know whether that was DNS, TLS, a timeout or
/// a body cut short. The error names the address and never a token: the token
/// travels in a header, and ureq's errors do not carry headers.
fn unreachable(error: &ureq::Error) -> Failure {
    tracing::warn!(%error, "ListenBrainz could not be reached");
    Failure::Unreachable
}

/// A ListenBrainz name made safe for a path segment.
///
/// Names come from MusicBrainz and are mostly plain, but nothing promises it,
/// and a name with a slash or a question mark would otherwise read a different
/// address.
fn encode_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn header_number(headers: &ureq::http::HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// How long until the rate limit resets, from `X-RateLimit-Reset-In`.
fn reset_in(headers: &ureq::http::HeaderMap) -> Option<Duration> {
    header_number(headers, "x-ratelimit-reset-in").map(|seconds| Duration::from_secs(seconds.max(1)))
}

fn remaining(headers: &ureq::http::HeaderMap) -> Option<u64> {
    header_number(headers, "x-ratelimit-remaining")
}

/// Reads one page of the listens endpoint.
pub fn parse_page(body: &str) -> Result<Vec<Listen>, Failure> {
    #[derive(Deserialize)]
    struct Answer {
        payload: Payload,
    }
    #[derive(Deserialize)]
    struct Payload {
        listens: Vec<serde_json::Value>,
    }

    let answer: Answer = serde_json::from_str(body).map_err(|_| Failure::Malformed)?;
    answer.payload.listens.iter().map(parse_listen).collect()
}

/// One listen from its JSON.
///
/// Strict about what ListenBrainz itself writes -- the time and the msid -- and
/// lenient about what the submitting player wrote: `additional_info` is
/// whatever a scrobbler chose to send, and a malformed length or id there
/// costs that field, not the listen.
fn parse_listen(value: &serde_json::Value) -> Result<Listen, Failure> {
    let listened_at = value.get("listened_at").and_then(serde_json::Value::as_i64).ok_or(Failure::Malformed)?;
    let listened_at = OffsetDateTime::from_unix_timestamp(listened_at).map_err(|_| Failure::Malformed)?;
    let recording_msid = value
        .get("recording_msid")
        .and_then(serde_json::Value::as_str)
        .and_then(|msid| Uuid::parse_str(msid).ok())
        .ok_or(Failure::Malformed)?;

    let metadata = value.get("track_metadata");
    let mapping = metadata.and_then(|metadata| metadata.get("mbid_mapping"));
    let info = metadata.and_then(|metadata| metadata.get("additional_info"));

    Ok(Listen {
        listened_at,
        recording_msid,
        artists: artists_of(mapping, info),
        seconds: info.and_then(seconds_of),
    })
}

/// Who a listen was by.
///
/// ListenBrainz's own mapping first: it is matched against MusicBrainz by
/// ListenBrainz, where `additional_info` is whatever the player believed. The
/// player's ids when the mapping has none -- measured on 2026-10-10, a mapping
/// can name the recording and still carry no artists, while the player that
/// submitted it knew exactly who it was.
fn artists_of(mapping: Option<&serde_json::Value>, info: Option<&serde_json::Value>) -> Vec<Uuid> {
    let ids = |source: Option<&serde_json::Value>| -> Vec<Uuid> {
        let mut ids = Vec::new();
        let listed = source
            .and_then(|source| source.get("artist_mbids"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for id in listed {
            if let Some(id) = id.as_str().and_then(|id| Uuid::parse_str(id).ok())
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        ids
    };

    let mapped = ids(mapping);
    if mapped.is_empty() { ids(info) } else { mapped }
}

/// The recording's length in whole seconds, from `duration_ms` or `duration`.
///
/// Players send either, as an integer or now and then as a float or a string.
/// A length that is not a positive number is no length.
fn seconds_of(info: &serde_json::Value) -> Option<i32> {
    let number = |value: &serde_json::Value| -> Option<f64> {
        match value {
            serde_json::Value::Number(number) => number.as_f64(),
            serde_json::Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    };

    let seconds = info
        .get("duration_ms")
        .and_then(number)
        .map(|ms| ms / 1000.0)
        .or_else(|| info.get("duration").and_then(number))?;

    if !seconds.is_finite() || seconds < 1.0 {
        return None;
    }
    // A day: past it, the number is not a length of anything played.
    let seconds = seconds.round().min(86_400.0);
    #[allow(clippy::cast_possible_truncation, reason = "bounded to 1..=86400 just above")]
    Some(seconds as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listen shaped like the ones ListenBrainz returned on 2026-10-10, with
    /// made-up ids.
    fn listen(at: i64, mapped: &[&str], submitted: &[&str], duration_ms: Option<serde_json::Value>) -> serde_json::Value {
        let mut info = serde_json::json!({ "artist_mbids": submitted, "submission_client": "fixture" });
        if let Some(duration) = duration_ms {
            info["duration_ms"] = duration;
        }
        serde_json::json!({
            "inserted_at": at + 30,
            "listened_at": at,
            "recording_msid": "fd0cad33-ea33-453b-8155-1292379277db",
            "user_name": "fixture",
            "track_metadata": {
                "artist_name": "Fixture",
                "track_name": "Fixture",
                "additional_info": info,
                "mbid_mapping": { "artist_mbids": mapped, "recording_mbid": "30d08f4c-d825-4ae1-b79c-44242cddd7c0" }
            }
        })
    }

    const A: &str = "1c70a3fc-fa3c-4be1-8b55-c3192db8a884";
    const B: &str = "78b8347b-a1e2-4e36-a917-92d632a62c18";

    fn page(listens: &[serde_json::Value]) -> String {
        serde_json::json!({ "payload": { "count": listens.len(), "user_id": "fixture", "listens": listens } }).to_string()
    }

    #[test]
    fn the_mapping_names_the_artists_when_it_can() {
        let parsed = parse_page(&page(&[listen(1_700_000_000, &[A, B], &[B], Some(serde_json::json!(402_390)))])).unwrap();
        assert_eq!(parsed[0].artists, vec![Uuid::parse_str(A).unwrap(), Uuid::parse_str(B).unwrap()]);
        assert_eq!(parsed[0].seconds, Some(402));
    }

    #[test]
    fn a_mapping_without_artists_falls_back_to_what_the_player_said() {
        // The shape measured on 2026-10-10: mbid_mapping names the recording
        // and no artists, additional_info carries the player's artist ids.
        let parsed = parse_page(&page(&[listen(1_700_000_000, &[], &[B], None)])).unwrap();
        assert_eq!(parsed[0].artists, vec![Uuid::parse_str(B).unwrap()]);
        assert_eq!(parsed[0].seconds, None);
    }

    #[test]
    fn nobody_named_anywhere_is_an_empty_list_and_not_a_failure() {
        let parsed = parse_page(&page(&[listen(1_700_000_000, &[], &[], None)])).unwrap();
        assert_eq!(parsed[0].artists, Vec::<Uuid>::new());
    }

    #[test]
    fn a_garbled_id_from_a_player_costs_the_id_and_not_the_listen() {
        let parsed = parse_page(&page(&[listen(1_700_000_000, &[], &["not-an-id", B, B], None)])).unwrap();
        assert_eq!(parsed[0].artists, vec![Uuid::parse_str(B).unwrap()]);
    }

    #[test]
    fn lengths_arrive_in_several_shapes() {
        let at = 1_700_000_000;
        for (sent, expected) in [
            (serde_json::json!(200_400), Some(200)),
            (serde_json::json!(200_600.0), Some(201)),
            (serde_json::json!("185000"), Some(185)),
            (serde_json::json!(0), None),
            (serde_json::json!(-5), None),
            (serde_json::json!("soon"), None),
        ] {
            let parsed = parse_page(&page(&[listen(at, &[A], &[], Some(sent.clone()))])).unwrap();
            assert_eq!(parsed[0].seconds, expected, "{sent}");
        }

        // `duration` is seconds, for the players that send that instead.
        let mut by_seconds = listen(at, &[A], &[], None);
        by_seconds["track_metadata"]["additional_info"]["duration"] = serde_json::json!(241);
        assert_eq!(parse_page(&page(&[by_seconds])).unwrap()[0].seconds, Some(241));
    }

    #[test]
    fn a_listen_without_its_own_identity_fails_the_page() {
        // The time and the msid are written by ListenBrainz itself. Missing
        // them means the format changed, and paying for the half of a page
        // that still parses would hide that.
        let mut broken = listen(1_700_000_000, &[A], &[], None);
        broken.as_object_mut().unwrap().remove("recording_msid");
        assert_eq!(parse_page(&page(&[broken])), Err(Failure::Malformed));
        assert_eq!(parse_page("<html>maintenance</html>"), Err(Failure::Malformed));
    }

    #[test]
    fn a_name_is_one_path_segment_whatever_it_holds() {
        assert_eq!(encode_path("rob"), "rob");
        assert_eq!(encode_path("a/b?c"), "a%2Fb%3Fc");
        assert_eq!(encode_path("ñ"), "%C3%B1");
    }

    #[test]
    fn every_failure_has_the_code_the_column_allows() {
        let migration = include_str!("../../migrations/0013_scrobbling.sql");
        for failure in [Failure::Unreachable, Failure::UnknownUser, Failure::Throttled, Failure::Malformed] {
            assert!(migration.contains(&format!("'{}'", failure.code())), "{failure:?}");
        }
    }
}
