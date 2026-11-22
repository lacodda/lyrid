---
title: Refusals
description: How the API says no — the shape every failure answers with, the stable codes, and how the interface turns a code into the reader's language.
---

Every failure the API answers — a refusal of the request, or a fault on the server — has one shape:

```json
{ "code": "token_invalid", "error": "that link is not valid any more - ask for a new one" }
```

- **`code`** is the contract. It is stable: once a code has shipped it keeps its name and its meaning, and a client can branch on it. It is `snake_case`, and the list below is all of them.
- **`error`** is English, for a person reading a log or a `curl`. It can be reworded between versions; nothing should branch on it.
- A refusal that needs a number to be said carries it beside the two, under the name its message uses: `password_too_short` carries `min`, the shortest password taken, and `ids_malformed` carries `max`, the longest list of stars.

The status says what kind of failure it is; the code says which one. A client that meets a code it does not know can still read the status, and show `error`.

## How the interface says it

The interface does not show `error`. It looks the code up in its own catalogue, `errors.<code>` in `web/src/i18n/locales`, and says the message in the reader's language, with the carried numbers put in. The server's English is the fallback for a code the interface does not know yet — a server newer than the page — and a failure with no body at all, such as a proxy's error page, falls back to the screen's own words for what it was doing.

Two checks keep the three in step. A test in the server holds the catalogue to the list of codes in both directions, and each message to the numbers its refusal carries; the build holds every other locale to English, key for key and placeholder for placeholder. The same test holds this page to the list, so a code nobody documented does not compile into a release.

## What the framework refuses

Some requests are refused before any handler reads them: a body that is not JSON or lacks a field, a path segment that is not a number, a method the route does not take, a body over the route's limit. These answer in the same shape, with the status the framework chose and its own words as `error` — they name the field that was wrong, which is worth more to whoever sent the request than a phrase of ours. A `405` keeps its `Allow` header.

An address under `/api` that is not a route answers `404` with `no_such_route`, rather than the interface's page: a client asking for JSON is never handed HTML.

`GET /health` is not a refusal when it answers `503`; it has [its own shape](/lyrid/reference/health/). The tiles are files, and a tile that is not there is a bare `404`.

## The codes

### Requests

| Code | Status | Phrase |
| --- | --- | --- |
| `malformed_request` | 400 | the request could not be read |
| `no_such_route` | 404 | there is no such route |
| `method_not_allowed` | 405 | that route does not take that method |
| `body_too_large` | 413 | the body is larger than this route takes |
| `unsupported_media_type` | 415 | the body is not of a type this route reads |
| `server_failed` | 500 | something went wrong on the server |

### Signing in

| Code | Status | Phrase |
| --- | --- | --- |
| `not_signed_in` | 401 | not signed in |
| `credentials_wrong` | 401 | that address and password do not match an account |
| `email_malformed` | 400 | that does not look like an email address |
| `password_too_short` | 400 | a password needs at least 10 characters (carries `min`) |
| `email_taken` | 409 | an account with that address already exists |
| `token_invalid` | 400 | that link is not valid any more - ask for a new one |
| `not_admin` | 403 | not an admin |
| `session_unreadable` | 500 | the session could not be read |
| `session_not_opened` | 500 | the session could not be opened |
| `session_not_ended` | 500 | the session could not be ended |
| `link_unchecked` | 500 | the link could not be checked |
| `request_failed` | 500 | the request could not be handled |

### The account

| Code | Status | Phrase |
| --- | --- | --- |
| `account_not_created` | 500 | the account could not be created |
| `account_unreadable` | 500 | the account could not be read |
| `account_not_exported` | 500 | the account could not be exported |
| `account_not_deleted` | 500 | the account could not be deleted |
| `address_not_confirmed` | 500 | the address could not be confirmed |
| `password_not_changed` | 500 | the password could not be changed |
| `profile_unreadable` | 500 | the profile could not be read |
| `profile_not_saved` | 500 | the profile could not be saved |
| `mode_chosen` | 409 | the mode has been chosen, and it stays |
| `mode_not_chosen` | 500 | the mode could not be chosen |
| `halo_shape_unknown` | 400 | that marker shape is not one of ours |
| `halo_colour_unknown` | 400 | that marker colour is not one of ours |
| `camera_off_map` | 400 | that is not a place on the map |
| `born_out_of_range` | 400 | that is not a year someone listening now was born in |

