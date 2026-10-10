//! Dossiers: the pages a station opens into.
//!
//! Two kinds of station hold stars together on this map (banks 9 and 32 of
//! the mechanics): a **label**, whose roster is who released on it, and a
//! **scene**, whose roster is who comes from a place. Each dossier answers the
//! same three questions -- who is here (the roster), when (the chronology),
//! and where on the sky (the map: every placed member, so the reader can see
//! whether the cluster is one) -- and then points onwards: a label to the
//! places its roster comes from, a scene to the labels its people released on,
//! both to their sound.
//!
//! Only the stars of the current layout count. A roster member the map
//! cannot fly to would be a name leading nowhere, and the dossier is a page of
//! the sky, not of the canon.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::Serialize;
use sqlx::PgPool;

use crate::api::listening::{Dial, Sky};
use crate::app::AppState;
use crate::markup::{self, Kind, Piece};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/labels/{id}", get(label)).route("/api/scenes/{qid}", get(scene))
}

/// How many roster members are listed by name. The map shows them all; a list
/// of three thousand names is not read by anyone.
const LISTED: usize = 60;

/// How many members a dossier reads at all. Far above any label on the
/// 100,000-star slice -- the largest roster there is a few thousand -- and a
/// ceiling for the full canon, where a major's roster is a sky of its own.
const MEMBERS: i64 = 5000;

/// How many names a pointer list -- sound, places, labels, imprints -- gives.
const POINTERS: i64 = 8;

/// A label as a page.
#[derive(Serialize)]
struct LabelDossier {
    id: i32,
    name: String,
    /// The label's page on Discogs, where the dossier's facts come from.
    discogs_url: String,
    /// The description, as paragraphs of pieces a page can render and link.
    profile: Vec<Vec<Segment>>,
    parent: Option<LabelRef>,
    sublabels: Vec<LabelRef>,
    /// The label's output year by year, over all its official releases.
    chronology: Vec<YearCount>,
    roster: Roster<LabelMember>,
    sound: Vec<Sound>,
    /// Where the roster comes from: the scenes this label draws on.
    scenes: Vec<SceneRef>,
}

/// A place as a page.
#[derive(Serialize)]
struct SceneDossier {
    qid: i32,
    name: String,
    wikidata_url: String,
    /// How many of the roster formed here, and how many were born here: two
    /// different claims, counted apart as the card keeps them apart.
    formed: usize,
    born: usize,
    /// When the scene's acts began, year by year.
    chronology: Vec<YearCount>,
    roster: Roster<SceneMember>,
    sound: Vec<Sound>,
    /// The labels the scene's people released on.
    labels: Vec<LabelRef>,
}

/// Who is on a station: the first names listed, and every member's place.
#[derive(Serialize)]
struct Roster<T> {
    /// Every placed member, however many are listed.
    size: usize,
    listed: Vec<T>,
    /// `[id, x, y, brightness]` of every placed member, for the map.
    map: Vec<(i32, f32, f32, f32)>,
}

#[derive(Serialize)]
struct LabelMember {
    id: i32,
    name: String,
    /// How many of the artist's releases carry this label.
    releases: i32,
    first_year: Option<i16>,
    last_year: Option<i16>,
}

#[derive(Serialize)]
struct SceneMember {
    id: i32,
    name: String,
    /// Born here rather than formed here.
    born: bool,
    begin_year: Option<i16>,
    end_year: Option<i16>,
}

#[derive(Serialize, Debug, PartialEq)]
struct YearCount {
    year: i16,
    count: i32,
}

#[derive(Serialize)]
pub(super) struct LabelRef {
    pub id: i32,
    pub name: String,
    /// How many placed artists the reference stands for: a sublabel's roster,
    /// or how many of a scene's people released on this label.
    pub artists: i32,
}

#[derive(Serialize)]
pub(super) struct SceneRef {
    pub qid: i32,
    pub name: String,
    pub artists: i32,
}

