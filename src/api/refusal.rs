//! How the API says no.
//!
//! Every refusal the server gives is one row of the table below, and every
//! one answers the same shape:
//!
//! ```json
//! {"code": "token_invalid", "error": "that link is not valid any more - ask for a new one"}
//! ```
//!
//! The **code** is the contract. It is written once, here, never changes once
//! shipped, and is what the interface looks its own words up by, in the
//! reader's language (`errors.<code>` in `web/src/i18n/locales`). The
//! **phrase** is English for whoever reads a log or a `curl`, and the words the
//! interface falls back to when it meets a code newer than itself. A refusal
//! that needs a number to be said -- the shortest password, the longest list
//! of stars -- carries it beside the two, under the name the catalogue
//! interpolates.
//!
//! The variant is the only way a handler has of refusing: there is no
//! function taking a message any more, so a refusal without a code cannot be
//! written. What the framework refuses before a handler runs -- a body that
//! is not JSON, a method a route does not take, an address under `/api` that
//! is not a route -- is given a code by [`coded`], one layer over the whole
//! API, so the promise holds for every extractor there is, not only the ones
//! used today.

use std::borrow::Cow;

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value};

/// The refusals, one row each: the variant, its code on the wire, the status
/// it answers with and its English phrase.
///
/// The code is spelt out rather than derived from the variant's name, so that
/// renaming a variant in Rust cannot quietly rename the contract.
macro_rules! refusals {
    ($( $(#[$doc:meta])* $variant:ident = $code:literal, $status:ident, $phrase:expr; )*) => {
        /// Why the API said no. See the module for the shape it answers.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Refusal {
            $( $(#[$doc])* $variant, )*
        }

        impl Refusal {
            /// Every refusal there is, for the tests that hold the catalogue
            /// and the documentation to this table.
            #[cfg(test)]
            pub(crate) const ALL: &[Self] = &[$(Self::$variant),*];

            /// The stable name of this refusal on the wire.
            pub(crate) const fn code(self) -> &'static str {
                match self { $(Self::$variant => $code,)* }
            }

            /// The status it answers with.
            pub(crate) const fn status(self) -> StatusCode {
                match self { $(Self::$variant => StatusCode::$status,)* }
            }

            /// What it says in English.
            pub(crate) fn phrase(self) -> Cow<'static, str> {
                match self { $(Self::$variant => Cow::from($phrase),)* }
            }
        }
    };
}

refusals! {
    // ------------------------------------------------------------ requests
    /// A body, a query or a path segment the route could not read.
    MalformedRequest = "malformed_request", BAD_REQUEST, "the request could not be read";
    /// An address under `/api` that is not a route.
    NoSuchRoute = "no_such_route", NOT_FOUND, "there is no such route";
    /// A route asked with a method it does not take.
    MethodNotAllowed = "method_not_allowed", METHOD_NOT_ALLOWED, "that route does not take that method";
    /// A body over the route's limit.
    BodyTooLarge = "body_too_large", PAYLOAD_TOO_LARGE, "the body is larger than this route takes";
    /// A body that is not the type the route reads.
    UnsupportedMediaType = "unsupported_media_type", UNSUPPORTED_MEDIA_TYPE, "the body is not of a type this route reads";
    /// A failure the framework answered with no words of its own.
    ServerFailed = "server_failed", INTERNAL_SERVER_ERROR, "something went wrong on the server";

    // ----------------------------------------------------- signing in
    NotSignedIn = "not_signed_in", UNAUTHORIZED, "not signed in";
    /// A wrong password and an address with no account, in one answer.
    CredentialsWrong = "credentials_wrong", UNAUTHORIZED, "that address and password do not match an account";
    EmailMalformed = "email_malformed", BAD_REQUEST, "that does not look like an email address";
    /// Carries `min`, the shortest password taken.
    PasswordTooShort = "password_too_short", BAD_REQUEST, format!("a password needs at least {} characters", crate::auth::MIN_PASSWORD);
    EmailTaken = "email_taken", CONFLICT, "an account with that address already exists";
    /// A link from a letter that has expired, been used or was never issued:
    /// one code for the three, because which of them it is is nobody's
    /// business but the server's.
    TokenInvalid = "token_invalid", BAD_REQUEST, "that link is not valid any more - ask for a new one";
    NotAdmin = "not_admin", FORBIDDEN, "not an admin";
    SessionUnreadable = "session_unreadable", INTERNAL_SERVER_ERROR, "the session could not be read";
    SessionNotOpened = "session_not_opened", INTERNAL_SERVER_ERROR, "the session could not be opened";
    SessionNotEnded = "session_not_ended", INTERNAL_SERVER_ERROR, "the session could not be ended";
    LinkUnchecked = "link_unchecked", INTERNAL_SERVER_ERROR, "the link could not be checked";
    RequestFailed = "request_failed", INTERNAL_SERVER_ERROR, "the request could not be handled";

    // ------------------------------------------------------ the account
    AccountNotCreated = "account_not_created", INTERNAL_SERVER_ERROR, "the account could not be created";
    AccountUnreadable = "account_unreadable", INTERNAL_SERVER_ERROR, "the account could not be read";
    AccountNotExported = "account_not_exported", INTERNAL_SERVER_ERROR, "the account could not be exported";
    AccountNotDeleted = "account_not_deleted", INTERNAL_SERVER_ERROR, "the account could not be deleted";
    AddressNotConfirmed = "address_not_confirmed", INTERNAL_SERVER_ERROR, "the address could not be confirmed";
    PasswordNotChanged = "password_not_changed", INTERNAL_SERVER_ERROR, "the password could not be changed";
    ProfileUnreadable = "profile_unreadable", INTERNAL_SERVER_ERROR, "the profile could not be read";
    ProfileNotSaved = "profile_not_saved", INTERNAL_SERVER_ERROR, "the profile could not be saved";
    ModeChosen = "mode_chosen", CONFLICT, "the mode has been chosen, and it stays";
    ModeNotChosen = "mode_not_chosen", INTERNAL_SERVER_ERROR, "the mode could not be chosen";
    HaloShapeUnknown = "halo_shape_unknown", BAD_REQUEST, "that marker shape is not one of ours";
    HaloColourUnknown = "halo_colour_unknown", BAD_REQUEST, "that marker colour is not one of ours";
    CameraOffMap = "camera_off_map", BAD_REQUEST, "that is not a place on the map";
    BornOutOfRange = "born_out_of_range", BAD_REQUEST, "that is not a year someone listening now was born in";

    // ------------------------------------------------------- the letters
    LetterNotSent = "letter_not_sent", INTERNAL_SERVER_ERROR, "the letter could not be sent";
    LetterLanguageUnknown = "letter_language_unknown", BAD_REQUEST, "the letter is not written in that language";
    /// The weekly letter goes only to an address shown to be one's own.
    AddressUnconfirmed = "address_unconfirmed", FORBIDDEN, "confirm your address first - the letter goes only to an address you have shown is yours";
    LetterNotAskedFor = "letter_not_asked_for", INTERNAL_SERVER_ERROR, "the letter could not be asked for";
    LetterNotStopped = "letter_not_stopped", INTERNAL_SERVER_ERROR, "the letter could not be stopped";
    StopLinkCut = "stop_link_cut", BAD_REQUEST, "that link was cut short - open it from the letter again";

    // --------------------------------------------------------- the canon
    NoSuchArtist = "no_such_artist", NOT_FOUND, "no such artist";
    NoSuchLabel = "no_such_label", NOT_FOUND, "no such label";
    NoSuchScene = "no_such_scene", NOT_FOUND, "no such scene";
    NoSuchGenre = "no_such_genre", NOT_FOUND, "no such genre or style";
    NoSuchMechanic = "no_such_mechanic", NOT_FOUND, "not a mechanic we count";
    CanonUnreadable = "canon_unreadable", INTERNAL_SERVER_ERROR, "the canon could not be read";
    CanonUnsearchable = "canon_unsearchable", INTERNAL_SERVER_ERROR, "the canon could not be searched";
    /// Carries `max`, the longest list of stars taken at once.
    IdsMalformed = "ids_malformed", BAD_REQUEST, format!("ids must be 1 to {} numbers separated by commas", crate::api::artists::MAX_STARS);
    SameStar = "same_star", BAD_REQUEST, "a star compared with itself has nothing to show";
    DayMalformed = "day_malformed", BAD_REQUEST, "day must be a date written YYYY-MM-DD";
    YouthUnknown = "youth_unknown", BAD_REQUEST, "that youth is not one the canon can answer for";
    YouthUnreadable = "youth_unreadable", INTERNAL_SERVER_ERROR, "the stars of that youth could not be read";

    // ------------------------------------------------------- the sky
    SkyUnreadable = "sky_unreadable", INTERNAL_SERVER_ERROR, "the sky could not be read";
    YourSkyUnreadable = "your_sky_unreadable", INTERNAL_SERVER_ERROR, "your sky could not be read";
    ChangesUnreadable = "changes_unreadable", INTERNAL_SERVER_ERROR, "what changed in the sky could not be read";
    TooManyStars = "too_many_stars", BAD_REQUEST, "too many stars at once";
    StarNotOpened = "star_not_opened", INTERNAL_SERVER_ERROR, "the star could not be opened";
    StarsNotBrought = "stars_not_brought", INTERNAL_SERVER_ERROR, "the stars could not be brought";

    // ---------------------------------------------------- the expedition
    NotOnSky = "not_on_sky", BAD_REQUEST, "that artist is not a star on this sky";
    NotExploring = "not_exploring", CONFLICT, "only the exploration mode sets out on an expedition";
    ExpeditionBegun = "expedition_begun", CONFLICT, "the expedition has begun, from the star it began at";
    ExpeditionNotBegun = "expedition_not_begun", INTERNAL_SERVER_ERROR, "the expedition could not begin";
    ExpeditionUnreadable = "expedition_unreadable", INTERNAL_SERVER_ERROR, "the expedition could not be read";
    /// It did begin: what failed is reading it back for the answer.
    ExpeditionNotReadBack = "expedition_not_read_back", INTERNAL_SERVER_ERROR, "the expedition began, and could not be read back";

    // ------------------------------------------------------ listening
    ListeningUnreadable = "listening_unreadable", INTERNAL_SERVER_ERROR, "the listening could not be read";
    ListenBrainzTokenMalformed = "listenbrainz_token_malformed", BAD_REQUEST, "that does not look like a ListenBrainz token";
    ListenBrainzTokenUnknown = "listenbrainz_token_unknown", BAD_REQUEST, "ListenBrainz does not know that token";
    ListenBrainzThrottled = "listenbrainz_throttled", TOO_MANY_REQUESTS, "ListenBrainz asked us to wait - try again in a minute";
    ListenBrainzUnreachable = "listenbrainz_unreachable", BAD_GATEWAY, "ListenBrainz did not answer - try again in a minute";
    ListenBrainzTaken = "listenbrainz_taken", CONFLICT, "that ListenBrainz account is already linked to another lyrid account";
    ListenBrainzNotLinked = "listenbrainz_not_linked", BAD_REQUEST, "no ListenBrainz account is linked";
    ListenBrainzNotLinkedNow = "listenbrainz_not_linked_now", INTERNAL_SERVER_ERROR, "the account could not be linked";
    ListenBrainzNotUnlinked = "listenbrainz_not_unlinked", INTERNAL_SERVER_ERROR, "the account could not be unlinked";
    ReadTooSoon = "read_too_soon", TOO_MANY_REQUESTS, "read a moment ago - try again in a minute";
    HistoryBeingRead = "history_being_read", CONFLICT, "your history is still being read - it carries on by itself";
    LastfmNotCsv = "lastfm_not_csv", BAD_REQUEST, "that file is not a CSV export";
    LastfmNoColumns = "lastfm_no_columns", BAD_REQUEST, "that file has no column with a time or none with an artist";
    LastfmNotRead = "lastfm_not_read", INTERNAL_SERVER_ERROR, "the file could not be read in";
    LastfmNotForgotten = "lastfm_not_forgotten", INTERNAL_SERVER_ERROR, "the file's history could not be forgotten";

    // ------------------------------------------------------- the stand
    StandUnreadable = "stand_unreadable", INTERNAL_SERVER_ERROR, "the stand's state could not be read";
}

impl Refusal {
    /// The numbers a refusal needs to be said, by the names the catalogue
    /// interpolates them under.
    fn values(self) -> Map<String, Value> {
        let mut values = Map::new();
        match self {
            Self::PasswordTooShort => {
                values.insert("min".to_string(), Value::from(crate::auth::MIN_PASSWORD));
            }
            Self::IdsMalformed => {
                values.insert("max".to_string(), Value::from(crate::api::artists::MAX_STARS));
            }
            _ => {}
        }
        values
    }

    /// The body every refusal answers with.
    fn body(self, phrase: &str) -> Value {
        let mut body = self.values();
        body.insert("code".to_string(), Value::from(self.code()));
        body.insert("error".to_string(), Value::from(phrase));
        Value::Object(body)
    }
}

impl From<crate::auth::Unfit> for Refusal {
    fn from(unfit: crate::auth::Unfit) -> Self {
        match unfit {
            crate::auth::Unfit::Email => Self::EmailMalformed,
            crate::auth::Unfit::Password => Self::PasswordTooShort,
        }
    }
}

impl From<crate::expedition::Refusal> for Refusal {
    fn from(declined: crate::expedition::Refusal) -> Self {
        match declined {
            crate::expedition::Refusal::NotOnSky => Self::NotOnSky,
            crate::expedition::Refusal::NotExploring => Self::NotExploring,
            crate::expedition::Refusal::Begun => Self::ExpeditionBegun,
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status(), Json(self.body(&self.phrase()))).into_response()
    }
}

/// Gives a code to what the framework refused before a handler ran.
///
/// A handler always refuses with a [`Refusal`], which is JSON; anything else
/// that leaves the API as a failure was written by axum itself -- an extractor
/// that could not read the body, the query or the path, a method a route does
/// not take, a body over its limit -- in plain text and with no code. It is
/// rewritten into the same shape, keeping its status, its `Allow` header and
/// its words (which name the field that was wrong, and are worth more to
/// whoever sent the request than a phrase of ours).
pub(crate) async fn coded(response: Response) -> Response {
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error()) || is_json(&response) {
        return response;
    }

    let refusal = match status {
        StatusCode::NOT_FOUND => Refusal::NoSuchRoute,
        StatusCode::METHOD_NOT_ALLOWED => Refusal::MethodNotAllowed,
        StatusCode::PAYLOAD_TOO_LARGE => Refusal::BodyTooLarge,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => Refusal::UnsupportedMediaType,
        status if status.is_client_error() => Refusal::MalformedRequest,
        _ => Refusal::ServerFailed,
    };

    let (parts, body) = response.into_parts();
    // A rejection's text is a line or two; anything past this is not one.
    let said = axum::body::to_bytes(body, 16 * 1024)
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
        .unwrap_or_default();
    let phrase = if said.is_empty() { refusal.phrase() } else { Cow::from(said) };

    let mut answer = (status, Json(refusal.body(&phrase))).into_response();
    if let Some(allow) = parts.headers.get(header::ALLOW) {
        answer.headers_mut().insert(header::ALLOW, allow.clone());
    }
    answer
}

