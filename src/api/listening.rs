//! Listening to the sky: the radio of a nebula, and the signal of the day.
//!
//! Both play the artists' own `YouTube` channels, through the uploads playlist
//! ADR 0011 settled on -- the canon links an artist to a channel, not a
//! recording to a service, so what plays is a star, not a track.
//!
//! **Only a small part of the sky can play.** A star is playable when it is
//! placed and the canon holds its channel in the embeddable `/channel/UC…`
//! form: about ten thousand of them, on the 100,000-star slice and on the full
//! canon alike, because the channels are the scarce part. So instead of asking
//! Postgres which of a nebula's thousands of members have one -- measured at
//! 0.4 s warm and 3 s cold for Electronic on the slice -- the playable stars
//! are read once per layout into a [`Tuning`], and a radio or a signal is a
//! filter over it.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use tokio::sync::Mutex;

use crate::api::artists::channel_of;
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/radio", get(radio)).route("/api/signal", get(signal))
}

/// A star that can be played: where it is, and what to play.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Station {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// The uploads playlist of the artist's channel.
    pub uploads: String,
}

/// A playable star, with what a radio and a signal need to choose it.
#[derive(Clone, Debug)]
struct Tuned {
    station: Station,
    /// As the sky draws it: the square root of prominence over the brightest.
    brightness: f32,
    /// Outside the stars the wide sky draws -- see [`dark_after`].
    dark: bool,
    /// The artist's main genre and main style, from `main_genres`.
    genre: Option<String>,
    style: Option<String>,
}

/// Every playable star of one layout.
pub struct Tuning {
    layout: LayoutKey,
    stars: Vec<Tuned>,
}

/// Which layout a tuning was read from. The created-at time rides along with
/// the id because a stand reseeded from a dump can bring the same id back
/// with different stars behind it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct LayoutKey {
    id: i16,
    created: i64,
}

/// The tuning, held between requests and rebuilt when the layout changes.
///
/// A mutex rather than a read-write lock: the only slow path is a rebuild, and
/// two requests that both find the tuning stale should not both read ten
/// thousand rows -- the second waits and takes the first one's work.
#[derive(Clone, Default)]
pub struct Dial(Arc<Mutex<Option<Arc<Tuning>>>>);

impl Dial {
    /// Reads the tuning ahead of the first request that needs it. A failure
    /// is only logged: the first radio will try again and say so itself.
    pub async fn warm(self, pool: PgPool) {
        match self.tuning(&pool).await {
            Ok(Some(tuning)) => tracing::info!(stations = tuning.stars.len(), "playable stars read"),
            Ok(None) => tracing::info!("no layout yet, so nothing on the sky can play"),
            Err(error) => tracing::warn!(%error, "the playable stars could not be read ahead of time"),
        }
    }

    async fn tuning(&self, pool: &PgPool) -> sqlx::Result<Option<Arc<Tuning>>> {
        let mut connection = pool.acquire().await?;
        let Some(layout) = latest_layout(&mut connection).await? else {
            return Ok(None);
        };
        let mut held = self.0.lock().await;
        if let Some(tuning) = held.as_ref().filter(|tuning| tuning.layout == layout) {
            return Ok(Some(Arc::clone(tuning)));
        }
        let tuning = Arc::new(tune(&mut connection, layout, dark_after()).await?);
        *held = Some(Arc::clone(&tuning));
        Ok(Some(tuning))
    }
}

/// How many of the brightest stars the wide sky draws: the stars of pyramid
/// levels 0 and 1. A signal comes from beyond them, out of the dark -- a star
/// that only shows once the view has closed in.
///
/// Taken from the tile plan rather than written again here, so the dark is
/// the dark the map actually draws.
fn dark_after() -> usize {
    crate::layout::tiles::Plan::default().stars_at(1)
}

