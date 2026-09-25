//! What the sky asks about a star.
//!
//! The map itself is static tiles and never touches the database, so these
//! endpoints serve only what a click or a search box needs. For the card that
//! means the whole canon meeting on one screen: the name and years from
//! `MusicBrainz`, genres from Discogs with a release count behind each, origin
//! and influence from Wikidata, the lead paragraphs from Wikipedia, and the
//! neighbours from co-listening.
//!
//! One rule here is not a matter of taste. Wikipedia prose arrives under
//! CC BY-SA, and its attribution is stored in the same row as the text; this
//! module keeps them together all the way to the wire, so a client cannot
//! receive the words without the credit they require.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api::listening::{Dial, Nebula, radio_here, radio_of};
use crate::api::relations::{Why, why_many};
use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/artists/{id}", get(artist))
        .route("/api/search", get(search))
        .route("/api/nearby", get(nearby))
        .route("/api/stars", get(stars))
        .route("/api/region", get(region))
}

/// One artist, as a card shows them.
#[derive(Serialize)]
struct Artist {
    id: i32,
    mbid: uuid::Uuid,
    name: String,
    /// `MusicBrainz`'s disambiguation comment, which is what tells two acts of
    /// one name apart.
    comment: Option<String>,
    kind: Option<String>,
    area: Option<String>,
    begin_year: Option<i16>,
    end_year: Option<i16>,
    /// Where the star sits in the current layout, when it has a place.
    position: Option<Position>,
    /// Genres by weight, strongest first.
    genres: Vec<Genre>,
    /// Nearest neighbours in the similarity graph, each with the reasons the
    /// canon can give for the edge.
    similar: Vec<Alongside>,
    /// Where the act comes from, as Wikidata records it.
    origin: Option<Origin>,
    /// Labels the act has been signed to.
    labels: Vec<String>,
    /// Who shaped this artist, and who they went on to shape. Directed, so
    /// the two directions are different facts and are kept apart.
    influenced_by: Vec<Neighbour>,
    influenced: Vec<Neighbour>,
    /// The lead of the artist's Wikipedia article, with the credit its licence
    /// requires travelling in the same value.
    prose: Option<Prose>,
    /// Release groups, newest first.
    releases: Vec<Release>,
    /// Where this artist can actually be heard, and where they can be read
    /// about. From the canon's own URL relationships, so no request leaves
    /// this server to build the card.
    listen: Vec<Link>,
    /// The playlist id that embeds this artist's `YouTube` channel, when the
    /// canon knows a channel in an embeddable form. This is the one address
    /// on the card a page can play rather than link to.
    youtube_uploads: Option<String>,
    /// The nebula whose radio this star belongs to, when that radio has
    /// anything to play. Decided here rather than read off `genres` by the
    /// client: that list is cut to eight, and "main" has one definition.
    radio: Option<Nebula>,
}

/// One outbound link, with the service named rather than the raw kind.
///
/// `MusicBrainz` describes a link by what it is for -- "free streaming",
/// "purchase for download" -- while a listener thinks in services. The host
/// carries the name, so it is read here rather than left to the client: the
/// rule for what counts as listenable belongs in one place.
#[derive(Serialize)]
struct Link {
    /// "Spotify", "Bandcamp", "`YouTube`"...
    service: String,
    url: String,
}

/// Where an act comes from, and which question that answers.
#[derive(Serialize)]
struct Origin {
    /// The place itself: "Seattle", "Liverpool".
    place: Option<String>,
    /// The country, when Wikidata records one separately.
    country: Option<String>,
    /// True when this is a person's birthplace rather than a group's place of
    /// formation. "Formed in Seattle" and "born in Seattle" are different
    /// claims and the card says which one it is showing.
    is_birth: bool,
    /// Wikidata's inception year, kept beside `MusicBrainz`'s own `begin_year`
    /// rather than replacing it: the two can disagree, and a curated value
    /// should not be silently overwritten by a crowdsourced one.
    inception_year: Option<i16>,
}

/// A Wikipedia lead, inseparable from its attribution.
///
/// Every field the licence requires is here because the row it came from
/// stores them together. Serialising the extract without them would need a
/// deliberate act, not an oversight.
#[derive(Serialize)]
struct Prose {
    extract: String,
    source_title: String,
    source_url: String,
    licence: String,
}

/// One release group, as a discography line.
#[derive(Serialize)]
struct Release {
    name: String,
    /// "Album", "Single", "EP"... from `MusicBrainz`'s primary type.
    primary_type: Option<String>,
    year: Option<i16>,
}

#[derive(Serialize)]
struct Position {
    x: f32,
    y: f32,
    /// How brightly the star is drawn: connectivity, not popularity.
    brightness: f32,
}

#[derive(Serialize)]
struct Genre {
    name: String,
    is_style: bool,
    releases: i32,
}

#[derive(Serialize)]
struct Neighbour {
    id: i32,
    name: String,
    score: f32,
}

