# 16. Listening to the sky: one player, a player per star, stations read once

Date: 2026-09-25

## Status

Accepted. Amends [ADR 0011](0011-listening-from-the-canon.md) on how the
channel is played; what is played is unchanged.

## Context

v0.14 adds the radio of a nebula — the artists of a genre or style, one after
another — and the signal of the day, one faint star to go and find. ADR 0011
already settled what can be played: an artist's own YouTube channel, through the
uploads playlist its id implies, and nothing that needs an API on the critical
path. Four questions came with making that into a radio.

**Who is in a nebula.** The names on the sky (ADR 0015) and the compass's "where
am I" both count an artist under its *main* genre and style — the one carried by
most of its releases. The radio of "Soul" has to play the stars written under
"Soul", or it plays a sky the map does not show. The rule was written out as a
`DISTINCT ON` in two queries; the radio would have been the third.

**How to ask it quickly.** Which of a nebula's members have a channel was
measured on the 100,000-star slice at 0.4 s warm and 3 s cold for Electronic, per
press of play. The playable stars, though, are few — about ten thousand, on the
slice and on the full canon alike, because channels are the scarce part.

**How a radio knows a song has ended.** The card played the channel in a bare
iframe, which can play but cannot say that a video ended or refuses to be
embedded.

**What the embedded player actually does.** Measured in a browser on 25 radio
stations: 16 answered within 2.4 s; 9 — a third — never answered at all, with no
state and no error. And a player that has loaded one channel's uploads silently
ignores a request for another's: it re-cues the list it already holds, and a
second request gets no answer. A radio built on one player played its first star
and skipped every one after it.

## Decision

**"Main" is one function in the schema**, `main_genres(artists integer[])`,
which ranks the genres of the artists it is given. The names, the compass and
the radio all call it. A function over a set rather than a view over everyone,
because every caller already holds a small set and a view cannot be told about
it: measured, a `NOT EXISTS` view made the compass 5× slower (400 ms against 70)
and a `LATERAL` view made cutting the names take 32 s; the function takes 47 ms,
160 ms and 0.85 s. It is plain SQL and `STABLE`, so the planner inlines it.

**The playable stars are read once per layout** into memory — each with its
channel, its brightness as the sky draws it, whether it is dark, and its main
genre and style — and read again when the newest layout changes. A radio or a
signal is a filter over them. The server reads them in the background as it
starts, so the first listener does not wait.

**The official IFrame Player API, and a new player for every star.** The API is
YouTube's own embed with a script beside it, not the Data API, so ADR 0002 and
ADR 0011 stand: the ids still come from MusicBrainz and nothing asks YouTube
about the canon. The script is fetched on the first press of play and never
before. A radio cues the star's uploads, plays one of the latest ten, and moves
on when it ends. A channel that does not answer within six seconds is skipped by
a radio and reported by a card; eight silent stars in a row stop the radio,
since that is YouTube out of reach rather than bad luck.

**One player in the product.** The card no longer embeds anything: its play
button hands the channel to the player, which keeps playing while the listener
flies elsewhere.

## Consequences

- The radio, the signal and the card's radio button answer from memory in
  milliseconds; the tuning costs one read of about 1.7 s per layout.
- Between two stars of a radio there is a second of a new player loading.
- A third of the canon's channels are silent in an embed, so a radio skips about
  one star in three. The signal of the day carries five stars, the same five for
  everyone, and plays the first that answers — one silent signal would otherwise
  be a mute day in three.
- The uploads playlist includes Shorts, and the radio sometimes plays one.
  YouTube's undocumented long-form playlist (`UULF…`) excludes them but is
  silent for some channels whose uploads play; that trade is left for later.
- Adding a migration now rebuilds every binary that embeds the migrations
  (`build.rs`): the macro reads the directory at compile time, and cargo did not
  know it.