/// A style (or, where a roster has none, a genre) and how many members have
/// it as their main one.
#[derive(Serialize)]
struct Sound {
    name: String,
    is_style: bool,
    artists: i32,
}

/// One run of a description: words, and at most one thing they lead to.
#[derive(Serialize, Debug, PartialEq, Eq)]
struct Segment {
    text: String,
    /// A star on this sky the words name.
    #[serde(skip_serializing_if = "Option::is_none")]
    star: Option<i32>,
    /// A label with a dossier of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<i32>,
    /// A web address, only ever `http` or `https`.
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

impl Segment {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            star: None,
            label: None,
            url: None,
        }
    }
}

async fn label(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    answer(load_label(&state.pool, &state.dial, id).await, "label", id)
}

async fn scene(State(state): State<AppState>, Path(qid): Path<i32>) -> Response {
    answer(load_scene(&state.pool, &state.dial, qid).await, "scene", qid)
}

fn answer<T: Serialize>(result: sqlx::Result<Option<T>>, what: &'static str, id: i32) -> Response {
    match result {
        Ok(Some(dossier)) => (StatusCode::OK, Json(dossier)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": format!("no such {what}") }))).into_response(),
        Err(error) => {
            tracing::error!(%error, what, id, "failed to read a dossier");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be read" })),
            )
                .into_response()
        }
    }
}

/// As the sky draws it: the square root of prominence over the brightest.
fn brightness(weight: f32, brightest: f32) -> f32 {
    if brightest > 0.0 { (weight / brightest).sqrt() } else { 0.0 }
}

async fn load_label(pool: &PgPool, dial: &Dial, id: i32) -> sqlx::Result<Option<LabelDossier>> {
    let Some((name, profile, parent_id, parent_name)) = sqlx::query_as::<_, (String, Option<String>, Option<i32>, Option<String>)>(
        "SELECT l.name, l.profile, l.parent_label_id, parent.name
         FROM label l LEFT JOIN label parent ON parent.id = l.parent_label_id
         WHERE l.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let chronology = sqlx::query_as::<_, (i16, i32)>("SELECT year, releases FROM label_year WHERE label_id = $1 ORDER BY year")
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(year, count)| YearCount { year, count })
        .collect();

    let sublabels = sqlx::query_as::<_, (i32, String, i32)>(
        "SELECT c.id, c.name, (SELECT count(*) FROM label_artist la WHERE la.label_id = c.id)::int AS artists
         FROM label c WHERE c.parent_label_id = $1
         ORDER BY artists DESC, c.name
         LIMIT $2",
    )
    .bind(id)
    .bind(POINTERS * 2)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, artists)| LabelRef {
        id,
        name: plain_name(&name).to_string(),
        artists,
    })
    .collect();

    let (roster, members) = label_roster(pool, dial, id).await?;

    let sound = sound(pool, &members).await?;
    let scenes = scenes_of(pool, &members).await?;
    let profile = match profile {
        Some(text) => render(pool, &text).await?,
        None => Vec::new(),
    };

    Ok(Some(LabelDossier {
        id,
        name: plain_name(&name).to_string(),
        discogs_url: format!("https://www.discogs.com/label/{id}"),
        profile,
        parent: parent_id.zip(parent_name).map(|(id, name)| LabelRef {
            id,
            name: plain_name(&name).to_string(),
            artists: 0,
        }),
        sublabels,
        chronology,
        roster,
        sound,
        scenes,
    }))
}