/// A neighbour in the similarity graph, and why it is one.
///
/// Its own type rather than a field on `Neighbour`: an influence list is
/// already its own reason, and an empty `why` on every influence would be a
/// promise the card never keeps.
#[derive(Serialize)]
struct Alongside {
    id: i32,
    name: String,
    score: f32,
    why: Why,
}

async fn artist(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    match load_artist(&state.pool, &state.dial, id).await {
        Ok(Some(artist)) => (StatusCode::OK, Json(artist)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "no such artist" }))).into_response(),
        Err(error) => {
            tracing::error!(%error, artist = id, "failed to read an artist");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be read" })),
            )
                .into_response()
        }
    }
}

async fn load_artist(pool: &PgPool, dial: &Dial, id: i32) -> sqlx::Result<Option<Artist>> {
    let Some(row) = sqlx::query_as::<
        _,
        (
            i32,
            uuid::Uuid,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i16>,
            Option<i16>,
        ),
    >("SELECT id, mbid, name, comment, kind, area, begin_year, end_year FROM artist WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    // The newest layout is the one the client is looking at; an older one
    // would place the star somewhere the map does not show it.
    let position = sqlx::query_as::<_, (f32, f32, Option<f32>)>(
        "SELECT p.x, p.y, pr.weight
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE p.artist_id = $1
         ORDER BY l.created_at DESC
         LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(|(x, y, weight)| Position {
        x,
        y,
        brightness: weight.unwrap_or(0.0),
    });

    let genres = sqlx::query_as::<_, (String, bool, i32)>(
        "SELECT g.name, g.is_style, ag.releases
         FROM artist_genre ag JOIN genre g ON g.id = ag.genre_id
         WHERE ag.artist_id = $1
         ORDER BY ag.releases DESC, g.name
         LIMIT 8",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, is_style, releases)| Genre { name, is_style, releases })
    .collect();

    let similar = alongside(pool, id).await?;

    let (listen, youtube_uploads) = listen(pool, id).await?;
    let radio = radio_of(pool, dial, id).await;

    let origin = origin(pool, id).await?;
    let labels = labels(pool, id).await?;
    let prose = prose(pool, id).await?;
    let releases = releases(pool, id).await?;

    // Influence is directed, so each direction is its own query rather than
    // one query over both columns: "shaped by" and "went on to shape" are
    // different claims, and collapsing them would invent symmetry Wikidata
    // never asserted. Ordered by brightness so the better-known names lead.
    let influenced_by = influences(pool, id, Direction::Sources).await?;
    let influenced = influences(pool, id, Direction::Targets).await?;

    let (id, mbid, name, comment, kind, area, begin_year, end_year) = row;
    Ok(Some(Artist {
        id,
        mbid,
        name,
        comment,
        kind,
        area,
        begin_year,
        end_year,
        position,
        genres,
        similar,
        origin,
        labels,
        influenced_by,
        influenced,
        prose,
        releases,
        listen,
        youtube_uploads,
        radio,
    }))
}

/// The nearest neighbours, each with the reasons the canon gives for the edge.
async fn alongside(pool: &PgPool, id: i32) -> sqlx::Result<Vec<Alongside>> {
    // Similarity is stored once per unordered pair, so neighbours come from
    // both columns.
    //
    // The metric is pinned for the same reason as in `influences`: an edge is
    // keyed by (metric_id, source_id, target_id), and a second metric would
    // otherwise put the same neighbour in the list once per metric, on scores
    // that are not comparable across metrics anyway.
    let neighbours = sqlx::query_as::<_, (i32, String, f32)>(
        "SELECT other.id, other.name, e.score
         FROM artist_similarity e
         JOIN artist other ON other.id = CASE WHEN e.source_id = $1 THEN e.target_id ELSE e.source_id END
         WHERE (e.source_id = $1 OR e.target_id = $1)
           AND e.metric_id = (SELECT max(id) FROM similarity_metric)
         ORDER BY e.score DESC
         LIMIT 10",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let ids: Vec<i32> = neighbours.iter().map(|n| n.0).collect();
    let mut reasons = why_many(&mut *pool.acquire().await?, id, &ids).await?;
    Ok(neighbours
        .into_iter()
        .map(|(other, name, score)| Alongside {
            id: other,
            name,
            score,
            // The edge is the co-listening reason, so it is filled from the
            // score already in hand rather than asked for a second time.
            why: Why {
                co_listening: Some(score),
                ..reasons.remove(&other).unwrap_or_default()
            },
        })
        .collect())
}

/// Where the act comes from, resolved from Wikidata item ids into words.
///
/// Wikidata stores a place as an item id, and the labels for those items were
/// captured during the same dump pass precisely so a card does not have to
/// reach back into a hundred gigabytes to say "Seattle".
async fn origin(pool: &PgPool, id: i32) -> sqlx::Result<Option<Origin>> {
    Ok(sqlx::query_as::<_, (Option<String>, Option<String>, Option<bool>, Option<i16>)>(
        "SELECT place.label, country.label, f.origin_is_birth, f.inception_year
         FROM artist_fact f
         LEFT JOIN wikidata_item place ON place.qid = f.origin_qid
         LEFT JOIN wikidata_item country ON country.qid = f.country_qid
         WHERE f.artist_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(|(place, country, is_birth, inception_year)| Origin {
        place,
        country,
        is_birth: is_birth.unwrap_or(false),
        inception_year,
    }))
}

async fn labels(pool: &PgPool, id: i32) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT item.label
         FROM artist_wikidata_label l
         JOIN wikidata_item item ON item.qid = l.label_qid
         WHERE l.artist_id = $1 AND item.label IS NOT NULL
         ORDER BY item.label
         LIMIT 8",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// The Wikipedia lead, selected together with the credit its licence requires.
///
/// The extract and the attribution come out of the row that stores them
/// together. There is no query in this codebase that can hand back the words
/// alone.
async fn prose(pool: &PgPool, id: i32) -> sqlx::Result<Option<Prose>> {
    Ok(
        sqlx::query_as::<_, (String, String, String, String)>("SELECT extract, source_title, source_url, licence FROM artist_prose WHERE artist_id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|(extract, source_title, source_url, licence)| Prose {
                extract,
                source_title,
                source_url,
                licence,
            }),
    )
}

/// The discography, ordered albums first and then oldest first.
///
/// That approximates "the records this artist is known for" from the only
/// fields the canon holds, and neither half is arbitrary. Newest-first looks
/// obvious and is wrong: the Beatles carry 696 release groups typed Album,
/// almost all reissues and compilations, so the newest twelve are 2025-2026
/// repackagings and Abbey Road is nowhere. Oldest-first surfaces the debut,
/// because compilations of a body of work can only come after it.
///
/// The honest limit: `MusicBrainz` separates a studio album from a live one
/// through *secondary* types, which the v0.2.0 import does not read, so
/// concert recordings typed Album still appear among the studio records.
/// Fixing that means re-importing the canon, which is a stage of its own.
async fn releases(pool: &PgPool, id: i32) -> sqlx::Result<Vec<Release>> {
    Ok(sqlx::query_as::<_, (String, Option<String>, Option<i16>)>(
        "SELECT name, primary_type, year
         FROM release_group
         WHERE artist_id = $1
         ORDER BY CASE primary_type
                    WHEN 'Album' THEN 0
                    WHEN 'EP' THEN 1
                    WHEN 'Single' THEN 2
                    ELSE 3
                  END,
                  year IS NULL, year, name
         LIMIT 12",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, primary_type, year)| Release { name, primary_type, year })
    .collect())
}

/// Where the artist can be heard, newest-known service first, and the channel
/// the card can play.
///
/// Read from the canon's own URL relationships: nothing here calls out to a
/// streaming service, which is what ADR 0002 requires of the critical path.
/// The trade is visible on the card - these are artist pages, not tracks,
/// because `MusicBrainz` links an artist to a service and not a recording to
/// one. Measured on the slice: of 100,000 placed stars, 49,090 have somewhere
/// to go and 14,198 have a `YouTube` channel.
async fn listen(pool: &PgPool, id: i32) -> sqlx::Result<(Vec<Link>, Option<String>)> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT kind, url
         FROM artist_url
         WHERE artist_id = $1
           AND kind IN ('free streaming', 'streaming', 'bandcamp', 'soundcloud',
                        'youtube', 'purchase for download', 'official homepage')
         ORDER BY kind, url",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    // Read before the links are cut to one per service below: an artist whose
    // first `YouTube` link is a handle and whose second is the channel itself
    // would otherwise keep the handle and lose the player. 103 stars of the
    // slice were silent that way while the radio could play them.
    let uploads = channel_of(rows.iter().filter(|(kind, _)| kind == "youtube").map(|(_, url)| url.as_str()));

    let mut links: Vec<Link> = Vec::new();
    for (kind, url) in rows {
        let service = service_of(&url, &kind);
        // One row per service, and one per address: an artist with four
        // Spotify links is a cataloguing artefact, not four places to listen.
        // The address check catches the same shop filed under two kinds --
        // the Beatles' Korean shop is both "streaming" and "purchase for
        // download", and unnamed links keep the kind as their service, so
        // deduplicating by name alone would let it through twice.
        if links.iter().any(|link| link.service == service || link.url == url) {
            continue;
        }
        links.push(Link { service, url });
    }
    // Streaming first, then the artist's own places: someone who clicked a
    // star wants to hear it before they want to read a homepage.
    links.sort_by_key(|link| service_rank(&link.service));
    // Measured on the canon: a median artist has 4 of these and 90% have 9 or
    // fewer, but the tail reaches 53 - a wall of regional shops nobody scrolls.
    // The ranking above means what survives the cut is the part worth showing.
    links.truncate(8);
    Ok((links, uploads))
}

/// Names the service behind a URL.
///
/// Matched on the registrable name rather than the whole host, for two
/// reasons found in the canon: Bandcamp gives every artist their own
/// subdomain (10,006 of them, all one service), and the shops run national
/// domains -- `music.amazon.com` and `music.amazon.co.uk` are the same shop.
/// So the host is reduced to the label before its public suffix, and matched
/// exactly. That exactness matters: a substring test would read
/// `notspotify.com` as Spotify.
///
/// Anything unrecognised falls back to `MusicBrainz`'s own word for the link,
/// which is a fair description even when it is not a name -- the canon has a
/// long tail of shops this list will never cover.
fn service_of(url: &str, kind: &str) -> String {
    const SERVICES: [(&str, &str); 18] = [
        ("spotify", "Spotify"),
        ("deezer", "Deezer"),
        ("apple", "Apple Music"),
        ("itunes", "Apple Music"),
        ("tidal", "Tidal"),
        ("youtube", "YouTube"),
        ("soundcloud", "SoundCloud"),
        ("bandcamp", "Bandcamp"),
        ("qobuz", "Qobuz"),
        ("beatport", "Beatport"),
        ("amazon", "Amazon Music"),
        ("napster", "Napster"),
        ("junodownload", "Juno Download"),
        ("traxsource", "Traxsource"),
        ("pandora", "Pandora"),
        ("7digital", "7digital"),
        ("melon", "Melon"),
        ("mora", "mora"),
    ];

    let Some(name) = registrable_name(url) else { return kind.to_string() };
    for (label, service) in SERVICES {
        if name == label {
            return (*service).to_string();
        }
    }
    kind.to_string()
}

/// The "uploads" playlist of a `YouTube` channel, which is embeddable.
///
/// Every channel has an implicit playlist of everything it has posted, and its
/// id is the channel id with `UC` swapped for `UU`. That playlist embeds
/// through the standard player, so the card can play an artist's own channel
/// without the `YouTube` Data API -- which ADR 0002 keeps out of the critical
/// path.
///
/// Only the `/channel/UC…` form carries the id: `/user/…` and `/@handle` links
/// name a channel without giving its id, and resolving them needs the very API
/// this avoids. Measured on the canon: 10,155 of 15,944 channels are the
/// embeddable form; the rest stay a link.
pub(crate) fn uploads_playlist(url: &str) -> Option<String> {
    let id = url.split("/channel/").nth(1)?.split(['/', '?', '#']).next()?;
    // A channel id is "UC" and 22 more characters of base64url. Checked so a
    // malformed link becomes no player rather than a broken one.
    let rest = id.strip_prefix("UC")?;
    let well_formed = rest.len() == 22 && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    well_formed.then(|| format!("UU{rest}"))
}

/// The channel a star plays: the first of its `YouTube` links, in address
/// order, that names a channel by id.
///
/// One rule for the card and the radio, so a star the radio plays is a star
/// whose card can play it too.
pub(crate) fn channel_of<'a>(urls: impl IntoIterator<Item = &'a str>) -> Option<String> {
    urls.into_iter().find_map(uploads_playlist)
}

/// The label a domain is registered under: `music.amazon.co.uk` -> `amazon`.
///
/// Not a public-suffix list -- that would be a dependency and a data file for
/// a naming nicety. The rule instead: drop the last label, and drop the one
/// before it too when it is a country-code second level like `co.uk` or
/// `com.au`. Wrong for a handful of exotic suffixes, which then fall back to
/// the link's own kind rather than being named wrongly.
fn registrable_name(url: &str) -> Option<String> {
    let host = url.split("//").nth(1).unwrap_or(url).split('/').next()?;
    let host = host.split(':').next()?;
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    if labels.len() < 2 {
        return None;
    }

    let last = labels[labels.len() - 1];
    let second = labels[labels.len() - 2];
    // "co.uk", "com.au", "co.jp": the second-level label is a suffix too, so
    // the name is one further left.
    let name_at = if last.len() == 2 && matches!(second, "co" | "com" | "net" | "org" | "ac" | "or") {
        labels.len().checked_sub(3)?
    } else {
        labels.len() - 2
    };
    Some(labels[name_at].to_lowercase())
}

/// Listening comes before reading, and the big services before the rest.
fn service_rank(service: &str) -> u8 {
    match service {
        "YouTube" => 0,
        "Spotify" | "Apple Music" | "Deezer" | "Tidal" => 1,
        "Bandcamp" | "SoundCloud" => 2,
        "official homepage" => 9,
        _ => 5,
    }
}

/// Which end of the influence arrow to follow.
#[derive(Clone, Copy)]
enum Direction {
    /// Who shaped this artist.
    Sources,
    /// Whom this artist shaped.
    Targets,
}

async fn influences(pool: &PgPool, id: i32, direction: Direction) -> sqlx::Result<Vec<Neighbour>> {
    // Two fixed statements rather than one with a swapped column pair: the
    // planner sees each one whole, and each uses the index built for its own
    // direction (the reverse one exists for exactly this query).
    //
    // Prominence is keyed by (metric_id, artist_id), so joining it without
    // naming a metric would return one row per metric and repeat every
    // influence in the list. Today there is a single metric and the bug would
    // be invisible; v0.17.1 adds a second one, at which point every card would
    // quietly show its influences twice. The subquery pins one metric now.
    let sql = match direction {
        Direction::Sources => {
            "SELECT a.id, a.name, coalesce(p.weight, 0)
             FROM artist_influence i
             JOIN artist a ON a.id = i.influence_id
             LEFT JOIN artist_prominence p
               ON p.artist_id = a.id
              AND p.metric_id = (SELECT max(id) FROM similarity_metric)
             WHERE i.artist_id = $1
             ORDER BY coalesce(p.weight, 0) DESC, a.name
             LIMIT 8"
        }
        Direction::Targets => {
            "SELECT a.id, a.name, coalesce(p.weight, 0)
             FROM artist_influence i
             JOIN artist a ON a.id = i.artist_id
             LEFT JOIN artist_prominence p
               ON p.artist_id = a.id
              AND p.metric_id = (SELECT max(id) FROM similarity_metric)
             WHERE i.influence_id = $1
             ORDER BY coalesce(p.weight, 0) DESC, a.name
             LIMIT 8"
        }
    };

    Ok(sqlx::query_as::<_, (i32, String, f32)>(sql)
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, name, score)| Neighbour { id, name, score })
        .collect())
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

/// A search hit: enough to list it and to fly to it.
#[derive(Serialize, Clone)]
struct Hit {
    id: i32,
    name: String,
    comment: Option<String>,
    x: Option<f32>,
    y: Option<f32>,
}

async fn search(State(state): State<AppState>, Query(query): Query<SearchQuery>) -> Response {
    let term = query.q.trim();
    if term.len() < 2 {
        return (StatusCode::OK, Json(Vec::<Hit>::new())).into_response();
    }

    match run_search(&state.pool, term).await {
        Ok(hits) => (StatusCode::OK, Json(hits)).into_response(),
        Err(error) => {
            tracing::error!(%error, "search failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be searched" })),
            )
                .into_response()
        }
    }
}