async fn latest_layout(connection: &mut PgConnection) -> sqlx::Result<Option<LayoutKey>> {
    let row = sqlx::query_as::<_, (i16, i64)>(
        "SELECT id, (extract(epoch FROM created_at) * 1000000)::bigint
         FROM sky_layout ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(connection)
    .await?;
    Ok(row.map(|(id, created)| LayoutKey { id, created }))
}

/// Reads the playable stars of a layout.
///
/// `drawn` is how many of the brightest stars count as lit; everything after
/// them in the tile order -- brightness down, then id up, the order the tiles
/// are cut in -- is dark.
async fn tune(connection: &mut PgConnection, layout: LayoutKey, drawn: usize) -> sqlx::Result<Tuning> {
    // The brightest star of all, to normalise against the way the tiles do.
    let brightest: f32 = sqlx::query_scalar(
        "SELECT COALESCE(max(pr.weight), 0)::real
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE p.layout_id = $1",
    )
    .bind(layout.id)
    .fetch_one(&mut *connection)
    .await?;

    // The last lit star in the tile order. Everything after it is dark; with
    // no such star the whole sky fits in the wide levels and nothing is.
    let offset = i64::try_from(drawn.saturating_sub(1)).unwrap_or(i64::MAX);
    let edge: Option<(f32, i32)> = sqlx::query_as(
        "SELECT COALESCE(pr.weight, 0)::real, p.artist_id
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE p.layout_id = $1
         ORDER BY 1 DESC, 2
         OFFSET $2 LIMIT 1",
    )
    .bind(layout.id)
    .bind(offset)
    .fetch_optional(&mut *connection)
    .await?;

    // Every placed star with a channel link, and its main genre and style.
    // Driven from the channel links, which are the few, rather than from the
    // stars, which are the many -- and "main" is asked of those few only.
    let rows = sqlx::query_as::<_, (i32, String, f32, f32, f32, String, Option<String>, Option<String>)>(
        "WITH channel AS (
             SELECT artist_id, url FROM artist_url
             WHERE kind = 'youtube' AND url LIKE '%/channel/UC%'
         ),
         main AS (
             SELECT m.artist_id, m.is_style, g.name
             FROM main_genres(ARRAY(SELECT DISTINCT artist_id FROM channel)) m
             JOIN genre g ON g.id = m.genre_id
         )
         SELECT a.id, a.name, p.x, p.y, COALESCE(pr.weight, 0)::real, c.url, genre.name, style.name
         FROM channel c
         JOIN artist_position p ON p.artist_id = c.artist_id AND p.layout_id = $1
         JOIN sky_layout l ON l.id = p.layout_id
         JOIN artist a ON a.id = c.artist_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = c.artist_id AND pr.metric_id = l.metric_id
         LEFT JOIN main genre ON genre.artist_id = c.artist_id AND NOT genre.is_style
         LEFT JOIN main style ON style.artist_id = c.artist_id AND style.is_style
         ORDER BY a.id, c.url",
    )
    .bind(layout.id)
    .fetch_all(&mut *connection)
    .await?;

    // One station per artist, on the channel its card plays: the rows come in
    // artist order, so each artist's links arrive together.
    let stars = rows
        .chunk_by(|a, b| a.0 == b.0)
        .filter_map(|links| {
            let (id, name, x, y, weight, _, genre, style) = links[0].clone();
            let uploads = channel_of(links.iter().map(|link| link.5.as_str()))?;
            // After the edge in the tile order: dimmer, or as bright and
            // later by id.
            let dark = edge.is_some_and(|(edge_weight, edge_id)| edge_weight.total_cmp(&weight).then(id.cmp(&edge_id)).is_gt());
            Some(Tuned {
                station: Station { id, name, x, y, uploads },
                brightness: if brightest > 0.0 { (weight / brightest).sqrt() } else { 0.0 },
                dark,
                genre,
                style,
            })
        })
        .collect();
    Ok(Tuning { layout, stars })
}

/// A nebula a radio can play: a genre or a style, by the name the sky writes.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Nebula {
    pub name: String,
    pub kind: Kind,
}

/// A radio needs at least this many stations to be offered. Below it, a
/// style's radio would repeat two or three channels in a loop; its genre is
/// offered instead, which is the wider sound the style sits inside.
const RADIO_MIN: usize = 8;