/// A label's roster: the members listed by name, every member's place, and
/// the ids of all of them for the pointers that follow.
///
/// Members are placed on the current layout and dimmed against its brightest
/// star, the way the tiles draw them.
async fn label_roster(pool: &PgPool, dial: &Dial, id: i32) -> sqlx::Result<(Roster<LabelMember>, Vec<i32>)> {
    let Some(Sky {
        layout: layout_id,
        metric: metric_id,
        brightest,
    }) = dial.sky(pool).await?
    else {
        return Ok((
            Roster {
                size: 0,
                listed: Vec::new(),
                map: Vec::new(),
            },
            Vec::new(),
        ));
    };
    let rows = sqlx::query_as::<_, (i32, String, i32, Option<i16>, Option<i16>, f32, f32, f32)>(
        "SELECT a.id, a.name, la.releases, la.first_year, la.last_year, p.x, p.y, COALESCE(pr.weight, 0)::real AS weight
         FROM label_artist la
         JOIN artist a ON a.id = la.artist_id
         JOIN artist_position p ON p.artist_id = la.artist_id AND p.layout_id = $2
         LEFT JOIN artist_prominence pr ON pr.artist_id = la.artist_id AND pr.metric_id = $3
         WHERE la.label_id = $1
         ORDER BY la.releases DESC, weight DESC, a.id
         LIMIT $4",
    )
    .bind(id)
    .bind(layout_id)
    .bind(metric_id)
    .bind(MEMBERS)
    .fetch_all(pool)
    .await?;
    let members: Vec<i32> = rows.iter().map(|row| row.0).collect();
    let map = rows.iter().map(|r| (r.0, r.5, r.6, brightness(r.7, brightest))).collect();
    let listed = rows
        .into_iter()
        .take(LISTED)
        .map(|(id, name, releases, first_year, last_year, ..)| LabelMember {
            id,
            name,
            releases,
            first_year,
            last_year,
        })
        .collect();
    Ok((
        Roster {
            size: members.len(),
            listed,
            map,
        },
        members,
    ))
}

async fn load_scene(pool: &PgPool, dial: &Dial, qid: i32) -> sqlx::Result<Option<SceneDossier>> {
    let Some(name) = sqlx::query_scalar::<_, Option<String>>("SELECT label FROM wikidata_item WHERE qid = $1")
        .bind(qid)
        .fetch_optional(pool)
        .await?
        .flatten()
    else {
        return Ok(None);
    };
    let Some(Sky {
        layout: layout_id,
        metric: metric_id,
        brightest,
    }) = dial.sky(pool).await?
    else {
        return Ok(None);
    };

    // Brightest first: a place has no weight of its own to rank people by,
    // and the sky's own measure puts the names a reader knows at the top.
    let rows = sqlx::query_as::<_, (i32, String, bool, Option<i16>, Option<i16>, f32, f32, f32)>(
        "SELECT a.id, a.name, COALESCE(f.origin_is_birth, false), COALESCE(a.begin_year, f.inception_year), a.end_year,
                p.x, p.y, COALESCE(pr.weight, 0)::real AS weight
         FROM artist_fact f
         JOIN artist a ON a.id = f.artist_id
         JOIN artist_position p ON p.artist_id = f.artist_id AND p.layout_id = $2
         LEFT JOIN artist_prominence pr ON pr.artist_id = f.artist_id AND pr.metric_id = $3
         WHERE f.origin_qid = $1
         ORDER BY weight DESC, a.id
         LIMIT $4",
    )
    .bind(qid)
    .bind(layout_id)
    .bind(metric_id)
    .bind(MEMBERS)
    .fetch_all(pool)
    .await?;

    // A place nobody on this sky comes from is not a scene.
    if rows.is_empty() {
        return Ok(None);
    }

    let members: Vec<i32> = rows.iter().map(|row| row.0).collect();
    let born = rows.iter().filter(|row| row.2).count();
    let chronology = arrivals(rows.iter().filter_map(|row| row.3));
    let map = rows.iter().map(|r| (r.0, r.5, r.6, brightness(r.7, brightest))).collect();
    let size = rows.len();
    let listed = rows
        .into_iter()
        .take(LISTED)
        .map(|(id, name, born, begin_year, end_year, ..)| SceneMember {
            id,
            name,
            born,
            begin_year,
            end_year,
        })
        .collect();

    let sound = sound(pool, &members).await?;
    let labels = labels_of(pool, &members).await?;

    Ok(Some(SceneDossier {
        qid,
        name,
        wikidata_url: format!("https://www.wikidata.org/wiki/Q{qid}"),
        formed: size - born,
        born,
        chronology,
        roster: Roster { size, listed, map },
        sound,
        labels,
    }))
}