fn is_json(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use axum::{Router, middleware};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn body_of(response: Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    fn read(path: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{} could not be read: {error}", path.display()))
    }

    /// The catalogue's `errors` section of one locale, code by message.
    fn catalogue(locale: &str) -> BTreeMap<String, String> {
        let locale: Value = serde_json::from_str(&read(&format!("web/src/i18n/locales/{locale}.json"))).unwrap();
        locale
            .get("errors")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("the catalogue has no `errors` section"))
            .iter()
            .map(|(code, message)| (code.clone(), message.as_str().unwrap_or_default().to_string()))
            .collect()
    }

    /// `{{min}}` and `{{max}}`: what a message asks the refusal to carry.
    fn placeholders(message: &str) -> BTreeSet<String> {
        message
            .split("{{")
            .skip(1)
            .filter_map(|rest| rest.split_once("}}"))
            .map(|(name, _)| name.trim().to_string())
            .collect()
    }

    #[test]
    fn every_code_is_one_refusal_and_written_the_way_a_key_can_be() {
        let mut seen = BTreeSet::new();
        for refusal in Refusal::ALL {
            let code = refusal.code();
            assert!(seen.insert(code), "{code} names two refusals");
            assert!(
                !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{code} should be snake_case: it is a key in the catalogue"
            );
            assert!(!refusal.phrase().is_empty(), "{code} says nothing");
        }
    }

    #[test]
    fn every_refusal_has_words_in_the_catalogue_and_every_word_a_refusal() {
        // The interface finds its words by the code. A code with no message
        // reaches a reader as the server's English; a message with no code is
        // a rename that landed on one side. The other locales are held to
        // this one by `web/tools/check-locales.mjs`.
        let catalogue = catalogue("en");
        let codes: BTreeSet<String> = Refusal::ALL.iter().map(|refusal| refusal.code().to_string()).collect();
        let keys: BTreeSet<String> = catalogue.keys().cloned().collect();
        let unsaid: Vec<_> = codes.difference(&keys).collect();
        let unknown: Vec<_> = keys.difference(&codes).collect();
        assert!(unsaid.is_empty(), "refusals with no message under `errors` in en.json: {unsaid:?}");
        assert!(unknown.is_empty(), "messages under `errors` in en.json that no refusal sends: {unknown:?}");
    }

    #[test]
    fn a_message_asks_only_for_the_numbers_its_refusal_carries() {
        // `{{min}}` with nothing called `min` in the body renders as an empty
        // gap in a sentence; a number carried and never said is a promise the
        // documentation makes for nothing.
        let catalogue = catalogue("en");
        for refusal in Refusal::ALL {
            let wanted = placeholders(&catalogue[refusal.code()]);
            let carried: BTreeSet<String> = refusal.values().keys().cloned().collect();
            assert_eq!(wanted, carried, "{} asks for {wanted:?} and carries {carried:?}", refusal.code());
            assert!(
                !carried.contains("code") && !carried.contains("error"),
                "{} would overwrite its own code or phrase",
                refusal.code()
            );
        }
    }

    #[test]
    fn every_code_is_documented() {
        // The codes are a contract with whoever calls the API, and a contract
        // nobody can read is not one.
        let page = read("docs/site/src/content/docs/reference/refusals.md");
        let missing: Vec<_> = Refusal::ALL
            .iter()
            .map(|refusal| refusal.code())
            .filter(|code| !page.contains(&format!("| `{code}` |")))
            .collect();
        assert!(missing.is_empty(), "codes missing from the refusals reference: {missing:?}");
    }

    #[tokio::test]
    async fn a_refusal_answers_its_status_its_code_and_its_words() {
        let response = Refusal::TokenInvalid.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_of(response).await;
        assert_eq!(body["code"], "token_invalid");
        assert_eq!(body["error"], "that link is not valid any more - ask for a new one");
    }

    #[tokio::test]
    async fn a_refusal_that_needs_a_number_carries_it() {
        let body = body_of(Refusal::PasswordTooShort.into_response()).await;
        assert_eq!(body["code"], "password_too_short");
        assert_eq!(body["min"], crate::auth::MIN_PASSWORD);
        assert!(body["error"].as_str().unwrap().contains(&crate::auth::MIN_PASSWORD.to_string()));
    }

    fn api() -> Router {
        #[derive(serde::Deserialize)]
        struct Named {
            #[allow(dead_code, reason = "only its absence is tested")]
            name: String,
        }
        Router::new()
            .route("/api/named", post(|axum::Json(_): axum::Json<Named>| async { "ok" }))
            .route("/api/number/{n}", get(|axum::extract::Path(_): axum::extract::Path<i32>| async { "ok" }))
            .route("/api/refused", get(|| async { Refusal::NotSignedIn }))
            .route("/api/{*rest}", axum::routing::any(|| async { Refusal::NoSuchRoute }))
            .layer(middleware::map_response(coded))
    }

    async fn ask(request: Request<Body>) -> (StatusCode, Value, Option<String>) {
        let response = api().oneshot(request).await.unwrap();
        let status = response.status();
        let allow = response.headers().get(header::ALLOW).map(|value| value.to_str().unwrap().to_string());
        (status, body_of(response).await, allow)
    }

    #[tokio::test]
    async fn what_the_framework_refuses_carries_a_code_too() {
        // Missing field: axum's 422, in its own words, which name the field.
        let (status, body, _) = ask(Request::post("/api/named")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap())
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "malformed_request");
        assert!(body["error"].as_str().unwrap().contains("name"), "{body}");

        // Not JSON at all, and no content type.
        let (status, body, _) = ask(Request::post("/api/named").body(Body::from("name=ada")).unwrap()).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(body["code"], "unsupported_media_type");

        // A path segment that is not a number.
        let (status, body, _) = ask(Request::get("/api/number/seven").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "malformed_request");

        // A method the route does not take: the `Allow` header survives.
        let (status, body, allow) = ask(Request::delete("/api/named").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(body["code"], "method_not_allowed");
        assert_eq!(body["error"], "that route does not take that method");
        assert!(allow.is_some_and(|allow| allow.contains("POST")));

        // No such route.
        let (status, body, _) = ask(Request::get("/api/nothing/here").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "no_such_route");
    }

    #[tokio::test]
    async fn a_refusal_of_our_own_passes_the_layer_untouched() {
        let (status, body, _) = ask(Request::get("/api/refused").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body, serde_json::json!({"code": "not_signed_in", "error": "not signed in"}));
    }
}
