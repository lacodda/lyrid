# 20. The past lights the sky and does not pay

Date: 2026-10-13

## Status

Accepted.

## Context

v0.17 read listening from the moment of the link (ADR 0019). v0.18 brings the
rest of a person's life on the sky: their whole ListenBrainz history, and a
Last.fm history for those whose scrobbles live there. Three questions came
with it.

**Does the past pay light?** Light is paid per minute, five times over for a
star heard fewer than ten times. A history is large -- the public account used
for measuring holds 220,287 listens from 2006 on -- and paid by the same rule
it would hand a long-time listener hundreds of thousands of light on the day
they link, against a thousand a day for everyone listening now. It is also the
cheapest thing to forge: ListenBrainz accepts listens with any past time (that
is how imports work), and a CSV file is whatever its owner typed into it.

**How is a history read?** Measured on 2026-10-13 against that account:
ListenBrainz says where a history begins (`oldest_listen_ts`) beside any page
of listens; bounded windows of 30, 60, 90, 180 and 365 days answered a full
page in 3.4 to 6.2 seconds -- the time follows the page, not the span -- while
the forty-second cut that shaped ADR 0019 belongs to requests bounded on one
side only. Twenty years in three-day windows is 2,400 requests; most of them
cross quiet stretches.

**What does a name in a Last.fm file mean?** A Last.fm export carries the
artist's name and, sometimes, a MusicBrainz id. Measured on the full canon:
4,703 of the 20,000 brightest artists share their name with another artist,
and for 4,486 of them the brightest is at least twice as bright as the next.

## Decision

**The past opens stars and pays nothing.** A listen from before the link, and
every listen from a file, is written to the ledger with `light` NULL -- "paid
nothing", which is a different statement from "paid zero". It opens the stars
it names and counts towards what is familiar, so a star heard five hundred
times in 2015 pays as familiar on the first listen after the link. A file can
never pay: the ledger refuses a `lastfm` row with light.

**One reader, one cursor.** A new link starts reading at the beginning of the
person's history and moves forwards, so the ledger grows in the order the
listening happened and every live listen is priced against everything heard
before it. Fresh listening is read once the history has caught up with the
link. While it has not, a link whose last read finished well is taken again at
once rather than in a quarter of an hour, and reading by hand is refused: the
background is already reading as fast as ListenBrainz allows. Links made under
v0.17 read their history from the start; the ledger's key makes the listens
they already hold cost nothing, and those keep what they were paid.

**Windows grow over quiet years.** The first window of a read is three days,
as before -- the routine read is one request -- and each window that comes
back done doubles the next, up to ninety days.

**Prices follow the clock, not the ledger.** "Heard before" counts the rows
before the first new listen; the rows already in the ledger within the same
stretch are walked through with the new ones, counted and not paid again. A
listen read late is priced as the listen it was at its own moment. Every
writer of the ledger -- the background, the button, a file -- holds one
advisory lock per account, since an account need not be linked to receive a
file.

**A name means what Last.fm means.** An id the canon knows wins. Otherwise a
name is the best-known artist of that name if they are at least twice as
bright as the next, and nobody when it is closer than that: the listen is kept,
unnamed, because a wrong star is worse than none. One moment is one listen: a
file does not write a second that ListenBrainz already told, and ListenBrainz's
telling replaces a file's copy, since it knows the artists by id. The file is
not kept; a listen from it is identified by a UUID derived from its artist and
track names, so the same file read twice is the same rows, and every listen a
file brought can be forgotten at once.

## Consequences

- **The economy starts level.** Whatever light buys from v0.25 on, a person
  who joined with twenty years of history and a person who joined with none
  hold the same light on their first day.
- **A forged history pays nothing.** The forgery ADR 0019 left open is closed
  for everything submitted with a past time; what remains is listening
  submitted as current, which the per-listen ceiling bounds until v0.25.
- **A link's first hour is history.** Someone with twenty years of listening
  waits half an hour or so before their latest listens show; the panel says
  how far the history has got.
- **The ledger holds whole histories.** 220,000 rows for one account. The
  summary and the home star read them by aggregate on each request; measured
  in the release's live run before anything is materialised.
