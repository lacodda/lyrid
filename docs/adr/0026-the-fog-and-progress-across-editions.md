# 26. The fog, and progress that survives a new canon

Date: 2026-11-05

## Status

Accepted. Fills in the fog that [ADR 0003](0003-sky-map-architecture.md) sketched, and
starts the "once and never" of [ADR 0012](0012-accounts-with-a-password.md).

## Context

v0.22 is the first version with a player's progress in it, so it has to answer
four questions at once, and the last of them outlives every feature built on
this one.

**What does the dark look like?** The approved reference for the interface draws
a soft fog: the unheard sky is grey silhouettes, regions have no names, an open
star lights what is around it, and up to three dashed echoes of similar unheard
stars always give a next step. ADR 0003 said how: a small per-user coverage
texture sampled in the star shader. It did not account for the tile pyramid,
whose levels filter by brightness: most of what someone finds for themselves is
dim, and would vanish on zooming out.

**What opens a star?** The Vision says listening. Half the sky has somewhere to
listen and a seventh has a channel that embeds, so a rule of "the player only"
would leave most of the sky reachable only by people with a ListenBrainz account.

**Who is asked for a mode, and when?** Every profile made so far landed in the
creative mode by a default nobody chose (decision of 2026-09-11, migration 0009).

**What happens to progress when the canon changes?** The canon is rebuilt from
dumps. Every rebuild renumbers the canon's own keys and lays the sky out again,
and MusicBrainz merges artists, so that some ids stop existing.

## Decision

**The server stores which stars, by MusicBrainz id, and nothing about light.** A
star is open when the listening ledger names it (ADR 0019) or `star_opening` does.
The second holds only what the ledger cannot: thirty seconds of a channel that
really sounded here, or a link followed out to listen. The browser works out the
rest: it stamps each open star's light into a texture, 1,024 texels a side and a
radius of 14 layout units (measured: 9 neighbours on the slice of 100,000, three
times that on the whole canon), and samples it in the vertex shader of the same
single draw call. So a sky laid out again lights the same stars in their new
places, and there is nothing derived to store or migrate.

**Own stars get a pass of their own.** The person's open stars are drawn from a
buffer beside the tiles with the field's program, whatever the zoom, and no dimmer
than a floor. The creative sky pays one uniform branch per star and draws nothing
extra.

**A star opens at thirty seconds of sound, or at a link followed.** The clock runs
on the player's own word that it is playing, so a channel that will not play, a
pause or a stall count for nothing. A link followed proves less, and is what there
is for the stars with no channel. `star_opening.how` says which (`played`,
`followed`, or `brought` from a browser), because it cannot be worked out later and
a sky with first discoverers will have to tell them apart. None of it pays light:
that stays with the ledger (ADR 0020).

**Fog is not secrecy.** The tiles stay public and the same for everybody, and
search and the card stay whole: a person can look anything up and fly there, and
the star stays dark until they listen. What the fog withholds is the map itself:
a click in the dark picks nothing, the list of the view holds only open stars and
nameless echoes, and a region nobody has been to reads "uncharted". A person can
only be cheating themselves, and that is not worth a server-side check on every
request.

**NULL is a mode not chosen.** The default went and the existing answers were
forgotten, so there is one screen and one route for someone who has just registered
and someone from before there was a choice. `PUT /api/me/mode` writes over nothing
only; a trigger on the column refuses to change or forget a mode once chosen,
whichever code asks. The question can be put off, because something that cannot be
taken back should not also be something that cannot be left. The dialog's focus
lands on "decide later", so a stray Enter cannot decide it. Choosing the creative
mode forgets anything opened on the way to choosing, and that mode keeps no openings
from then on: the charter does not keep what nothing uses.

**A visitor's listening waits in their browser.** Anonymous visitors see the whole
sky (ADR 0012), and what they hear is remembered in local storage by MusicBrainz id.
It moves into the account at any sign-in, not only a registration, and is forgotten
only once the server has said it holds it. Stale ids land on the star they became.

**Progress is carried once per edition per person.** `lyrid canon carry-over`
rewrites openings and the credits in the ledger through `artist_redirect`, in one
transaction per person under the ledger's lock, and marks the edition reached on the
profile. Two ids that became one keep the earliest moment and what opened it then. An
id the canon no longer holds and has no redirect stays where it is: it lights nothing
today, and the canon may hold it again. `tools/stage-seed.sh` runs this at the end of
filling a stand, and a server runs it when it starts. The profile's edition has no
foreign key, because filling a stand replaces `canon_edition` under the accounts.

Rewriting was chosen over resolving redirects on every read: with the rewrite done,
every query can join on an id the canon holds, and the edition on the profile says
plainly how far a person has been brought.

## Consequences

- **Drawing the fog costs nothing a person can see.** Measured on a slice of 100,000
  stars with six open and eighteen echoes, at 1440x900 on a laptop's integrated
  graphics, moving the camera at every zoom: the same 11.1 ms a frame in both modes,
  which is that screen's 90 Hz and so a ceiling rather than a cost. A sky with very
  many open stars was not measured.
- **Reading a sky costs a query that grows with the person.** Measured on the slice,
  on a laptop: 3.7 s for the echoes of 20,000 open stars from a cold cache, a case
  only a very long history reaches. Most people hold hundreds. It is asked once on
  load, once per star opened, and every five minutes. If long histories become common
  the echoes can be materialised, as the wish for the listening summary already says.
- **A layout moves every star.** The fog follows, but the person's mental map does
  not. Keeping an edition's layout close to the last one is a question of its own.
- **On a stand, a merge whose target was cut from the slice cannot be followed.** The
  redirect goes with the artist it points at, so that opening stays under its old id,
  dark, until a canon that holds the target arrives.
- **Edition 3 went under a live account on a real stand.** Openings and listens were
  made through the API on stars MusicBrainz then merged, the canon's tables were
  replaced, and the account came out with each merge followed, the earliest moment
  kept, a star that left the canon still recorded, and nothing else changed. Run a
  second time, the carry changed nothing.
- **Half the sky can only be opened by a link followed or a scrobble.** That is the
  honest size of the embedded player, not a rule of this version.
- **YouTube is out of the loop in automated browsers.** Playback was refused in the
  browser these versions are checked in, so the thirty seconds were rehearsed against
  a stand-in player. The real one needs a person's browser.