async fn run_search(pool: &PgPool, term: &str) -> sqlx::Result<Vec<Hit>> {
    // Substring rather than prefix: an English band as famous as "The
    // Beatles" starts with an article, and a prefix search for "beatles"
    // would find a tribute act called "Beatless" and miss them entirely.
    //
    // Only among stars that have a place -- a result the map cannot fly to is
    // a dead end -- and ranked so the obvious answer leads: an exact name
    // first, then how woven into the graph the artist is. Connectivity beats
    // where the match falls, or searching "beatles" leads with a band called
    // "Beatless" simply because the word starts its name.
    let rows = sqlx::query_as::<_, (i32, String, Option<String>, Option<f32>, Option<f32>)>(
        "SELECT a.id, a.name, a.comment, p.x, p.y
         FROM artist a
         JOIN artist_position p ON p.artist_id = a.id
         LEFT JOIN artist_prominence pr ON pr.artist_id = a.id
         WHERE a.name ILIKE '%' || $1 || '%'
         ORDER BY
             (lower(a.name) = lower($1)) DESC,
             COALESCE(pr.weight, 0) DESC,
             (a.name ILIKE $1 || '%') DESC,
             length(a.name),
             a.name
         LIMIT 12",
    )
    .bind(term)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(id, name, comment, x, y)| Hit { id, name, comment, x, y }).collect())
}

