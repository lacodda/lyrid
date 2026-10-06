---
title: Importing spectra
description: Measure what each star sounds like — tempo, energy, mood and three more bands — from the frozen AcousticBrainz dump.
---

A spectrum is what a star sounds like: tempo, energy, mood, dance, sound and
voice, each drawn on the card as a place on a line among every measured star.
It comes from AcousticBrainz, which ran the Essentia extractor over listeners'
own audio files from 2015 to 2022 and published what it measured as a frozen
dump, CC0.

Run the [MusicBrainz import](/lyrid/guides/importing-musicbrainz/) first: the
spectra attach to the canon's artists.

## Get the dumps

Two parts of the dump, and a MusicBrainz full export to say whose recordings
they are:

```sh
ab=https://data.metabrainz.org/pub/musicbrainz/acousticbrainz/dumps
mkdir acousticbrainz && cd acousticbrainz
for n in $(seq 0 29); do
  curl -C - -O "$ab/acousticbrainz-highlevel-json-20220623/acousticbrainz-highlevel-json-20220623-$n.tar.zst"
done
curl -O "$ab/acousticbrainz-lowlevel-features-20220623/acousticbrainz-lowlevel-features-20220623-rhythm.tar.zst"
```

- **High-level JSON** — thirty archives, about 42 GB together, one JSON
  document per submission holding what the trained models made of the audio.
- **Rhythm CSV** — about 1.3 GB, the tempo of every submission.
- **`mbdump.tar.bz2`** — the same file the [MusicBrainz
  import](/lyrid/guides/importing-musicbrainz/) reads. Any recent export will
  do: MusicBrainz keeps only the latest two, so it is usually newer than the
  canon, and an artist's id does not change while the artist exists.

The server supports resuming, so an interrupted download continues with `-C -`.
Each directory has a `sha256sums` file to check the archives against.

## Run the import

```sh
lyrid import acousticbrainz --dir ./acousticbrainz --musicbrainz ./mbdump.tar.bz2
```

Build it with `--release`: the high-level archives are decoded in parallel, one
per core, and a debug build takes many times longer. The import refuses a
directory where the archives do not run 0, 1, 2 … without a gap — a download
that did not finish would otherwise publish spectra measured on part of the
data as if it were the whole.

It reports what it dropped rather than leaving the gaps invisible:

```
INFO lyrid::import::acousticbrainz: high-level archives read; reading tempo submissions=29460584 collapsed=2760952 undecided=1590462 unreadable=145386 recordings=6623985
INFO lyrid::import::acousticbrainz: a listed collapse, found where the list says output="relaxed" value=0.80882 submissions=2219517
…
INFO lyrid::import::acousticbrainz: tempo read; resolving recordings to artists rows=27528046 recordings=6623985
INFO lyrid::import::acousticbrainz: recordings resolved export="2026-09-30 00:22:23.614571+00" direct=5920382 redirected=687984 unknown=15619
INFO lyrid::import::acousticbrainz: spectra measured; writing to PostgreSQL artists=218113
```

Those are the figures of the 20220623 dump against the MusicBrainz export of
2026-09-30, on 16 threads: about an hour and a half, most of it in the single
bzip2 pass over `mbdump.tar.bz2`.

- **`collapsed`** — submissions where a model answered with its collapsed
  value. See below.
- **`undecided`** — submissions where a model answered exactly one half: both
  classes equally likely, the answer a model gives when it has nothing to go on.
- **`unreadable`** — documents with no high-level block at all.
- **`redirected`** — recordings found only through a merge since 2022.
- **`unknown`** — recordings MusicBrainz no longer has.

## What is not trusted

Some submissions carry a **collapsed** output: one value, to five decimals,
returned for thousands of unrelated files — "relaxed" at 0.80882, "electronic"
at 0.97939. The features the model read were broken for that file, and a model
fed broken features answers the same thing whatever the music. Lossy files are
the usual victims. A submission with any collapsed output, or any model at
exactly one half, is dropped whole: its outputs are one analysis of one file,
and part of it is known to be broken.

The import bins every output as it reads and reports every value that looks
collapsed: the listed ones as found, any other as a warning.

```
WARN lyrid::import::acousticbrainz: a value no music produces is not in the list of collapses output="…" value=… submissions=…
```

On the 20220623 dump the search finds the five listed collapses — relaxed,
electronic, acoustic, gender and sad, between 96,000 and 2.2 million
submissions each — and nothing else. The dump is frozen, so that settles the
question for good; the search stays so the claim is checked rather than
remembered.

Tempo is read from every submission: the collapse does not touch the rhythm
extractor, whose tempi are distributed the same with or without it.

## What a spectrum is made of

Each recording counts once, however many times it was submitted; a star needs
three measured recordings for a spectrum. Tempo is the median over the
recordings, each the median of its own submissions — a median, because a
tempo estimator's classic error is half or double the beat.

The other bands are means of the models' outputs, combined where one model
does not say it alone:

| Band | From | Runs from … to |
| --- | --- | --- |
| tempo | beats per minute | slow … fast |
| energy | `(aggressive + party + 1 − relaxed) / 3` | calm … intense |
| mood | `(happy + 1 − sad) / 2` | dark … bright |
| dance | `danceable` | not for dancing … for dancing |
| sound | `(electronic + 1 − acoustic) / 2` | acoustic … electronic |
| voice | `voice` | instrumental … sung |

The card shows a star's **place** on each band among every measured star, not
the number itself. The models are poorly calibrated — most music ever measured
is "relaxed" by their reckoning — so a raw figure would put nearly every star
at the same end of the same line. The places are read off `spectrum_scale`,
which the import fills with each band's percentiles. See
[the schema](/lyrid/reference/canon-schema/#artist_spectrum) and
[ADR 0018](https://github.com/lacodda/lyrid/blob/main/docs/adr/0018-spectra-from-acousticbrainz.md).

## On a stand

A slice keeps the spectra of the artists it keeps, and keeps the scale whole,
so a stand places its stars among all measured stars of the canon. To carry
spectra to a stand without replacing anything else:

```sh
tools/stage-seed.sh --skip-tiles --tables artist_spectrum,spectrum_scale
```

The two tables go together; a stand with one and not the other draws no
spectrum and says why in its log.