/// How many members began in each year, oldest first.
fn arrivals(years: impl Iterator<Item = i16>) -> Vec<YearCount> {
    let mut counts: HashMap<i16, i32> = HashMap::new();
    for year in years {
        *counts.entry(year).or_insert(0) += 1;
    }
    let mut out: Vec<YearCount> = counts.into_iter().map(|(year, count)| YearCount { year, count }).collect();
    out.sort_unstable_by_key(|entry| entry.year);
    out
}

/// The main styles of a roster -- `main_genres`, the one definition the sky's
/// names and the radio also read -- or its main genres when it has no styles.
async fn sound(pool: &PgPool, members: &[i32]) -> sqlx::Result<Vec<Sound>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (String, bool, i32)>(
        "SELECT g.name, g.is_style, count(*)::int
         FROM main_genres($1) m JOIN genre g ON g.id = m.genre_id
         GROUP BY g.name, g.is_style
         ORDER BY g.is_style DESC, 3 DESC, g.name",
    )
    .bind(members)
    .fetch_all(pool)
    .await?;
    let styles = rows.iter().any(|row| row.1);
    Ok(rows
        .into_iter()
        .filter(|row| row.1 == styles)
        .take(usize::try_from(POINTERS).unwrap_or(8))
        .map(|(name, is_style, artists)| Sound { name, is_style, artists })
        .collect())
}

/// The places a set of stars comes from, most shared first.
pub(super) async fn scenes_of(pool: &PgPool, members: &[i32]) -> sqlx::Result<Vec<SceneRef>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, (i32, String, i32)>(
        "SELECT i.qid, i.label, count(*)::int
         FROM artist_fact f JOIN wikidata_item i ON i.qid = f.origin_qid
         WHERE f.artist_id = ANY($1) AND i.label IS NOT NULL
         GROUP BY i.qid, i.label
         ORDER BY 3 DESC, i.label
         LIMIT $2",
    )
    .bind(members)
    .bind(POINTERS)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(qid, name, artists)| SceneRef { qid, name, artists })
    .collect())
}

/// The labels a set of stars released on, by how many of them did.
async fn labels_of(pool: &PgPool, members: &[i32]) -> sqlx::Result<Vec<LabelRef>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, (i32, String, i32)>(
        "SELECT l.id, l.name, count(*)::int
         FROM label_artist la JOIN label l ON l.id = la.label_id
         WHERE la.artist_id = ANY($1)
         GROUP BY l.id, l.name
         ORDER BY 3 DESC, sum(la.releases) DESC, l.id
         LIMIT $2",
    )
    .bind(members)
    .bind(POINTERS)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, artists)| LabelRef {
        id,
        name: plain_name(&name).to_string(),
        artists,
    })
    .collect())
}

/// A Discogs name without the number Discogs adds to tell namesakes apart:
/// "Antidote (4)" is the fourth label called Antidote, and it is called
/// Antidote. The number is an address, and the address is the id.
pub(super) fn plain_name(name: &str) -> &str {
    let Some(open) = name.strip_suffix(')').and_then(|rest| rest.rfind(" (")) else {
        return name;
    };
    let digits = &name[open + 2..name.len() - 1];
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        &name[..open]
    } else {
        name
    }
}

