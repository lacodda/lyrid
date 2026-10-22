---
title: What changed in the sky
description: Which edition of the canon the sky is and what that edition changed, the stars rising this week, and the releases out around now.
---

The sky holds still between rebuilds of the canon, and the music under it does not. The status line names the **edition** of the canon you are looking at — *edition 1 · what changed* — and opens a panel with three things in it.

## This sky

An edition is the canon sealed once it is built: which import of each source it was made from, and the layout its stars are placed by. The panel says which edition this is, when it was sealed and how many stars it has, and lists its sources with their versions — MusicBrainz of 15 August, Discogs of 1 August, ListenBrainz's listeners of 15 October, its older similarity of 2020, and so on.

From the second edition on it also says what changed since the one before:

- **new stars** — artists who have a place in this sky and had none in the last one; the brightest are named, and a press flies to them;
- **merged** — stars MusicBrainz merged into another artist, by the name they had, with the star they became;
- **gone dark** — stars that left without a merge: deleted, or left with too few listeners, or none shared with anyone else, to place them by.

The first edition changed nothing, because nothing came before it, and the panel says so. Editions are explained in [ADR 0023](https://github.com/lacodda/lyrid/blob/main/docs/adr/0023-editions-of-the-canon.md).

## Rising this week

Every day ListenBrainz publishes the listens it received, everyone's, as an open dump. lyrid reads it and counts, for each star, **how many people** listened to it that day — people, not listens: one account sending a hundred thousand listens a day is one listener.

A star is rising when its listeners a day over the last week are well above what they were over the four weeks before: at least five a day, and half as many again as before. Days ListenBrainz did not publish are left out of both sides rather than counted as quiet, and nothing is called rising until there is a week and a month to compare. Each riser shows both numbers — *25 a day · was 2.6*.

This is everyone's listening on ListenBrainz, not yours: the listening lyrid keeps for a linked account serves only that account's own sky.

## Out around now

ListenBrainz also lists the releases coming out within a month of today. The panel shows the ones by stars of this sky from the week before today to the week after — *out* and *coming* — brightest stars first, each one a way to fly to its star. Tags and cover art are not shown: neither comes under an open licence.

## Where it comes from

Both are read by the server itself, every half hour, without anything a visitor does waiting on them. How it reads them, and why from the dumps rather than from an API: [ADR 0022](https://github.com/lacodda/lyrid/blob/main/docs/adr/0022-living-data-from-the-daily-dumps.md). How whoever runs a stand sees that it is working: [Watching a stand](/lyrid/guides/watching-a-stand/).
