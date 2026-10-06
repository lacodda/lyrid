# 18. Spectra from AcousticBrainz: a star's place on six bands, from trusted measurements only

Date: 2026-10-06

## Status

Accepted.

## Context

The plan has promised since the founding that a star would have a spectrum —
energy, tempo, mood — from AcousticBrainz, the sixth open source the canon is
built from. v0.7 left it out of the card because the source had never been
imported; v0.16 is that import.

AcousticBrainz ran the Essentia extractor over listeners' own audio files from
2015 to 2022 and published what it measured before it closed: a frozen dump
dated 2022-06-23, CC0 (the archives carry the licence in a `COPYING` file).
Two parts of it matter here, both keyed by recording MBID and submission
number: thirty `tar.zst` archives of high-level JSON, one document per
submission with what the trained models made of the audio — danceable,
acoustic, electronic, aggressive, happy, party, relaxed, sad, sung, and more —
and a CSV of rhythm descriptors with the tempo.

Three facts shaped the decision.

**The maintainers did not trust it.** The post that closed the project
(MetaBrainz blog, 2022-02-16) says the tempo is wrong for many recordings, the
key is right only for some styles, and the genre models "didn't work very
well". Nothing in the dump says which values are the wrong ones.

**A large share of submissions is broken in a way that can be seen.** Binning
every output to five decimals finds single values held by thousands of
unrelated submissions while the values around them vary smoothly: "relaxed" at
0.80882, "electronic" at 0.97939, "acoustic" at 0.09559, "sad" at 0.35221,
"gender" at 0.62213. A model answers the same thing whatever the music when
the features it reads are broken. In the dump's 100,000-submission sample a
quarter of submissions carry at least one such value, most often from lossy
files (70–86% of MP3, Vorbis and AAC submissions against 4% of FLAC); Pink
Floyd's were 85% collapsed, which made them, unfiltered, an electronic
instrumental act. The tempo of those same submissions is distributed exactly
as everyone else's: the rhythm extractor reads the signal another way.

**The canon holds no recordings.** The data is per recording; the sky is made
of artists.

## Decision

**A spectrum belongs to a star, and is its recordings brought together.**
Recordings are resolved to artists through a MusicBrainz full export — the
recording table, its merge redirects, and the first credited artist, the rule
the canon already uses to place a release group. Any recent export will do:
MusicBrainz keeps only the two latest, so the one at hand is usually newer
than the canon, and an artist id is stable for as long as the artist exists.
Not through the artist tags inside the submissions: those are whatever the
submitter's files said, and MusicBrainz is the canon's truth about who made a
recording. Special purpose artists get no spectrum.

**Every recording weighs one, and a spectrum needs three.** Submissions are
averaged into their recording, recordings into their artist: a hit submitted
two hundred times is still one song. Fewer than three recordings is one song's
mood rather than a star's, and the table refuses such a row.

**A submission is trusted only when every model answered.** One output at a
collapse point (±0.0001, which takes the trail a collapse leaves beside
itself), or at exactly one half — the uniform answer of a model with nothing
to go on — and the whole submission is dropped, because its outputs are one
analysis of one file and part of it is known to be broken. The import bins
every output as it reads and warns about any spike the list does not explain,
so the list is checked against the whole dump on every run rather than against
a sample once. Tempo uses every submission, as the median of its recording,
and the median of those for the star: a median because a tempo estimator's
classic failure is half or double the beat.

**Six bands, each a place among all measured stars.** Tempo; energy, from
calm to intense, `(aggressive + party + 1 − relaxed) / 3`; mood, from dark to
bright, `(happy + 1 − sad) / 2`; dance; sound, from acoustic to electronic,
`(electronic + 1 − acoustic) / 2`; and voice, from instrumental to sung. The
table stores the nine measured means as measured, and derives the three
composite bands as generated columns, so each formula lives once, in the
schema, and can be revised without reading 42 GB again. The card shows a
star's rank on each band among every measured star, read off the band's
percentiles in `spectrum_scale`, not the number a model gave: the models are
poorly calibrated — "relaxed" is above one half for most music ever measured —
so a raw figure would put almost every star at one end of the same line. Tempo
also keeps its beats per minute, which mean something by themselves.

**What is not taken.** The genre classifiers (genres come from Discogs, and
their own maintainers found them weak), the key (right only for some styles),
and the gender model, which is read only as one of the signs of a collapsed
submission and never stored: what a model guesses about a singer's sex is not
a fact about anyone.

**The word is the band's.** "Spectrum" had also named the genre shares in the
comparison of two stars (v0.13); those are now the genre mix, so one word on
one screen means one thing.

## Measured

On the full dump (2026-10-06, 16 threads, release build):

- **29,460,584 submissions** read in 15 minutes. 2,760,952 (9.4%) carried a
  collapsed output, 1,590,462 (5.4%) an undecided one, 145,386 (0.5%) had no
  high-level block at all. 24,963,784 were trusted.
- **6,623,985 recordings** have at least one trusted submission, and every one
  of them has a tempo: 27,528,046 rhythm rows were taken for them.
- The spike search finds all five listed collapses and nothing else: relaxed
  in 2,219,517 submissions, electronic in 1,064,132, acoustic in 941,939,
  gender in 820,324, sad in 95,701. Sad's never showed in the 100,000-submission
  sample, which is why the search runs on every import rather than once.
- Against the MusicBrainz export of 2026-09-30, 5,920,382 recordings resolved
  under their own MBID, 687,984 (10.4%) only through a merge since 2022, and
  15,619 no longer exist.
- **218,113 stars have a spectrum** — 7.4% of the canon's 2,959,376, and 58,010
  of the 100,000 a stand's slice keeps: the measured stars are the listened-to
  ones. The median star is measured on 11 recordings, the 90th percentile on
  55, the 99th on 325.
- The whole run took 86 minutes, 66 of them in the bzip2 pass over the
  MusicBrainz export.
- On the stars a reader would check: energy runs from Enya (0.06) and Leonard
  Cohen through Pink Floyd and Taylor Swift to Metallica and Slayer (0.76);
  Daft Punk is the most electronic of them (0.84) and Frank Sinatra the most
  acoustic (0.20); Miles Davis is instrumental, Johnny Cash sung. Tempo is the
  weakest band, as the maintainers warned: half of all measured stars sit
  between 114 and 132 BPM, so a few beats move a star a long way along it.

## Consequences

- The import reads about 42 GB of zstd JSON and a 7.6 GB MusicBrainz export.
  The high-level archives decode in parallel, one per core; the export is one
  bzip2 pass, the longest part of the run.
- The dump will never change, so a spectrum is as old as 2022: an artist who
  started later has none, and one who changed sound since keeps the old one.
- A star's spectrum says where it stands among the measured stars of the
  canon, so it moves a little whenever the canon is imported again — the same
  property a percentile has anywhere.
- Quiet music leans dark on the mood band: the sad model hears softness as
  sorrow, so solo piano sits at the dark end beside music that is dark in
  earnest. The card does not pretend otherwise; the band is named "mood", not
  "sorrow".
- The slice cuts `artist_spectrum` with the artists and keeps
  `spectrum_scale` whole, so a stand places its stars among all measured stars.
  Carrying spectra to a stand is `tools/stage-seed.sh --tables
  artist_spectrum,spectrum_scale`; one table without the other draws no
  spectrum, with a warning in the log.
