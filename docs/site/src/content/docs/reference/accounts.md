---
title: Accounts
description: Registration, sessions and the profile — what an account holds, what it deliberately does not, and why the mode cannot be changed.
---

An account adds memory to the sky and takes nothing away from it. The map, the star card and the search never ask who is asking: browsing anonymously is a supported state, not a trial. What signing in buys today is that your mode, your last view and your marker follow you to another machine.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/auth/register` | Create an account and sign in. Body: `email`, `password`, `mode`. |
| `POST` | `/api/auth/login` | Sign in. Body: `email`, `password`. |
| `POST` | `/api/auth/logout` | End the session. |
| `GET` | `/api/me` | The signed-in profile, or `401` when nobody is. |
| `PATCH` | `/api/me` | Save part of the profile. |

All five answer JSON. A refusal carries `{"error": "..."}` written for a person to read, and the client shows those words as they are rather than substituting its own.

### The profile

```json
{
  "id": 1,
  "email": "ada@example.com",
  "mode": "create",
  "halo_shape": "ring",
  "halo_colour": "azure",
  "camera": { "x": -59.17, "y": -69.55, "scale": 12.0 }
}
```

`PATCH /api/me` accepts `halo_shape`, `halo_colour` and `camera`, each optional. Only the fields present are written, so saving a camera does not clear a marker the client never touched. It returns the whole profile as it now stands.

## The mode is chosen once

The mode is set when the account is created and there is no route that changes it afterwards. `PATCH /api/me` has no `mode` field: a request carrying one is not refused, it simply has nothing to write, because the struct it parses into has no place to put it.

This is a product rule — [why the choice is permanent](/lyrid/concepts/two-modes/) — and it is enforced on the server rather than in the interface. A rule the client alone keeps is a rule until the first `curl`.

## Sessions

A session is a random 256-bit token stored in the database and sent as a cookie:

- **`HttpOnly`** — no script can read the token, so an XSS hole cannot leak it. The consequence is that the page cannot tell whether it is signed in by looking at storage; it asks `/api/me`, and a `401` is the ordinary answer for a visitor rather than an error.
- **`SameSite=Lax`** — a link from elsewhere arrives signed in, a cross-site form post does not carry the session.
- **`Secure`** — only when [`LYRID_SECURE_COOKIE`](/lyrid/reference/configuration/) says so, because a `Secure` cookie over the stand's plain HTTP is never sent at all.
- **30 days**, checked against the database's own `now()` rather than the server's clock.

Sessions live in a table rather than in a signed token so that signing out actually ends one. A stateless token stays valid until it expires no matter what the user pressed; here the row is deleted, and the next request with that token is a `401`.

Passwords are stored as Argon2id PHC strings with a random per-password salt. A stored hash that has been corrupted into nonsense fails to verify rather than matching anything — including an empty password.

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

- **No email confirmation and no password reset.** Both need mail, which arrives with the privacy charter. Until then the address is an identifier, not something the server has proven you own.
- **No account deletion or export yet.** Also part of the privacy charter, and deliberately built before anything personal beyond a profile is collected.
- **No third-party sign-in.** It would put a rate-limited external service in the path of signing in, against the spirit of [ADR 0002](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md), and would not work on a stand with no route to the internet.
- **Nothing from the game.** Fog, light, contracts and the journal each bring their own tables when their versions arrive. The profile holds a mode, a marker and a camera, and no columns are added ahead of the features that need them.