#[derive(Deserialize)]
struct StarsQuery {
    /// Comma-separated artist ids, in the order they are wanted back.
    ids: String,
}

/// How many stars one request may name.
///
/// The same bound a route in the address has: this endpoint exists so a route
/// of stars can be drawn without opening a card per stop, and a route is a
/// walk through the sky, not an export of it.
pub const MAX_STARS: usize = 50;

/// A few named stars, with their places: what a route needs to draw itself.
///
/// Returned in the order asked, because a route's order is its meaning; an id
/// the canon does not know is left out rather than failing the rest, since one
/// stale stop should not erase a route someone sent.
async fn stars(State(state): State<AppState>, Query(query): Query<StarsQuery>) -> Response {
    let Some(ids) = parse_ids(&query.ids) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("ids must be 1 to {MAX_STARS} numbers separated by commas") })),
        )
            .into_response();
    };

    match run_stars(&state.pool, &ids).await {
        Ok(stars) => (StatusCode::OK, Json(stars)).into_response(),
        Err(error) => {
            tracing::error!(%error, "reading stars failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the canon could not be read" })),
            )
                .into_response()
        }
    }
}

/// `"54,962,120"` -> the ids, or `None` when the list is empty, too long, or
/// holds anything that is not a positive id.
///
/// All or nothing: a list with one garbled entry is a garbled address, and
/// answering the part that parsed would draw a route with a stop silently
/// missing. Repeats are kept -- a route may come back to where it started.
fn parse_ids(raw: &str) -> Option<Vec<i32>> {
    let ids: Vec<i32> = raw
        .split(',')
        .map(|part| part.trim().parse::<i32>().ok().filter(|id| *id > 0))
        .collect::<Option<_>>()?;
    (!ids.is_empty() && ids.len() <= MAX_STARS).then_some(ids)
}