/// Which radio to offer for a star or a patch of sky whose main style and
/// genre are these: the style, which is the nearer name, when its radio has
/// enough to play -- otherwise the genre, when it has anything at all.
fn playable(stars: &[Tuned], genre: Option<&str>, style: Option<&str>) -> Option<Nebula> {
    let size = |kind: Kind, name: &str| {
        stars
            .iter()
            .filter(|star| match kind {
                Kind::Genre => star.genre.as_deref() == Some(name),
                Kind::Style => star.style.as_deref() == Some(name),
            })
            .count()
    };
    let nebula = |kind: Kind, name: &str| Nebula { name: name.to_string(), kind };
    if let Some(style) = style.filter(|style| size(Kind::Style, style) >= RADIO_MIN) {
        return Some(nebula(Kind::Style, style));
    }
    genre.filter(|genre| size(Kind::Genre, genre) > 0).map(|genre| nebula(Kind::Genre, genre))
}

/// The radio a card offers: its star's main style or genre, by [`playable`].
///
/// Never fails the card. The card is about the star, and a radio that could
/// not be worked out is a missing button, not a missing star -- so a failure
/// is logged and answered with none.
pub(crate) async fn radio_of(pool: &PgPool, dial: &Dial, id: i32) -> Option<Nebula> {
    let mains = sqlx::query_as::<_, (String, bool)>("SELECT g.name, m.is_style FROM main_genres(ARRAY[$1]) m JOIN genre g ON g.id = m.genre_id")
        .bind(id)
        .fetch_all(pool)
        .await;
    let tuning = dial.tuning(pool).await;
    match (mains, tuning) {
        (Ok(mains), Ok(Some(tuning))) => {
            let main = |style: bool| mains.iter().find(|row| row.1 == style).map(|row| row.0.as_str());
            playable(&tuning.stars, main(false), main(true))
        }
        (Ok(_), Ok(None)) => None,
        (Err(error), _) | (_, Err(error)) => {
            tracing::warn!(%error, artist = id, "the radio of a star could not be worked out");
            None
        }
    }
}

/// The radio for a patch of sky whose commonest main genre and style are
/// these, by [`playable`]. Like [`radio_of`], a failure is a missing offer.
pub(crate) async fn radio_here(pool: &PgPool, dial: &Dial, genre: Option<&str>, style: Option<&str>) -> Option<Nebula> {
    match dial.tuning(pool).await {
        Ok(tuning) => tuning.and_then(|tuning| playable(&tuning.stars, genre, style)),
        Err(error) => {
            tracing::warn!(%error, "the radio of a region could not be worked out");
            None
        }
    }
}

/// Whether a nebula is a genre or a style: the two layers of names on the sky.
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Genre,
    Style,
}

#[derive(Deserialize)]
struct RadioQuery {
    /// The nebula, by the name the sky writes on it.
    name: String,
    kind: Kind,
    /// Makes the queue repeatable: the same seed, the same order. The client
    /// draws a new one for each radio it starts.
    seed: u64,
}

#[derive(Serialize)]
struct Radio {
    name: String,
    kind: Kind,
    /// In the order to play them. Empty when the nebula has no channels.
    stations: Vec<Station>,
}

/// How long one queue is. Fifty stations is a few hours of one video each;
/// a listener who gets to the end is sent a fresh queue on a new seed.
pub const RADIO_LENGTH: usize = 50;

/// The radio of a nebula: its stars with a channel, in a shuffled order that
/// favours the bright ones.
async fn radio(State(state): State<AppState>, Query(query): Query<RadioQuery>) -> Response {
    let known = sqlx::query_scalar::<_, i32>("SELECT id FROM genre WHERE name = $1 AND is_style = $2")
        .bind(&query.name)
        .bind(query.kind == Kind::Style)
        .fetch_optional(&state.pool)
        .await;
    match known {
        Ok(Some(_)) => {}
        // A name the canon does not have is a wrong address, not a quiet
        // nebula: an empty queue would read as "nobody here has a channel".
        Ok(None) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "no such genre or style" }))).into_response(),
        Err(error) => return unreadable(&error),
    }

    match state.dial.tuning(&state.pool).await {
        Ok(tuning) => {
            let stations = tuning
                .map(|tuning| queue(&tuning.stars, &query.name, query.kind, query.seed))
                .unwrap_or_default();
            (
                StatusCode::OK,
                Json(Radio {
                    name: query.name,
                    kind: query.kind,
                    stations,
                }),
            )
                .into_response()
        }
        Err(error) => unreadable(&error),
    }
}

