# 12. Accounts with a password, sessions in the database

Date: 2026-09-01

## Status

Accepted.

## Context

lyrid has been anonymous for eleven versions. The sky, the star card, the
search and the ether all answer the same for everybody, and everything a
visitor chooses — which marker to draw, where the camera sits — lives in the
browser's local storage or in the address bar.

The profiles milestone ends that. It is the first personal data in a database
whose other twenty-two tables are a canon rebuildable from open dumps at any
time. From here on, a table exists that no dump can restore.

Three ways in were considered.

**Third-party sign-in (GitHub, Google).** No passwords to store, no reset flow
to build. But it puts a rate-limited external service on the path of signing
in — the exact shape [ADR 0002](0002-universe-from-open-dumps.md) refuses for
data — it needs application secrets deployed to every stand, and it does not
work at all on a stand with no route to the internet, which is where this
product is tested.

**A handle and a password, with no address at all.** The least personal data
possible. But a forgotten password is then an account lost forever with no
recourse, and the two features queued immediately behind this one — the
privacy charter's export-and-delete, and the ListenBrainz binding — both
assume a way to reach the person.

**An address and a password.** Chosen.

## Decision

**Email and password, hashed with Argon2id; sessions as random tokens in a
database table, carried by an `HttpOnly` cookie.**

- The address is stored case-folded, so one mailbox is one account and the
  `UNIQUE` constraint enforces it rather than every query remembering to.
- The password floor is length (ten characters, counted as characters and not
  as bytes) and nothing else. Composition rules push people towards
  `Password1!`, which is worse than a long phrase.
- Sessions are rows, not signed tokens. Signing out deletes the row, so it
  actually ends the session; a stateless token stays valid until it expires no
  matter what the user pressed. Expiry is checked against the database's
  `now()`, not the server's clock.
- The cookie is `HttpOnly` and `SameSite=Lax` always, and `Secure` only when
  configured — a `Secure` cookie over the stand's plain HTTP is never sent
  back, which breaks sign-in with no error anywhere to read.
- The mode lives on the profile and has no update path, in the schema and in
  the API both. It is a product rule (Vision, principle 5) and a rule the
  client alone keeps is a rule until the first `curl`.
- A saved camera is stored with the layout it was taken in and dropped when
  that layout is no longer the current one. Coordinates are comparable only
  within one layout (see migration 0007): after a rebuild the same numbers
  point at a different star, so restoring them would open the map somewhere
  nobody chose.

## Consequences

- **No external service is on the path of signing in.** A stand with no
  internet route still has working accounts, and there are no application
  secrets to deploy alongside one.
- **The password is ours to keep safe.** Argon2id with per-password salts is
  the mitigation; a corrupted hash fails to verify rather than matching
  anything, including an empty password.
- **A forgotten password cannot yet be reset**, because there is no mail.
  This is a real gap and it closes with the privacy charter (v0.11), which
  needs mail for confirmation and deletion anyway. Until then an address is an
  identifier, not something the server has proven the user owns.
- **The two failures of signing in are indistinguishable**, deliberately:
  telling "no such address" from "wrong password" turns the form into a way of
  asking whether a person has an account here. Registration is the exception —
  it must say an address is taken, and trying to register already reveals it.
- **Anonymous browsing keeps working, and that is load-bearing.** The public
  read-only sky is a version of its own (v0.11), and the game's fog only makes
  sense against a mode that has none. An account is memory added, never a gate.
- **The stand now holds data no dump can rebuild.** The restore command
  already refuses to overwrite a non-empty database without `--force` — a rule
  taken on 2026-08-27, before it was needed, and this is the version that
  makes it needed.