async fn run_stars(pool: &PgPool, ids: &[i32]) -> sqlx::Result<Vec<Hit>> {
    let rows = sqlx::query_as::<_, (i32, String, Option<String>, Option<f32>, Option<f32>)>(
        "SELECT a.id, a.name, a.comment, p.x, p.y
         FROM artist a
         LEFT JOIN artist_position p
             ON p.artist_id = a.id
            AND p.layout_id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
         WHERE a.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    let by_id: std::collections::HashMap<i32, Hit> = rows
        .into_iter()
        .map(|(id, name, comment, x, y)| (id, Hit { id, name, comment, x, y }))
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.get(id)).cloned().collect())
}

#[derive(Deserialize)]
struct NearbyQuery {
    /// The visible rectangle in layout coordinates.
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

/// A star in view: enough to name it in a list and to open its card.
#[derive(Serialize)]
struct Nearby {
    id: i32,
    name: String,
    comment: Option<String>,
    x: f32,
    y: f32,
}

/// The stars in a rectangle, most prominent first.
///
/// This exists for the list beside the canvas, which is how the sky is read by
/// a keyboard and by a screen reader. It is a question the *server* has to
/// answer rather than the client: the tiles the canvas draws from carry ids and
/// positions but no names, and at a wide zoom they are a thinned sample rather
/// than everything in view — so a client-side list would be a list of the stars
/// that happened to survive thinning, named by one request each.
///
/// Ordered by prominence and not by distance from the middle. A reader asking
/// "what am I looking at" wants the names worth knowing in this patch of sky,
/// and the nearest star to the exact centre of an arbitrary pan is nobody in
/// particular.
async fn nearby(State(state): State<AppState>, Query(query): Query<NearbyQuery>) -> Response {
    // A rectangle given the wrong way round is a client bug, not an empty
    // patch of sky: answering `[]` would send whoever wrote it looking for
    // missing stars. The bounds are normalised instead, which is the one
    // reading that cannot be wrong.
    let (min_x, max_x) = minmax(query.min_x, query.max_x);
    let (min_y, max_y) = minmax(query.min_y, query.max_y);

    match run_nearby(&state.pool, min_x, min_y, max_x, max_y).await {
        Ok(stars) => (StatusCode::OK, Json(stars)).into_response(),
        Err(error) => {
            tracing::error!(%error, "nearby failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the sky could not be read" })),
            )
                .into_response()
        }
    }
}

