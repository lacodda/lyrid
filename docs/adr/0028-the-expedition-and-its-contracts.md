# 28. The expedition, and contracts read from the openings

Date: 2026-11-10

## Status

Accepted. Builds on [ADR 0026](0026-the-fog-and-progress-across-editions.md):
one more way a star opens, and the first record of the game that is not an
opening.

## Context

v0.22 gave the exploration mode its dark and left a person in it with a hint:
search for an artist you love and play them. v0.23 is the onboarding the plan
calls the expedition (junction S5): name a favourite artist, land in the home
sector, take a first contract that teaches how the sky opens. Four questions
come with it, and the last one outlives this version, because the economy of
light (v0.28) pays for contracts.

**Does naming a star open it?** The fog's rule is that listening opens a star.
A seventh of the sky has a channel that embeds and half of it has anywhere to
listen at all, so a favourite named in the dark may have no way to be played
here, and the expedition would begin on a star the person cannot light.

**What is the home sector?** The obvious answer is a disc on the map. Measured
on edition 3 for the 2,000 most listened stars: the closest similar star lies
85 units away on the median and 219 at the ninetieth percentile, and the third
closest 105 and 255 - against 14 units of the fog's light. A disc wide enough
to hold the echoes would hold thousands of strangers; one the size of the light
would hold none of them.

**Where is progress kept?** A contract counts openings, which already have two
records (ADR 0026); a counter beside them is a third.

**What survives a new canon?** The home port and the star a contract is about
are stars, and stars are renumbered and merged by every rebuild.

## Decision

**Naming an artist you love opens their star, once, recorded as `named`.** A
person who loves an artist has heard them; the fog is not secrecy (ADR 0026),
and the one star a person can claim this way is the one they set out from. It
is written down as what it was, beside `played`, `followed` and `brought`, so a
sky with first discoverers can tell it apart. An expedition that starts lit has
its echoes in sight from the first second, which is what the first contract
needs.

**The home port lives on the profile, by MusicBrainz id, and is set once.**
Not the home star the listening points at: that one moves with what a person
hears, and this one is where they chose to start, and where the ship will be
based. The route writes it only when it is empty; whether a ship may move it is
the ship's question.

**A sector is a star's neighbourhood by the similarity the sky on show was laid
out from**: its placed neighbours in the current metric, without the special
purpose artists - exactly the set its echoes are drawn from. Following the
dashed rings always fulfils a contract about it, and no geometry has to be
tuned to two canons of different sizes.

**Progress is read from the openings on every request and never stored.** A new
star is one whose first opening, by either record, is at or after the contract
was issued: a star heard last year is not a discovery of this contract, however
recently it was heard again. The goal is three, or what the sector has left
unopened when that is fewer; with nothing left the contract is fulfilled as it
is issued, which the schema allows only then.

**What is stored is the moment a contract was fulfilled**, and it is the moment
of the opening that reached the goal, not the moment somebody looked. It is
written the first time a read finds it, and stays: a later canon that changes
the neighbourhood cannot take a fulfilled contract back, and the economy pays
against that moment the way it pays listens on arrival (ADR 0019).

**The home port and every contract's star are carried through redirects** with
the openings, in the same transaction per person (ADR 0026).

## Consequences

- **Reading a contract costs a pass over the person's ledger**, for the stars
  of one neighbourhood: the same order as reading their sky, and it is asked
  with it. Materialising listening (planned with the echoes in v0.48) would
  make both cheap together.
- **A contract about a star the sky on show does not hold makes no progress**
  until a canon holds it again. On a stand cut to a slice this can happen to a
  home port that left the slice; the contract waits rather than failing.
- **One kind of contract exists**, and the schema lists it. Daily contracts in
  v0.28 add kinds, not tables.
- **The expedition can be put off**, like the mode. A person who does gets the
  v0.22 sky: dark, with the hint and a button that asks again.
