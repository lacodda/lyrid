# 25. bzip2 on every core, and most lines never parsed

Date: 2026-10-29

## Status

Accepted. Revises the running times in ADR 0006 and ADR 0018; the decisions
in them stand.

## Context

Every large source lyrid reads is bzip2: the Wikidata JSON dump (103.6 GB),
the MusicBrainz export (7.2 GB), the Wikipedia articles and their index. A
full Wikidata pass took about eighteen hours, and the importer reads the file
twice -- facts first, then the labels of the items those facts point at
(ADR 0006) -- so a fresh canon of facts cost a day.

Measured on 2026-08-28: of each stretch of the Wikidata pass, **26.6 s went to
decompression and 9.5 s to parsing JSON**. bzip2 on one core was the wall, and
the parse behind it was paying for lines it then threw away: only 5.8% of
entities carry a MusicBrainz id (1,161 of the first 20,000), and checking a
line for the key `"P434"` before handing it to `serde_json` made the parse
five times faster while finding the same 1,161.

Two facts about the files decide the shape of the fix:

- **The Wikidata dump is one bzip2 stream.** The plan assumed it was a
  multistream file, like Wikipedia's articles, that could be cut into
  independent streams. It is not: the first 64 MB hold exactly one stream
  header. Streams do not divide it.
- **bzip2 blocks are independent.** The compressor cuts its input into blocks
  of up to 900 kB and starts everything over at each: run-length coding,
  Huffman tables, the Burrows-Wheeler transform. Each block carries the CRC of
  its contents, and each stream the combined CRC of its blocks. What makes
  blocks awkward is that they are packed bit after bit: a block is found by
  its 48-bit signature, `0x314159265359`, at any bit position.

## Decision

**A reader of our own that splits on blocks, over the reference decoder.**
`import::bz` reads the compressed input in order and finds every block
signature at whatever bit it sits. Each block is shifted onto a byte boundary
and wrapped into a one-block stream of its own -- a header, the block, an
end-of-stream marker carrying the block's own CRC -- and libbzip2 (through the
`bzip2` crate, pure Rust since 0.6) decompresses it on a worker thread, checking
the CRC as it would for any file. The reader puts the blocks back in order and
checks each stream's combined CRC against the one written at its end.

The reader is a `BufRead` over the decompressed blocks, so every importer takes
it where it took `MultiBzDecoder`: Wikidata, MusicBrainz, the recordings pass
behind the spectra, ListenBrainz's relations, the Wikipedia index.

- **Errors stay errors.** A corrupt block fails its CRC; a lost or repeated
  block fails the stream's combined CRC. Without that last check a block
  dropped by the splitter would read as a shorter, valid file -- a test removes
  one block and expects the error.
- **A false signature is mended, not fatal.** The 48-bit pattern can occur by
  chance inside a block's data, once in 2^48 bit positions -- with two
  signatures to look for, roughly one Wikidata pass in two hundred meets one.
  Rare, but a canon is not rebuilt on odds. The two halves of a block cut there fail their CRCs;
  the reader joins a failed segment to the next one and decodes them as one.
  A false end-of-stream signature is recognised by what must follow a real
  one: the CRC, padding to the byte, then nothing or another stream header.
- **Memory is bounded.** At most four blocks per worker are in flight; the
  splitter waits for room rather than reading ahead of a slow consumer.

**The Wikidata passes parse almost nothing.** Lines are read as bytes, never
checked as UTF-8. The fact pass looks for `"P434"` and lets every line without
it go. The label pass reads the entity's id off the head of the line --
`{"type":"item","id":"Q31",` are the entity's own first two keys, so the id
read there cannot come from deeper inside -- and parses only the items it
wants. A line of any other shape is parsed in full, so a change of key order
upstream costs speed, not labels.

**Wikipedia's article streams are read on every core.** That file *is*
multistream and the index gives every stream's offset, so the workers each hold
a handle on the archive and take streams in file order; the prose comes out in
that order whichever worker read it.

## Considered

- **A ready parallel decoder crate.** crates.io has several -- `lbzip2`,
  `parallel_bzip2_decoder`, `pbz2`, `parallel-bz2-redux`, `crabz2`, `mibz2` --
  all younger than a year, with a few thousand downloads and their own
  implementations of the Burrows-Wheeler and Huffman stages. A decoding error
  there corrupts a canon. Here only the splitting is ours, and it is checked to
  the bit by CRCs computed by the reference decoder.
- **Splitting on streams.** Simpler, and enough for Wikipedia's articles, but
  the Wikidata dump is one stream: it would not have sped up the import the
  stage was opened for.
- **Parsing lines on several cores too.** Measured first: with the prefilter
  the parse runs well ahead of decompression, so a second pool would add
  ordering code for no time won.

## Consequences

Measured on 2026-10-29 on a laptop i7-1360P -- 4 performance and 8 efficiency
cores, 16 threads -- against the dumps of that week, the old code and the new
writing into two copies of the same canon:

| What | One core (v0.20) | Every core (v0.21) |
| --- | --- | --- |
| Reading `mbdump.tar.bz2` (7.2 GB, 25.4 GB of tar) | 914 s | 224 s, 112 MB of memory |
| Wikidata, first 3 million entities (37.5 GB of JSON): facts | 834 s | 232 s |
| the same: labels | 821 s | 252 s |
| Wikipedia prose, `20261001`: index | 54 s | 8 s |
| the same: 108,679 article streams | 38 min | 9.5 min |

- **The output is the same.** The MusicBrainz export read through both readers
  agrees on all 25,420,451,840 bytes. The Wikidata tables written by both
  versions from the same three million entities, and the 177,867 prose rows
  from the same Wikipedia run, have the same md5 row by row.
- **The fact pass now runs at the speed of decompression.** Reading the same
  three million entities with nothing but the search for `"P434"` takes
  236.6 s, the import 232 s: parsing the matching lines is hidden behind the
  blocks still being decompressed. Hence no second pool for parsing.
- **The ceiling is the machine.** The workers keep 9 to 13 threads busy, and a
  block costs two to three times the CPU time it costs alone: efficiency cores, shared
  hyperthreads, a laptop's power limit. More cores would go further; the reader
  does not need to change for them.

The real-dump check is a test: `agrees_with_libbzip2_on_a_real_dump` reads a
file named by `LYRID_BZ2_SAMPLE` through both readers and compares every byte.