fn minmax(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}

/// How many names the list carries.
///
/// Twelve, the same as the search box. A list long enough to need scrolling is
/// a second map rather than a way of reading this one, and a reader who wants
/// more moves the view.
const NEARBY_LIMIT: i64 = 12;

async fn run_nearby(pool: &PgPool, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> sqlx::Result<Vec<Nearby>> {
    // The newest layout, pinned the way the card's own position query pins it:
    // positions belong to a layout, and a query that joined every layout would
    // return one star several times over as soon as a second layout exists.
    let rows = sqlx::query_as::<_, (i32, String, Option<String>, f32, f32)>(
        "SELECT a.id, a.name, a.comment, p.x, p.y
         FROM artist_position p
         JOIN sky_layout l ON l.id = p.layout_id
         JOIN artist a ON a.id = p.artist_id
         LEFT JOIN artist_prominence pr
             ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
         WHERE l.id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
           AND p.x BETWEEN $1 AND $3
           AND p.y BETWEEN $2 AND $4
         ORDER BY COALESCE(pr.weight, 0) DESC, a.name
         LIMIT $5",
    )
    .bind(min_x)
    .bind(min_y)
    .bind(max_x)
    .bind(max_y)
    .bind(NEARBY_LIMIT)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(id, name, comment, x, y)| Nearby { id, name, comment, x, y }).collect())
}

/// What the middle of the view is: its commonest main style and genre, and
/// the radio of the place.
#[derive(Serialize)]
struct Region {
    style: Option<String>,
    genre: Option<String>,
    /// What "listen here" plays: the style when its radio has enough to play,
    /// else the genre. `None` over a patch of sky with no channels in it.
    radio: Option<Nebula>,
}

/// How many of the most prominent stars in the rectangle are asked what they
/// are. Enough for a majority to mean something, few enough that a view of the
/// whole canon is not a count over millions of rows.
const REGION_SAMPLE: i64 = 300;

/// Where the camera is, in the canon's words.
///
/// The compass's "where am I". Answered by the stars themselves rather than by
/// the names written on the map: measured on the slice, a dozen name anchors
/// sit within 25 units of the core's middle, so "the nearest name" there is a
/// coin toss -- Marvin Gaye's patch came out as Pop Rock. Asking the stars in
/// view which style is most often their main one says what the patch *is*.
async fn region(State(state): State<AppState>, Query(query): Query<NearbyQuery>) -> Response {
    let (min_x, max_x) = minmax(query.min_x, query.max_x);
    let (min_y, max_y) = minmax(query.min_y, query.max_y);
    match run_region(&state.pool, min_x, min_y, max_x, max_y).await {
        Ok(mut region) => {
            region.radio = radio_here(&state.pool, &state.dial, region.genre.as_deref(), region.style.as_deref()).await;
            (StatusCode::OK, Json(region)).into_response()
        }
        Err(error) => {
            tracing::error!(%error, "region failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "the sky could not be read" })),
            )
                .into_response()
        }
    }
}