### The letters

| Code | Status | Phrase |
| --- | --- | --- |
| `letter_not_sent` | 500 | the letter could not be sent |
| `letter_language_unknown` | 400 | the letter is not written in that language |
| `address_unconfirmed` | 403 | confirm your address first - the letter goes only to an address you have shown is yours |
| `letter_not_asked_for` | 500 | the letter could not be asked for |
| `letter_not_stopped` | 500 | the letter could not be stopped |
| `stop_link_cut` | 400 | that link was cut short - open it from the letter again |

### The canon

| Code | Status | Phrase |
| --- | --- | --- |
| `no_such_artist` | 404 | no such artist |
| `no_such_label` | 404 | no such label |
| `no_such_scene` | 404 | no such scene |
| `no_such_genre` | 404 | no such genre or style |
| `no_such_mechanic` | 404 | not a mechanic we count |
| `canon_unreadable` | 500 | the canon could not be read |
| `canon_unsearchable` | 500 | the canon could not be searched |
| `ids_malformed` | 400 | ids must be 1 to 50 numbers separated by commas (carries `max`) |
| `same_star` | 400 | a star compared with itself has nothing to show |
| `day_malformed` | 400 | day must be a date written YYYY-MM-DD |
| `youth_unknown` | 400 | that youth is not one the canon can answer for |
| `youth_unreadable` | 500 | the stars of that youth could not be read |

### The sky

| Code | Status | Phrase |
| --- | --- | --- |
| `sky_unreadable` | 500 | the sky could not be read |
| `your_sky_unreadable` | 500 | your sky could not be read |
| `changes_unreadable` | 500 | what changed in the sky could not be read |
| `too_many_stars` | 400 | too many stars at once |
| `star_not_opened` | 500 | the star could not be opened |
| `stars_not_brought` | 500 | the stars could not be brought |

### The expedition

| Code | Status | Phrase |
| --- | --- | --- |
| `not_on_sky` | 400 | that artist is not a star on this sky |
| `not_exploring` | 409 | only the exploration mode sets out on an expedition |
| `expedition_begun` | 409 | the expedition has begun, from the star it began at |
| `expedition_not_begun` | 500 | the expedition could not begin |
| `expedition_unreadable` | 500 | the expedition could not be read |
| `expedition_not_read_back` | 500 | the expedition began, and could not be read back |

### Listening

| Code | Status | Phrase |
| --- | --- | --- |
| `listening_unreadable` | 500 | the listening could not be read |
| `listenbrainz_token_malformed` | 400 | that does not look like a ListenBrainz token |
| `listenbrainz_token_unknown` | 400 | ListenBrainz does not know that token |
| `listenbrainz_throttled` | 429 | ListenBrainz asked us to wait - try again in a minute |
| `listenbrainz_unreachable` | 502 | ListenBrainz did not answer - try again in a minute |
| `listenbrainz_taken` | 409 | that ListenBrainz account is already linked to another lyrid account |
| `listenbrainz_not_linked` | 400 | no ListenBrainz account is linked |
| `listenbrainz_not_linked_now` | 500 | the account could not be linked |
| `listenbrainz_not_unlinked` | 500 | the account could not be unlinked |
| `read_too_soon` | 429 | read a moment ago - try again in a minute |
| `history_being_read` | 409 | your history is still being read - it carries on by itself |
| `lastfm_not_csv` | 400 | that file is not a CSV export |
| `lastfm_no_columns` | 400 | that file has no column with a time or none with an artist |
| `lastfm_not_read` | 500 | the file could not be read in |
| `lastfm_not_forgotten` | 500 | the file's history could not be forgotten |

### The stand

| Code | Status | Phrase |
| --- | --- | --- |
| `stand_unreadable` | 500 | the stand's state could not be read |
