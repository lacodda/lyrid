# 30. Refusals carry a code

Date: 2026-11-22

## Status

Accepted. Changes the refusal body described in
[ADR 0012](0012-accounts-with-a-password.md) and the interface's reading of it.

## Context

Since the first account the API has said no in a sentence: `{"error": "that link
is not valid any more - ask for a new one"}`, written for a person, and the
interface showed it as it came. That was deliberate -- one set of words, no
second classification of failures in the browser to drift from the server's.

Since v0.12 the interface speaks Russian as well as English, and the sentence
became a seam: a Russian screen with an English line in red under the form.
Translating the sentence in the browser would mean matching on English prose,
which is the second classification by another name, and it breaks on the first
rewording.

The refusals themselves were written by hand at about ninety places -- a
`json!({"error": ...})` here, a helper taking a message there, a status picked
at each site -- and nothing held them to anything. Axum's own rejections (a body
that is not JSON, a missing field, a wrong method) answered in plain text, and an
address under `/api` that was no route fell through to the SPA and came back as
`index.html` with a `200`.

## Decision

**Every refusal is a variant of one enum, `api::refusal::Refusal`**, written as a
table: the variant, its code on the wire, its status, its English phrase. The
body is `{"code": "...", "error": "...", ...values}`.

- **The code is the contract.** It is spelt out in the table rather than derived
  from the variant's name, so renaming Rust cannot rename the wire. It is stable
  once shipped; the phrase is not, and nothing should branch on it.
- **The phrase stays on the server**, for logs, `curl` and a client older than the
  server, which falls back to it for a code it does not know.
- **Numbers travel beside it.** A refusal that needs one to be said
  (`password_too_short` with `min`, `ids_malformed` with `max`) carries it, so the
  catalogue interpolates it rather than repeating the constant.
- **There is no other way to refuse.** The helpers that took a message are gone;
  a handler returns a variant, and domain refusals (`auth::Unfit`,
  `expedition::Refusal`) convert into one by `From`, exhaustively.
- **One code per thing the reader can be told, not per thing the server knows.**
  An expired, a spent and a never-issued link are one code, `token_invalid`, as
  they were one sentence: telling them apart is what the single answer exists to
  avoid. A wrong password and an unknown address stay one code,
  `credentials_wrong`.
- **The framework's refusals are coded by one layer** (`refusal::coded`, a
  `map_response` over the API router): a failure that leaves the API as anything
  but JSON was written by axum, and is rewritten into the same shape, keeping its
  status, its `Allow` header and its words, which name the field that was wrong.
  `/api` and `/api/{*rest}` answer `no_such_route` before the SPA fallback can.

The interface reads a refusal into a `Refusal` error (`web/src/refusal.ts`) and
says it with `explain`: the catalogue's `errors.<code>` in the reader's language
if it knows the code, else the server's phrase, else the screen's own fallback
for a failure that brought no body at all.

## Alternatives

- **A code added beside each hand-written `json!`.** The smallest change, and it
  leaves ninety places to forget the code at, and no list for anything to be
  checked against.
- **Wrapper extractors for `Json`, `Query` and `Path`** with a coded rejection.
  Precise, but a handler that used axum's own extractor would slip past it, and it
  does not reach a wrong method or a body over its limit. The layer covers every
  extractor there is, and every one added later.
- **Translating on the server**, by `Accept-Language`. The server would need a
  second catalogue of the interface's words, and every non-browser client would
  get prose in a language it did not ask for instead of a code it can branch on.

## Consequences

- The catalogue, the codes and the reference page are held to each other by tests
  in `refusal.rs`: every code has a message in `en.json` and every message a code,
  each message asks only for the numbers its refusal carries, and every code is in
  `docs/site/src/content/docs/reference/refusals.md`. The locale check holds
  Russian to English as before.
- A new refusal is a row, a message in each locale and a line in the reference;
  the tests say which is missing.
- The body grew a field and lost nothing: a client reading `error` still reads it.
