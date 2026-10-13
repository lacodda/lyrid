---
title: Scrobbling
description: Linking ListenBrainz — how real listening anywhere opens stars and gathers light, how your whole history lights the sky, what is read and when, and what is kept.
---

A preview is a scan and a video is a landing; the real journey is what you listen to anyway — on your phone, in your player, in the car. lyrid reads it from [ListenBrainz](https://listenbrainz.org): link your account once, and from then on every listen opens the stars it names and gathers **light**.

## Linking

Signed in, press **link ListenBrainz** and paste your user token from the [ListenBrainz settings page](https://listenbrainz.org/settings/). lyrid asks ListenBrainz whose token it is, remembers the name, and drops the token — it is never stored. Your listens on ListenBrainz are public and are read by name; the token only proves the account is yours, so nobody can gather light with somebody else's listening.

One ListenBrainz account can feed one lyrid account.

## Your history

The first thing lyrid reads is everything before the link: from your first listen on ListenBrainz forwards, so the sky lights up with the whole of what you have heard. The panel says how far it has got — *reading the history: up to March 2014* — and fresh listening is read once the history has caught up with the moment you linked. A long history takes a while: the account the reading was measured on holds twenty years and 220,000 listens.

**The past opens stars and pays no light.** Light is gathered by listening from the moment you link; your history makes stars familiar, so a star you played five hundred times in 2015 pays as familiar on your first listen after the link. Everyone starts with the same light, however long they have been scrobbling — and a history submitted after the fact, real or not, pays nothing.

A Last.fm history can be brought as a file too: see [your life on the sky](/lyrid/concepts/your-life-on-the-sky/).

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
- A listen is priced as of its own moment: one that arrives late counts what you had heard before it was played, not what you heard after.
- Only listening heard while the account is linked pays. History and files pay nothing.

The new pays five times the familiar on purpose: the sky is for finding music, and an album by someone you had never heard of is worth more to it than the hundredth play of an old favourite.

Light is gathered whichever mode your account will be in. What it buys arrives with the game.

## When listening is read

- **In the background**, every fifteen minutes, for every linked account — and back to back while a history is being read.
- **When you ask** — **read now** — at most once a minute, once the history is in.

Each read reaches back two days behind the last one, because players that lose their connection send their listens later with the time they were played. A listen read twice is still one listen: it is paid once, by construction.

ListenBrainz matches a listen to its artists some time after it arrives. A listen with no artists yet, less than a day old, waits for a later read; after a day it is counted as it is.

When ListenBrainz cannot be reached, asks lyrid to wait, or no longer knows the name you linked, the panel says so, and the next read tries again. A read that fails partway keeps what it got.

## What is kept

For each listen: when it was, which artists it was by (their MusicBrainz ids), how long it ran, the light it gathered (none for the past) and where it came from — ListenBrainz or a Last.fm file. No track or album names. See [what lyrid keeps](/lyrid/reference/accounts/) — the export hands every row back, and deleting the account removes them.

**Unlink** stops the reading. What was gathered stays with the account until the account is deleted.

The decisions and their measurements are in [ADR 0019](https://github.com/lacodda/lyrid/blob/main/docs/adr/0019-listening-read-from-listenbrainz.md) and, for the history, [ADR 0020](https://github.com/lacodda/lyrid/blob/main/docs/adr/0020-the-past-lights-the-sky-and-does-not-pay.md).
