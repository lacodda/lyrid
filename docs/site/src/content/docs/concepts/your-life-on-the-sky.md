---
title: Your life on the sky
description: The home star your listening gathers around, the stars of your youth, a Last.fm history brought as a file, and the weekly letter.
---

Once lyrid knows what you have listened to, the sky can say things about you that no chart can: where you live on it, what was playing when you were sixteen, and what this week turned up. All of it is under **your sky** in the listening panel, signed in.

## Home star

Of the stars you have heard, your **home** is the one nearest to all the rest of your listening, each star counted as many times as you heard it. The panel names it — *home: Portishead* — and one press flies there.

It is not the centre of your listening. Someone who listens to techno and to baroque has a centre in the empty sky between the two, or on a star they have never heard; a home is somewhere you actually live. So the home is always a star you have heard, chosen among the ones you return to most. It moves as your listening does, and the weekly letter says when it has.

## Stars of your youth

Give the year you were born, and lyrid shows the music that came out while you were fifteen to twenty — six years, both ends counted:

- artists with an album, an EP or a single first released in those years — compilations are a catalogue going round again, not music coming out;
- whose first record came no more than ten years before. A band reissuing a catalogue from the sixties is not the sound of a youth in 2000, and the canon cannot tell a reissue typed *Album* from a new one, so the age of the act stands in for it;
- brightest first, sixty of them.

The ones you have heard are marked, and **show on the sky** rings all sixty. The question asked of the sky is the years, not you: `/api/youth/1998` is the same for everyone and cached like the tiles. The year itself is kept in your profile, listed by the [charter](/lyrid/reference/accounts/#the-privacy-charter-export-and-delete), and can be forgotten at any time.

## A Last.fm history

Last.fm has no export of its own; your scrobbles come out of it with a tool such as [lastfm.ghan.nl/export](https://lastfm.ghan.nl/export/), as CSV. Bring the file to **your sky**:

- A file with a header row is read by its column names — `uts` or another time column, `artist`, and where present `artist_mbid` and `track`, however an exporter spells them.
- The four bare columns of lastfm-to-csv — artist, album, track, date — are read too.
- Times may be Unix seconds or milliseconds, Last.fm's own `31 Jan 2020 12:34`, or ISO 8601; all are UTC, as Last.fm reports them. A row without a time — the track that was playing when the file was made — or without an artist is counted as unreadable and skipped.

An artist is placed by its MusicBrainz id when the file has one the canon knows. Otherwise the name means what Last.fm means by it: the best-known artist of that name, if they are at least twice as bright as the next. Closer than that is a coin flip, and the listen is kept without an artist rather than given to the wrong one.

What a file brings **opens stars and pays no light**, like the history from ListenBrainz: a file is whatever its owner says it is. The file itself is not kept. The same file brought twice adds nothing the second time, and a listen ListenBrainz also reports — someone who scrobbles to both — is one listen, told by ListenBrainz. **Forget** takes back everything a file brought and nothing else.

## The weekly letter

Ask for it under **your sky** and a plain-text letter arrives each Monday: how much you listened that week, the stars it opened for the first time, the ones you heard most, and where your home star stands. A week without listening sends nothing.

- Only a confirmed address can ask, and the letter is written in the language you were reading lyrid in when you asked.
- Weeks run Monday to Monday in UTC; the letter goes out within an hour of a week's end, once.
- Every letter has a link that stops it with one press, and your mail client's own *unsubscribe* works too.

The decisions are in [ADR 0020](https://github.com/lacodda/lyrid/blob/main/docs/adr/0020-the-past-lights-the-sky-and-does-not-pay.md) and [ADR 0021](https://github.com/lacodda/lyrid/blob/main/docs/adr/0021-a-weekly-letter-asked-for.md).
