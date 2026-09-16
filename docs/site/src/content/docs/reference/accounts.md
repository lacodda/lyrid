---
title: Accounts
description: Registration, sessions and the profile — what an account holds, what it deliberately does not, and why the mode cannot be changed.
---

An account adds memory to the sky and takes nothing away from it. The map, the star card and the search never ask who is asking: browsing anonymously is a supported state, not a trial. What signing in buys today is that your mode, your last view and your marker follow you to another machine.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/auth/register` | Create an account and sign in. Body: `email`, `password`. |
| `POST` | `/api/auth/login` | Sign in. Body: `email`, `password`. |
| `POST` | `/api/auth/logout` | End the session. |
| `POST` | `/api/auth/confirm` | Confirm an address. Body: `token`. |
| `POST` | `/api/auth/confirm/resend` | Send the confirmation letter again. Needs a session, no body. |
| `POST` | `/api/auth/forgot` | Start a password reset. Body: `email`. |
| `POST` | `/api/auth/reset` | Finish a password reset. Body: `token`, `password`. |
| `GET` | `/api/me` | The signed-in profile, or `401` when nobody is. |
| `PATCH` | `/api/me` | Save part of the profile. |
| `DELETE` | `/api/me` | Destroy the account and everything it holds. |
| `GET` | `/api/me/export` | Everything held about the account, as a JSON file. |

All answer JSON. A refusal carries `{"error": "..."}` written for a person to read, and the client shows those words as they are rather than substituting its own.

### The profile

```json
{
  "id": 1,
  "email": "ada@example.com",
  "mode": "create",
  "halo_shape": "ring",
  "halo_colour": "azure",
  "camera": { "x": -59.17, "y": -69.55, "scale": 12.0 },
  "email_confirmed": true
}
```

`PATCH /api/me` accepts `halo_shape`, `halo_colour` and `camera`, each optional. Only the fields present are written, so saving a camera does not clear a marker the client never touched. It returns the whole profile as it now stands.

## The mode is chosen once — and only creative exists yet

`POST /api/auth/register` takes `email` and `password`. Nothing is asked about a mode any more: the field is absent from the struct the body deserialises into, so a request that sends one anyway (an old build, or someone hoping to pick ahead) sends something nothing reads. Every new profile lands in the creative mode, through the column's own default rather than a value the handler names.

The reason (owner decision, 2026-09-11): only the creative mode is built. The exploration mode arrives with the fog in v0.22, and a choice between a game and a description of a game is not a choice. [ADR 0012](https://github.com/lacodda/lyrid/blob/main/docs/adr/0012-accounts-with-a-password.md)'s rule that the mode is chosen once and never changed afterwards is unchanged — it simply has not started yet. It begins when there is something to choose between, and will then be offered once to every profile that predates it.

`PATCH /api/me` still has no `mode` field: a request carrying one is not refused, it simply has nothing to write, because the struct it parses into has no place to put it. There is still no route, anywhere, that writes a mode after the row is created.

This is a product rule — [why the choice is permanent](/lyrid/concepts/two-modes/) — and it is enforced on the server rather than in the interface. A rule the client alone keeps is a rule until the first `curl`.

## Sessions

A session is a random 256-bit token stored in the database and sent as a cookie:

- **`HttpOnly`** — no script can read the token, so an XSS hole cannot leak it. The consequence is that the page cannot tell whether it is signed in by looking at storage; it asks `/api/me`, and a `401` is the ordinary answer for a visitor rather than an error.
- **`SameSite=Lax`** — a link from elsewhere arrives signed in, a cross-site form post does not carry the session.
- **`Secure`** — only when [`LYRID_SECURE_COOKIE`](/lyrid/reference/configuration/) says so, because a `Secure` cookie over the stand's plain HTTP is never sent at all.
- **30 days**, checked against the database's own `now()` rather than the server's clock.

Sessions live in a table rather than in a signed token so that signing out actually ends one. A stateless token stays valid until it expires no matter what the user pressed; here the row is deleted, and the next request with that token is a `401`.

Passwords are stored as Argon2id PHC strings with a random per-password salt. A stored hash that has been corrupted into nonsense fails to verify rather than matching anything — including an empty password.

## Confirming an address

Registration issues a confirmation token in the same database transaction as the account — an account without one is an account that can never be confirmed and can never be told why — and mails a link, `{LYRID_PUBLIC_URL}/confirm?token=...`, after the transaction commits. Sending inside the transaction would hold a connection open across a round trip to a mail server, which is how one slow SMTP host empties the pool. A letter that fails to send does not undo the registration: the account exists either way, and there is a button to ask for the letter again.

`POST /api/auth/confirm` with `{token}` spends it and confirms the address. It answers `{"status":"confirmed"}`, or `400` with `{"error":"that link is not valid any more - ask for a new one"}` for a token that is expired, already used, or was never issued — one answer for all three, because none of them is the visitor's business: a link that says "this was already used" is a link that confirms a guess.

`POST /api/auth/confirm/resend` needs a session and takes no body. It sends the letter again and answers `{"status":"sent"}`, or `{"status":"already confirmed"}` if there is nothing left to do. It sits behind the session rather than behind an address typed into a form, because a form taking an address would let anyone post letters to anyone from this server.

Confirmation links last 24 hours, and spending one is a single `UPDATE ... WHERE used_at IS NULL`, so two clicks of the same link cannot both succeed — a read followed by a write would let both pass the read.

**Confirmation gates nothing except a password reset.** Signing in works and the sky opens, all before a single letter is answered. The only thing an unconfirmed address cannot do is receive a reset link, because a reset hands over an account to whoever reads a mailbox, and that has to be a mailbox someone has proven they can read.

## Resetting a password

`POST /api/auth/forgot` with `{email}` always answers `200` with `{"status":"sent if we know the address"}` — the same words whether or not the address has an account, so the form cannot be used to ask whether someone is a user here. Only a confirmed address actually receives a letter; an address nobody has proven ownership of is exactly the hole confirmation exists to close.

`POST /api/auth/reset` with `{token, password}` finishes it. The password is checked against the same ten-character floor as registration *before* the token is spent, so a token is never burnt on a password the server was going to refuse anyway — that would leave someone with a dead link and a typo. On success the server changes the password, deletes every session the account has open, retires any other live reset tokens, and clears the caller's own cookie. Ending every session is deliberate: a reset is what someone does when they believe their password is known to somebody else, and leaving that somebody else signed in answers only half the problem.

Reset links last 1 hour, shorter than confirmation links, because a reset link is a way into the account and not just a way of proving an address.

## The privacy charter: export and delete

`GET /api/me/export` needs a session and returns the whole account as a JSON file, sent with `Content-Disposition: attachment` so a browser saves it rather than displaying it — the point is to hand the data over, not show it. It holds `exported_at`, a short `note`, `account` (id, email, when it was confirmed, when it was created), `profile` (mode, marker shape and colour, the camera and the layout key it belongs to) and `sessions` (only their creation and expiry times). Session tokens are left out on purpose: a token is a live credential, and an export is a file that gets mailed around and left in a downloads folder — handing over working keys to the account is not the same as handing over the data about it.

`DELETE /api/me` needs a session and destroys the account immediately and completely: one `DELETE FROM app_user`, and the profile, sessions and tokens go with it through `ON DELETE CASCADE`. Not a flag, not a queue, not a promise to remove it within 30 days — a promise kept by a background job is a promise the user cannot check.

The usage counters described [below](/lyrid/reference/api/#usage-metrics) are untouched by a deletion, and correctly so: they hold no row about that person to delete.

The web page for both is at `/charter`.

## What the two failures of signing in have in common

A wrong password and an address with no account give the same answer, in the same words:

```json
{ "error": "that address and password do not match an account" }
```

Telling them apart would turn the sign-in form into a way of asking whether a particular person has an account here, which is a fact about them and not ours to publish.

Registration is different and says plainly that an address is taken (`409`). There is no way to offer an account and hide that the address already has one, and the address is discoverable by trying to register with it regardless.

## The camera and the sky it belongs to

A saved camera is stored with the layout it was taken in. Coordinates are only comparable within one layout — rebuilding the sky moves every star — so a camera from an older layout is not stale, it is meaningless: the same numbers point somewhere else entirely.

So `GET /api/me` returns `camera: null` when what is stored belongs to a layout the sky no longer shows. The rest of the profile comes back as normal; only the camera is dropped.

A link wins over a saved camera. When the address carries a view (`#x,y,scale`) or a star (`/star/54`), that is where the sky opens: a fragment was put there by whoever sent the link, while the saved camera is only where you happened to stop last time.

The camera is not saved on every frame. The client sends one only when the view has really moved — more than half a screen at the current zoom, or a change of zoom by a quarter — measured against what the account already holds rather than against the previous frame, so a slow drift still eventually crosses the line.

## What is deliberately absent

- **No exploration mode to choose.** Only the creative mode is built; the choice arrives with the fog in v0.22 and is offered once, then, to every profile that predates it.
- **No third-party sign-in.** It would put a rate-limited external service in the path of signing in, against the spirit of [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md), and would not work on a stand with no route to the internet.
- **Nothing from the game.** Fog, light, contracts and the journal each bring their own tables when their versions arrive. The profile holds a mode, a marker and a camera, and no columns are added ahead of the features that need them.