async fn run_region(pool: &PgPool, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> sqlx::Result<Region> {
    // The sample is the same "most prominent in the rectangle" the nearby list
    // reads; each star then votes with its main genre and its main style --
    // `main_genres`, the same "main" the names on the map are built from and
    // the radio plays.
    let rows = sqlx::query_as::<_, (String, bool)>(
        "WITH sample AS (
             SELECT p.artist_id
             FROM artist_position p
             JOIN sky_layout l ON l.id = p.layout_id
             LEFT JOIN artist_prominence pr ON pr.artist_id = p.artist_id AND pr.metric_id = l.metric_id
             WHERE l.id = (SELECT id FROM sky_layout ORDER BY created_at DESC LIMIT 1)
               AND p.x BETWEEN $1 AND $3
               AND p.y BETWEEN $2 AND $4
             ORDER BY COALESCE(pr.weight, 0) DESC
             LIMIT $5
         ),
         tally AS (
             SELECT g.name, g.is_style, count(*) AS votes
             FROM main_genres(ARRAY(SELECT artist_id FROM sample)) main
             JOIN genre g ON g.id = main.genre_id
             GROUP BY g.name, g.is_style
         )
         SELECT DISTINCT ON (is_style) name, is_style
         FROM tally
         ORDER BY is_style, votes DESC, name",
    )
    .bind(min_x)
    .bind(min_y)
    .bind(max_x)
    .bind(max_y)
    .bind(REGION_SAMPLE)
    .fetch_all(pool)
    .await?;

    let pick = |style: bool| rows.iter().find(|row| row.1 == style).map(|row| row.0.clone());
    Ok(Region {
        style: pick(true),
        genre: pick(false),
        radio: None,
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;

    /// A pool that never reaches a database, so routing and argument handling
    /// can be exercised without one.
    fn dead_pool() -> PgPool {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_lazy("postgres://nobody:nowhere@127.0.0.1:1/lyrid")
            .expect("lazy pool creation does not touch the network")
    }

    fn app() -> Router {
        routes().with_state(AppState {
            pool: dead_pool(),
            secure_cookie: false,
            public_url: "http://localhost:8080".to_string(),
            mailer: crate::mail::Mailer::Log,
            dial: crate::api::listening::Dial::default(),
        })
    }

    #[test]
    fn names_a_service_by_its_domain_suffix() {
        // Bandcamp gives every artist a subdomain -- 10,006 of them in the
        // canon -- so an exact host match would name none of them.
        assert_eq!(service_of("https://3six.bandcamp.com/", "bandcamp"), "Bandcamp");
        assert_eq!(service_of("https://open.spotify.com/artist/31v7", "streaming"), "Spotify");
        assert_eq!(service_of("https://music.apple.com/us/artist/420535261", "streaming"), "Apple Music");
        assert_eq!(service_of("https://itunes.apple.com/artist/1", "purchase for download"), "Apple Music");
        assert_eq!(service_of("https://www.youtube.com/channel/UC58", "youtube"), "YouTube");
    }

    #[test]
    fn a_channel_link_becomes_its_uploads_playlist() {
        // UC -> UU is the whole trick, and it is what lets the card play a
        // channel without the YouTube Data API.
        assert_eq!(
            uploads_playlist("https://www.youtube.com/channel/UCc4K7bAqpdBP8jh1j9XZAww").as_deref(),
            Some("UUc4K7bAqpdBP8jh1j9XZAww")
        );
        // Trailing paths and queries are not part of the id.
        assert_eq!(
            uploads_playlist("https://www.youtube.com/channel/UCc4K7bAqpdBP8jh1j9XZAww/videos?x=1").as_deref(),
            Some("UUc4K7bAqpdBP8jh1j9XZAww")
        );
    }

    #[test]
    fn a_channel_without_an_id_stays_a_link() {
        // 5,789 of the canon's 15,944 channels are named rather than
        // identified; resolving them needs the API this design avoids, so
        // they get no player at all rather than a broken one.
        assert_eq!(uploads_playlist("https://www.youtube.com/user/gratefulvideo"), None);
        assert_eq!(uploads_playlist("https://www.youtube.com/@someartist"), None);
        assert_eq!(uploads_playlist("https://www.youtube.com/c/NateIngalls"), None);
        // Malformed ids are refused rather than turned into a dead player.
        assert_eq!(uploads_playlist("https://www.youtube.com/channel/UCtooshort"), None);
        assert_eq!(uploads_playlist("https://www.youtube.com/channel/XY123456789012345678901"), None);
    }

    #[test]
    fn a_handle_filed_first_does_not_hide_the_channel() {
        // Addresses sort `@` before `c`, so a handle is often an artist's
        // first YouTube link; the channel after it is still the one to play.
        let links = [
            "https://www.youtube.com/@someartist",
            "https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv",
        ];
        assert_eq!(channel_of(links), Some("UUabcdefghijklmnopqrstuv".to_string()));
        assert_eq!(channel_of(["https://www.youtube.com/@someartist"]), None);
    }

    #[test]
    fn a_national_domain_is_the_same_shop() {
        // Found on the card: music.amazon.co.uk went unnamed while
        // music.amazon.com was named, which reads as two different shops.
        assert_eq!(service_of("https://music.amazon.co.uk/artists/B00G", "streaming"), "Amazon Music");
        assert_eq!(service_of("https://music.amazon.com/artists/B001", "streaming"), "Amazon Music");
        assert_eq!(service_of("https://us.7digital.com/artist/1", "purchase for download"), "7digital");
        assert_eq!(service_of("https://www.7digital.com/artist/1", "purchase for download"), "7digital");
    }

    #[test]
    fn a_lookalike_domain_is_not_the_service() {
        // The suffix must be a domain boundary: "notspotify.com" and
        // "spotify.com.example.org" are not Spotify.
        assert_eq!(service_of("https://notspotify.com/artist/1", "streaming"), "streaming");
        assert_eq!(service_of("https://spotify.com.example.org/x", "streaming"), "streaming");
    }

    #[test]
    fn an_unknown_host_keeps_musicbrainzs_own_word_for_it() {
        // Better a fair description than a guessed brand: the canon has a long
        // tail of shops this list will never name.
        assert_eq!(service_of("https://music.bugs.co.kr/artist/1", "streaming"), "streaming");
        assert_eq!(service_of("https://ototoy.jp/artist/1", "purchase for download"), "purchase for download");
        // And a URL with no host at all does not become one.
        assert_eq!(service_of("not a url", "streaming"), "streaming");
    }

    #[tokio::test]
    async fn a_short_search_term_answers_empty_without_touching_the_database() {
        // One letter would match a large share of three million artists, so
        // the query is refused before it is asked -- which is also why this
        // test can pass with no database behind it.
        let response = app().oneshot(Request::get("/api/search?q=a").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");
    }

    #[tokio::test]
    async fn a_search_without_a_term_is_a_bad_request_rather_than_a_crash() {
        let response = app().oneshot(Request::get("/api/search").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_unreachable_database_is_a_server_error_not_a_hang() {
        let response = app().oneshot(Request::get("/api/artists/1").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body.get("error").is_some(), "an error response should say so: {body}");
    }

    #[tokio::test]
    async fn a_non_numeric_artist_id_does_not_reach_the_database() {
        let response = app().oneshot(Request::get("/api/artists/nirvana").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn a_rectangle_given_backwards_is_normalised_rather_than_answered_empty() {
        // `BETWEEN 5 AND -5` matches nothing in Postgres, so a viewport sent
        // the wrong way round would report an empty patch of sky -- which
        // reads as "no stars here" rather than as "you asked wrongly", and
        // sends whoever wrote the client looking for missing data.
        assert_eq!(minmax(-5.0, 5.0), (-5.0, 5.0));
        assert_eq!(minmax(5.0, -5.0), (-5.0, 5.0));
        // A degenerate rectangle is left alone: a viewport of zero width is a
        // client that has not measured its canvas yet, and it asks again.
        assert_eq!(minmax(3.0, 3.0), (3.0, 3.0));
    }

    #[tokio::test]
    async fn a_viewport_missing_a_bound_is_a_bad_request_rather_than_a_guess() {
        // All four bounds or none: defaulting a missing edge to zero would
        // answer a rectangle nobody asked about, and the answer would look
        // plausible.
        let response = app()
            .oneshot(Request::get("/api/nearby?min_x=0&min_y=0&max_x=1").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn a_list_of_ids_is_read_whole_or_not_at_all() {
        assert_eq!(parse_ids("54,962,120"), Some(vec![54, 962, 120]));
        assert_eq!(parse_ids(" 7 , 7 "), Some(vec![7, 7]), "a route may return to a star");
        // One garbled stop is a garbled address, not a shorter route.
        assert_eq!(parse_ids("54,x,120"), None);
        assert_eq!(parse_ids("54,,120"), None);
        assert_eq!(parse_ids("54,-1"), None);
        assert_eq!(parse_ids(""), None);
    }

    #[test]
    fn a_list_of_ids_has_the_same_bound_as_a_route() {
        let fifty = (1..=50).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        assert_eq!(parse_ids(&fifty).map(|ids| ids.len()), Some(50));
        let fifty_one = (1..=51).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        assert_eq!(parse_ids(&fifty_one), None);
    }

    #[tokio::test]
    async fn a_region_missing_a_bound_is_a_bad_request_rather_than_a_guess() {
        let response = app()
            .oneshot(Request::get("/api/region?min_x=0&min_y=0&max_x=1").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_garbled_list_of_stars_is_a_bad_request_before_the_database() {
        let response = app().oneshot(Request::get("/api/stars?ids=1,two").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_unreachable_database_makes_the_nearby_list_a_server_error() {
        let response = app()
            .oneshot(Request::get("/api/nearby?min_x=-1&min_y=-1&max_x=1&max_y=1").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body.get("error").is_some(), "an error response should say so: {body}");
    }
}