/// The members of a nebula, in the order a radio plays them.
///
/// A weighted shuffle (Efraimidis and Spirakis): each star draws a uniform
/// `u` and is ranked by `ln(u) / w`, which puts a star of weight `w` ahead of
/// others with exactly the odds a weighted draw would. The weight is the
/// star's brightness with a floor under it -- the radio favours the stars the
/// sky draws bright, and the floor keeps the faint ones on the air rather than
/// never reached.
fn queue(stars: &[Tuned], name: &str, kind: Kind, seed: u64) -> Vec<Station> {
    let mut members: Vec<(f64, &Tuned)> = stars
        .iter()
        .filter(|star| {
            let main = match kind {
                Kind::Genre => star.genre.as_deref(),
                Kind::Style => star.style.as_deref(),
            };
            main == Some(name)
        })
        .map(|star| {
            let weight = f64::from(star.brightness.max(RADIO_FLOOR));
            (unit(seed, star.station.id).ln() / weight, star)
        })
        .collect();
    members.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.station.id.cmp(&b.1.station.id)));
    members.into_iter().take(RADIO_LENGTH).map(|(_, star)| star.station.clone()).collect()
}

/// The least weight a star plays with. Brightness runs from 1 at the hub of
/// the graph down to a few hundredths at its edge; below this the chance of
/// being played stops shrinking.
const RADIO_FLOOR: f32 = 0.05;

#[derive(Deserialize)]
struct SignalQuery {
    /// The listener's own calendar day, `YYYY-MM-DD`: the signal turns over at
    /// their midnight, not at the server's.
    day: String,
}

#[derive(Serialize)]
struct Signal {
    day: String,
    /// The day's signal first, then the stars a player falls back to, in
    /// order, when a channel will not play. Empty when nothing on this sky is
    /// both dark and playable.
    stars: Vec<Station>,
}

/// How many stars a signal carries. Measured on the canon, about a third of
/// the channels will not play in an embedded player -- silently, with no
/// error -- so a single star would leave the signal mute one day in three.
/// Five in a row are mute about one day in two hundred.
const SIGNAL_CANDIDATES: usize = 5;

/// The signal of the day: one dark, playable star, the same for everyone on
/// the same day -- with the next few behind it, in the same order for
/// everyone, for when its channel will not play.
async fn signal(State(state): State<AppState>, Query(query): Query<SignalQuery>) -> Response {
    let Some(day) = julian_day(&query.day) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "day must be a date written YYYY-MM-DD" })),
        )
            .into_response();
    };
    match state.dial.tuning(&state.pool).await {
        Ok(tuning) => {
            let stars = tuning.map(|tuning| picks(&tuning.stars, day, SIGNAL_CANDIDATES)).unwrap_or_default();
            (StatusCode::OK, Json(Signal { day: query.day, stars })).into_response()
        }
        Err(error) => unreadable(&error),
    }
}

/// The dark stars of a day, in the day's order, the first `n` of them.
///
/// A shuffle of the dark keyed on the day: every star is ranked by the day
/// mixed with its id, so the same day on the same sky is the same list,
/// whoever asks. The day is mixed rather than used as an index, or
/// neighbouring days would walk the list a step at a time and every signal
/// would sit beside yesterday's.
fn picks(stars: &[Tuned], day: i32, n: usize) -> Vec<Station> {
    let salt = mix(u64::from(day.unsigned_abs()));
    let mut dark: Vec<&Tuned> = stars.iter().filter(|star| star.dark).collect();
    dark.sort_by_key(|star| (mix(salt ^ u64::from(star.station.id.unsigned_abs())), star.station.id));
    dark.into_iter().take(n).map(|star| star.station.clone()).collect()
}

