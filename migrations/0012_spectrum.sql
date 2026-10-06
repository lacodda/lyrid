-- Spectra: what a star sounds like, measured from audio by AcousticBrainz.
--
-- AcousticBrainz ran Essentia over millions of listeners' own files from 2015
-- to 2022 and published what it measured as a frozen, CC0 dump. Each
-- measurement is of one recording; a star's spectrum is the measurements of
-- its recordings brought together. See ADR 0018.
--
-- One row per artist, not per recording: the canon holds no recordings, and
-- the card and the lenses ask about stars. Seven million rows kept for a
-- reader that does not exist would be weight, not data.

CREATE TABLE artist_spectrum (
    artist_id  integer PRIMARY KEY REFERENCES artist (id) ON DELETE CASCADE,
    -- How many of the artist's recordings the spectrum is measured on. Every
    -- band below is read from these same recordings, so "measured on 34
    -- recordings" is true of each of them rather than of one.
    recordings integer NOT NULL,
    -- Beats per minute: the median over the recordings, each recording the
    -- median of its own submissions. A median because the classic failure of
    -- a tempo estimator is to land on half or double the beat, and an average
    -- would drag every star towards its octave errors.
    tempo      real    NOT NULL,

    -- The mean, over the recordings, of what each high-level model said: the
    -- probability that a recording is danceable, acoustic, and so on. Stored
    -- as measured, because the bands below are an interpretation of them and
    -- an interpretation may be revised without reading 40 GB again.
    danceable  real    NOT NULL,
    acoustic   real    NOT NULL,
    electronic real    NOT NULL,
    aggressive real    NOT NULL,
    happy      real    NOT NULL,
    party      real    NOT NULL,
    relaxed    real    NOT NULL,
    sad        real    NOT NULL,
    -- Sung rather than instrumental.
    voice      real    NOT NULL,

    -- The bands a card draws besides tempo, danceable and voice, which are
    -- read as they are. Each formula lives here and nowhere else.
    --
    -- Energy is arousal: how charged the music is, from calm to intense.
    energy     real GENERATED ALWAYS AS ((aggressive + party + (1 - relaxed)) / 3) STORED,
    -- Mood is valence: from dark to bright. Quiet music leans dark here --
    -- the sad model hears softness as sorrow -- so solo piano sits at the dark
    -- end beside music that is dark in earnest.
    mood       real GENERATED ALWAYS AS ((happy + (1 - sad)) / 2) STORED,
    -- Sound runs from acoustic to electronic. Two models rather than one,
    -- because they are not complements: a piano ballad over a drum machine is
    -- neither, and both say so.
    sound      real GENERATED ALWAYS AS ((electronic + (1 - acoustic)) / 2) STORED,

    -- Fewer recordings than this and a spectrum is one track's mood, not a
    -- star's. The importer applies the same floor; the constraint makes a row
    -- under it impossible rather than merely unexpected.
    CONSTRAINT artist_spectrum_enough_recordings CHECK (recordings >= 3),
    CONSTRAINT artist_spectrum_tempo_positive CHECK (tempo > 0),
    CONSTRAINT artist_spectrum_probabilities CHECK (
        danceable  BETWEEN 0 AND 1 AND acoustic BETWEEN 0 AND 1 AND electronic BETWEEN 0 AND 1
        AND aggressive BETWEEN 0 AND 1 AND happy BETWEEN 0 AND 1 AND party BETWEEN 0 AND 1
        AND relaxed BETWEEN 0 AND 1 AND sad BETWEEN 0 AND 1 AND voice BETWEEN 0 AND 1
    )
);

-- Where a value sits among all measured stars: the percentiles of each band,
-- 0th to 100th, over the whole canon at import time.
--
-- A card shows a star's place on each band rather than the number behind it.
-- The models are poorly calibrated -- "relaxed" is above one half for most
-- recordings ever measured -- so a raw 0.81 says nothing, while "calmer than
-- four stars in five" says what the reader wanted to know.
--
-- Kept apart from the artists and never cut by a slice: a stand holding the
-- brightest hundred thousand should still place a star among all of them,
-- not among the ones that happened to survive the cut.
CREATE TABLE spectrum_scale (
    band      text   PRIMARY KEY,
    quantiles real[] NOT NULL,
    CONSTRAINT spectrum_scale_band_known CHECK (band IN ('tempo', 'energy', 'mood', 'danceable', 'sound', 'voice')),
    CONSTRAINT spectrum_scale_percentiles CHECK (cardinality(quantiles) = 101)
);
