---
title: Scrobbling
description: Linking ListenBrainz — how real listening anywhere opens stars and gathers light, what is read and when, and what is kept.
---

A preview is a scan and a video is a landing; the real journey is what you listen to anyway — on your phone, in your player, in the car. lyrid reads it from [ListenBrainz](https://listenbrainz.org): link your account once, and from then on every listen opens the stars it names and gathers **light**.

## Linking

Signed in, press **link ListenBrainz** and paste your user token from the [ListenBrainz settings page](https://listenbrainz.org/settings/). lyrid asks ListenBrainz whose token it is, remembers the name, and drops the token — it is never stored. Your listens on ListenBrainz are public and are read by name; the token only proves the account is yours, so nobody can gather light with somebody else's listening.

One ListenBrainz account can feed one lyrid account.

Reading starts from the moment you link. Your history before that is not read yet.

## Openings

A star is **open** from the first listen that names it. The panel under your account counts the stars of the current sky your listening has opened and names the latest ones; **show on the sky** rings every one of them and frames them. A star's card says how many times you have heard it and when first — only to you, and only once you have.

A listen credited to several artists opens each of them. A listen ListenBrainz could not match to anyone still counts, and opens nothing.

## Light

| What you heard | Light per minute |
| --- | --- |
| a star you have heard fewer than ten times before | 5 |
| anything else | 1 |

- **Minutes** are the recording's length, rounded: at least one, at most fifteen. A listen that does not report its length counts as three minutes — about a song.
- A listen credited to several artists pays **once**, at the rate of the newest of them.
- A listen pays when it arrives, under the rule of that day, and keeps what it paid.

The new pays five times the familiar on purpose: the sky is for finding music, and an album by someone you had never heard of is worth more to it than the hundredth play of an old favourite.

Light is gathered whichever mode your account will be in. What it buys arrives with the game.

## When listening is read

- **In the background**, every fifteen minutes, for every linked account.
- **When you ask** — **read now** — at most once a minute.

Each read reaches back two days behind the last one, because players that lose their connection send their listens later with the time they were played. A listen read twice is still one listen: it is paid once, by construction.

ListenBrainz matches a listen to its artists some time after it arrives. A listen with no artists yet, less than a day old, waits for a later read; after a day it is counted as it is.

When ListenBrainz cannot be reached, asks lyrid to wait, or no longer knows the name you linked, the panel says so, and the next read tries again. A read that fails partway keeps what it got.

## What is kept

For each listen: when it was, which artists it was by (their MusicBrainz ids), how long it ran, and the light it gathered. No track or album names. See [what lyrid keeps](/lyrid/reference/accounts/) — the export hands every row back, and deleting the account removes them.

**Unlink** stops the reading. What was gathered stays with the account until the account is deleted.

The decision and its measurements are in [ADR 0019](https://github.com/lacodda/lyrid/blob/main/docs/adr/0019-listening-read-from-listenbrainz.md).