/// `2026-09-25` -> its Julian day number, or `None` for anything that is not
/// a real date in that exact form.
fn julian_day(raw: &str) -> Option<i32> {
    let mut parts = raw.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let month = time::Month::try_from(month.parse::<u8>().ok()?).ok()?;
    let date = time::Date::from_calendar_date(year.parse().ok()?, month, day.parse().ok()?).ok()?;
    Some(date.to_julian_day())
}

/// A uniform draw in (0, 1) for one star under one seed.
///
/// Open at both ends: `ln(0)` is minus infinity, which would send a star to
/// the back of every queue whatever its weight.
fn unit(seed: u64, id: i32) -> f64 {
    let bits = mix(seed ^ u64::from(id.unsigned_abs()).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 11;
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64 mantissa exactly")]
    let fraction = (bits as f64 + 0.5) / (1u64 << 53) as f64;
    fraction
}

/// `SplitMix64`'s finaliser: a fixed, portable scramble of 64 bits. Written
/// out rather than borrowed from `rand`, whose generators are free to change
/// their output between versions -- and a queue that reshuffled itself on a
/// dependency update would not be repeatable.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn unreadable(error: &sqlx::Error) -> Response {
    tracing::error!(%error, "the playable stars could not be read");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "the sky could not be read" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        let pool = PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network");
        routes().with_state(AppState {
            pool,
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
            dial: Dial::default(),
        })
    }

    fn star(id: i32, brightness: f32, style: &str, dark: bool) -> Tuned {
        Tuned {
            station: Station {
                id,
                name: format!("Star {id}"),
                x: 0.0,
                y: 0.0,
                uploads: format!("UU{id:022}"),
            },
            brightness,
            dark,
            genre: Some("Funk / Soul".to_string()),
            style: Some(style.to_string()),
        }
    }

    #[test]
    fn a_radio_plays_only_its_own_nebula() {
        let stars = [star(1, 0.5, "Soul", false), star(2, 0.5, "Disco", false), star(3, 0.5, "Soul", false)];
        let ids: Vec<i32> = queue(&stars, "Soul", Kind::Style, 7).iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&2), "Disco is not Soul: {ids:?}");
        // The same name as a genre is a different nebula.
        assert!(queue(&stars, "Soul", Kind::Genre, 7).is_empty());
        assert_eq!(queue(&stars, "Funk / Soul", Kind::Genre, 7).len(), 3);
    }

    #[test]
    fn the_same_seed_is_the_same_queue_and_another_seed_is_not() {
        let stars: Vec<Tuned> = (1..=40).map(|id| star(id, 0.3, "Soul", false)).collect();
        let order = |seed| queue(&stars, "Soul", Kind::Style, seed).iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(order(11), order(11));
        assert_ne!(order(11), order(12));
    }

    #[test]
    fn a_bright_star_comes_on_the_air_sooner_than_a_faint_one() {
        // One hub and nineteen faint stars; over many radios the hub should
        // lead far more often than one in twenty. Uniform weights would put it
        // first about 5% of the time.
        let mut stars = vec![star(1, 1.0, "Soul", false)];
        stars.extend((2..=20).map(|id| star(id, RADIO_FLOOR, "Soul", false)));
        let first = (0..2000).filter(|seed| queue(&stars, "Soul", Kind::Style, *seed)[0].id == 1).count();
        // 1.0 against nineteen stars of 0.05: the hub leads about half the time.
        assert!(first > 800, "the hub led only {first} of 2000 radios");
        // And the faint ones are still played rather than starved.
        let faint_first = 2000 - first;
        assert!(faint_first > 400, "faint stars led only {faint_first} of 2000 radios");
    }

    #[test]
    fn a_style_too_small_for_a_radio_offers_its_genre() {
        let mut stars: Vec<Tuned> = (1..=3).map(|id| star(id, 0.3, "Northern Soul", false)).collect();
        let offer = |stars: &[Tuned]| playable(stars, Some("Funk / Soul"), Some("Northern Soul"));
        // Three channels is a loop, not a radio: the genre plays instead.
        assert_eq!(offer(&stars).map(|nebula| nebula.kind), Some(Kind::Genre));
        stars.extend((4..=i32::try_from(RADIO_MIN).unwrap()).map(|id| star(id, 0.3, "Northern Soul", false)));
        assert_eq!(
            offer(&stars),
            Some(Nebula {
                name: "Northern Soul".to_string(),
                kind: Kind::Style
            })
        );
        // Nothing playable at all is no offer, not an empty radio.
        assert_eq!(playable(&stars, Some("Jazz"), Some("Bop")), None);
    }

    #[test]
    fn a_queue_stops_at_its_length() {
        let stars: Vec<Tuned> = (1..=120).map(|id| star(id, 0.3, "Soul", false)).collect();
        let played = queue(&stars, "Soul", Kind::Style, 1);
        assert_eq!(played.len(), RADIO_LENGTH);
        let mut ids: Vec<i32> = played.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RADIO_LENGTH, "no station twice in one queue");
    }

    #[test]
    fn the_signal_comes_only_from_the_dark() {
        let stars: Vec<Tuned> = (1..=50).map(|id| star(id, 0.3, "Soul", id % 5 == 0)).collect();
        for day in 2_460_000..2_460_200 {
            let signal = picks(&stars, day, SIGNAL_CANDIDATES);
            assert_eq!(signal.len(), SIGNAL_CANDIDATES);
            assert!(signal.iter().all(|star| star.id % 5 == 0), "day {day} picked a lit star: {signal:?}");
            let mut ids: Vec<i32> = signal.iter().map(|star| star.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), SIGNAL_CANDIDATES, "no star twice in one day's signal");
        }
        let lit: Vec<Tuned> = (1..=5).map(|id| star(id, 0.3, "Soul", false)).collect();
        assert!(picks(&lit, 2_460_000, SIGNAL_CANDIDATES).is_empty(), "a sky with no dark has no signal");
    }

    #[test]
    fn one_day_is_one_signal_and_the_days_move_it() {
        let stars: Vec<Tuned> = (1..=500).map(|id| star(id, 0.3, "Soul", true)).collect();
        let signal = |day| picks(&stars, day, SIGNAL_CANDIDATES)[0].id;
        let today = signal(2_461_309);
        assert_eq!(signal(2_461_309), today);
        // A week of signals: mixed days rather than neighbours of each other.
        let week: Vec<i32> = (2_461_309..2_461_316).map(signal).collect();
        let mut distinct = week.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() >= 6, "a week of signals repeated itself: {week:?}");
        assert!(week.windows(2).any(|pair| (pair[0] - pair[1]).abs() > 1), "signals walk the list: {week:?}");
    }

    #[test]
    fn a_day_is_read_whole_or_not_at_all() {
        assert_eq!(julian_day("2026-09-25"), Some(2_461_309));
        assert_eq!(julian_day("2026-02-30"), None, "not a real date");
        assert_eq!(julian_day("2026-9-25"), None, "not the exact form");
        assert_eq!(julian_day("2026-09-25T00:00"), None);
        assert_eq!(julian_day("2026-09"), None);
        assert_eq!(julian_day(""), None);
    }

    #[test]
    fn a_draw_never_reaches_either_end() {
        for seed in [0, 1, u64::MAX] {
            for id in [0, 1, i32::MAX, i32::MIN] {
                let u = unit(seed, id);
                assert!(u > 0.0 && u < 1.0, "unit({seed}, {id}) = {u}");
            }
        }
    }

    #[tokio::test]
    async fn a_signal_without_a_day_is_a_bad_request_before_the_database() {
        let response = app()
            .oneshot(Request::get("/api/signal?day=yesterday").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_radio_needs_a_kind_it_knows() {
        let response = app()
            .oneshot(Request::get("/api/radio?name=Soul&kind=mood&seed=1").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// Reads a tuning from rows of its own, then rolls them back.
    ///
    /// The SQL is where "main", "placed" and "dark" are decided, so it runs
    /// against Postgres rather than being described to a mock. Without a test
    /// database it says so and does not pass for a reason it did not check.
    #[tokio::test]
    async fn the_tuning_holds_placed_channels_with_their_main_style_and_the_dark() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the tuning query was NOT checked against a database");
            return;
        };
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        let mut tx = pool.begin().await.unwrap();

        // Ids far above anything MusicBrainz issues.
        let (hub, faint, silent, unplaced) = (2_000_000_101, 2_000_000_102, 2_000_000_103, 2_000_000_104);
        for id in [hub, faint, silent, unplaced] {
            sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, gen_random_uuid(), $2, $2)")
                .bind(id)
                .bind(format!("Fixture {id}"))
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        let metric: i16 = sqlx::query_scalar("INSERT INTO similarity_metric (key, description) VALUES ('fixture-metric', 'fixture') RETURNING id")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let layout: i16 =
            sqlx::query_scalar("INSERT INTO sky_layout (key, metric_id, description, seed) VALUES ('fixture-layout', $1, 'fixture', 1) RETURNING id")
                .bind(metric)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        for (id, weight) in [(hub, 9.0_f32), (faint, 1.0), (silent, 5.0)] {
            sqlx::query("INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES ($1, $2, 1, 2)")
                .bind(layout)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO artist_prominence (metric_id, artist_id, degree, weight) VALUES ($1, $2, 1, $3)")
                .bind(metric)
                .bind(id)
                .bind(weight)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        let channel = |id: i32, rest: &str| {
            sqlx::query("INSERT INTO artist_url (artist_id, kind, url) VALUES ($1, 'youtube', $2)")
                .bind(id)
                .bind(format!("https://www.youtube.com/channel/UC{rest}"))
        };
        channel(hub, "aaaaaaaaaaaaaaaaaaaaaa").execute(&mut *tx).await.unwrap();
        // A second link to a channel: one station, not two.
        channel(hub, "bbbbbbbbbbbbbbbbbbbbbb").execute(&mut *tx).await.unwrap();
        channel(faint, "cccccccccccccccccccccc").execute(&mut *tx).await.unwrap();
        // A channel with no place on the sky cannot be flown to.
        channel(unplaced, "dddddddddddddddddddddd").execute(&mut *tx).await.unwrap();
        // `silent` has only a handle, which names a channel without its id.
        sqlx::query("INSERT INTO artist_url (artist_id, kind, url) VALUES ($1, 'youtube', 'https://www.youtube.com/@silent')")
            .bind(silent)
            .execute(&mut *tx)
            .await
            .unwrap();

        let style = |name: &'static str| {
            sqlx::query_scalar::<_, i32>(
                "INSERT INTO genre (name, is_style) VALUES ($1, true)
                 ON CONFLICT (name, is_style) DO UPDATE SET name = EXCLUDED.name RETURNING id",
            )
            .bind(name)
        };
        let soul = style("Fixture Soul").fetch_one(&mut *tx).await.unwrap();
        let disco = style("Fixture Disco").fetch_one(&mut *tx).await.unwrap();
        // The hub is mostly soul with a disco sideline; the faint star is disco.
        for (artist, genre, releases) in [(hub, soul, 30), (hub, disco, 4), (faint, disco, 7)] {
            sqlx::query("INSERT INTO artist_genre (artist_id, genre_id, releases) VALUES ($1, $2, $3)")
                .bind(artist)
                .bind(genre)
                .bind(releases)
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        let key = latest_layout(&mut tx).await.unwrap().expect("the fixture layout is the newest");
        assert_eq!(key.id, layout);
        // Two lit stars: the hub and `silent`; the faint one is past them.
        let tuning = tune(&mut tx, key, 2).await.expect("the query runs and decodes");
        let ids: Vec<i32> = tuning.stars.iter().map(|star| star.station.id).collect();
        assert_eq!(ids, vec![hub, faint], "placed, with an embeddable channel, once each");

        let hub_star = &tuning.stars[0];
        assert_eq!(hub_star.station.uploads, "UUaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(hub_star.style.as_deref(), Some("Fixture Soul"), "the main style, not the sideline");
        assert!((hub_star.brightness - 1.0).abs() < 1e-6, "the brightest star is 1");
        assert!(!hub_star.dark);
        assert!(tuning.stars[1].dark, "the third brightest is past the two lit ones");
        assert_eq!(
            queue(&tuning.stars, "Fixture Disco", Kind::Style, 3).iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![faint]
        );

        tx.rollback().await.unwrap();
    }
}