/// Turns a stored description into paragraphs of segments, resolving what the
/// canon can follow: an artist named by Discogs id becomes a star when the
/// canon links that id to one on this sky, and a label becomes a link when it
/// has a dossier here.
async fn render(pool: &PgPool, profile: &str) -> sqlx::Result<Vec<Vec<Segment>>> {
    let (artist_ids, label_ids) = followable(profile);

    let stars: HashMap<i32, i32> = if artist_ids.is_empty() {
        HashMap::new()
    } else {
        // One Discogs entry can be several canonical artists; the brightest
        // is the one a reader means.
        sqlx::query_as::<_, (i32, i32)>(
            "SELECT DISTINCT ON (d.discogs_id) d.discogs_id, d.artist_id
             FROM artist_discogs d
             JOIN artist_position p ON p.artist_id = d.artist_id
                 AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
             LEFT JOIN artist_prominence pr ON pr.artist_id = d.artist_id
             WHERE d.discogs_id = ANY($1)
             ORDER BY d.discogs_id, pr.weight DESC NULLS LAST, d.artist_id",
        )
        .bind(&artist_ids)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect()
    };

    let labels: HashSet<i32> = if label_ids.is_empty() {
        HashSet::new()
    } else {
        sqlx::query_scalar::<_, i32>("SELECT id FROM label WHERE id = ANY($1)")
            .bind(&label_ids)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect()
    };

    Ok(segments(profile, &stars, &labels))
}

/// The Discogs artist ids and label ids a description names, to look up.
fn followable(profile: &str) -> (Vec<i32>, Vec<i32>) {
    let mut artists = Vec::new();
    let mut labels = Vec::new();
    for piece in markup::pieces(profile) {
        if let Piece::Ref { kind, id: Some(id), .. } = piece {
            match kind {
                Kind::Artist => artists.push(id),
                Kind::Label => labels.push(id),
                Kind::Release | Kind::Master => {}
            }
        }
    }
    (artists, labels)
}

/// Paragraphs of segments. Separate from the lookups so the rules -- what
/// links, what reads as text, what is dropped -- are tested without Postgres.
fn segments(profile: &str, stars: &HashMap<i32, i32>, labels: &HashSet<i32>) -> Vec<Vec<Segment>> {
    profile
        .split("\n\n")
        .filter_map(|paragraph| {
            let mut out: Vec<Segment> = Vec::new();
            for piece in markup::pieces(paragraph.trim()) {
                let segment = match piece {
                    Piece::Text(text) => Segment::text(text),
                    Piece::Ref { name: None, .. } => continue,
                    Piece::Ref { kind, id, name: Some(name) } => {
                        let name = plain_name(name);
                        match (kind, id) {
                            // No star for an id the canon does not place: the
                            // words stay, as plain text.
                            (Kind::Artist, Some(id)) => Segment {
                                star: stars.get(&id).copied(),
                                ..Segment::text(name)
                            },
                            (Kind::Label, Some(id)) if labels.contains(&id) => Segment {
                                label: Some(id),
                                ..Segment::text(name)
                            },
                            _ => Segment::text(name),
                        }
                    }
                    Piece::Link { url, text } => match safe_url(url) {
                        Some(url) => Segment {
                            url: Some(url),
                            ..Segment::text(text)
                        },
                        None => Segment::text(text),
                    },
                };
                push(&mut out, segment);
            }
            let blank = out.iter().all(|segment| segment.text.trim().is_empty());
            (!blank).then_some(out)
        })
        .collect()
}

/// Appends a segment, joining it to the one before when both are plain text.
fn push(out: &mut Vec<Segment>, segment: Segment) {
    if segment.text.is_empty() {
        return;
    }
    let plain = |s: &Segment| s.star.is_none() && s.label.is_none() && s.url.is_none();
    if let Some(last) = out.last_mut()
        && plain(last)
        && plain(&segment)
    {
        last.text.push_str(&segment.text);
        return;
    }
    out.push(segment);
}

/// A web address a page may link to, or `None`.
///
/// Only `http` and `https` leave this server as links: a description is
/// upstream text, and `javascript:` in an `href` is how upstream text runs
/// code. An address written without a scheme, as Discogs profiles often do,
/// is given one.
fn safe_url(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(url.to_string())
    } else if lower.starts_with("www.") && !url.contains(char::is_whitespace) {
        Some(format!("https://{url}"))
    } else {
        None
    }
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
            dial: crate::api::listening::Dial::default(),
            listenbrainz: crate::scrobbling::Client::new("http://127.0.0.1:1"),
        })
    }

    fn text(s: &str) -> Segment {
        Segment::text(s)
    }

    #[test]
    fn drops_the_number_discogs_tells_namesakes_apart_with() {
        assert_eq!(plain_name("Antidote (4)"), "Antidote");
        assert_eq!(plain_name("Enigma Records (3)"), "Enigma Records");
        // Words in brackets are part of the name.
        assert_eq!(plain_name("Planet E (Detroit)"), "Planet E (Detroit)");
        assert_eq!(plain_name("Motown"), "Motown");
        assert_eq!(plain_name("()"), "()");
        assert_eq!(plain_name("X ()"), "X ()");
    }

    #[test]
    fn links_what_the_canon_can_follow_and_says_the_rest() {
        let stars = HashMap::from([(674, 42)]);
        let labels = HashSet::from([3]);
        let got = segments(
            "Founded by [a674=Stephan Grieder (2)] and [a239=Jesper], continued as [l3=Seasons Recordings] and [l9=Gone].",
            &stars,
            &labels,
        );
        assert_eq!(
            got,
            vec![vec![
                text("Founded by "),
                Segment {
                    star: Some(42),
                    ..text("Stephan Grieder")
                },
                text(" and Jesper, continued as "),
                Segment {
                    label: Some(3),
                    ..text("Seasons Recordings")
                },
                text(" and Gone."),
            ]]
        );
    }

    #[test]
    fn a_reference_with_no_name_leaves_no_trace() {
        let got = segments("Run by [a674] since 1996.", &HashMap::new(), &HashSet::new());
        assert_eq!(got, vec![vec![text("Run by  since 1996.")]]);
    }

    #[test]
    fn splits_paragraphs_on_a_blank_line_and_drops_empty_ones() {
        let got = segments("One.\nStill one.\n\n\n\n[b][/b]\n\nTwo.", &HashMap::new(), &HashSet::new());
        assert_eq!(got, vec![vec![text("One.\nStill one.")], vec![text("Two.")]]);
    }

    #[test]
    fn a_link_leaves_only_as_http() {
        let got = segments(
            "[url=javascript:alert(1)]click[/url] [url=http://a.example]a[/url] [url]www.b.example[/url]",
            &HashMap::new(),
            &HashSet::new(),
        );
        assert_eq!(
            got,
            vec![vec![
                text("click "),
                Segment {
                    url: Some("http://a.example".to_string()),
                    ..text("a")
                },
                text(" "),
                Segment {
                    url: Some("https://www.b.example".to_string()),
                    ..text("www.b.example")
                },
            ]]
        );
    }

    #[test]
    fn lists_the_ids_worth_looking_up() {
        let (artists, labels) = followable("[a674=A] [a=B] [l3] [l=C] [m12=D] [r1=E]");
        assert_eq!(artists, vec![674]);
        assert_eq!(labels, vec![3]);
    }

    #[test]
    fn counts_arrivals_by_year_oldest_first() {
        assert_eq!(
            arrivals([1991, 1976, 1991].into_iter()),
            vec![YearCount { year: 1976, count: 1 }, YearCount { year: 1991, count: 2 }]
        );
    }

    #[test]
    fn brightness_is_drawn_as_the_sky_draws_it() {
        assert!((brightness(4.0, 16.0) - 0.5).abs() < 1e-6);
        assert!(brightness(1.0, 0.0).abs() < f32::EPSILON, "an empty sky has no brightest star");
    }

    #[tokio::test]
    async fn a_label_id_that_is_not_a_number_does_not_reach_the_database() {
        let response = app().oneshot(Request::get("/api/labels/motown").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = app().oneshot(Request::get("/api/scenes/Q18125").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_unreachable_database_is_a_server_error_not_a_hang() {
        let response = app().oneshot(Request::get("/api/labels/1").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
