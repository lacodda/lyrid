# 0007 · Prose parsed from wikitext, reached through the multistream index

Date: 2026-08-18. Status: accepted.

## Context

An artist card without prose is a table of identifiers. The words have to come
from Wikipedia, and two facts shaped how:

**There is no dump of ready-made extracts any more.** The Wikimedia Enterprise
HTML dumps, which carried an `abstract` field per article, stopped being
replicated to `dumps.wikimedia.org` on 24 March 2025, and
`enwiki-latest-abstract.xml.gz` is absent from the current listings — both
checked directly. The live REST summary endpoint is a rate-limited API, which
ADR 0002 forbids in the critical path.

**The article dump is 27 GB, and the canon wants a small slice of it.** Reading
the whole archive to find a few hundred thousand articles is the wrong shape.

## Decision

**Leads are parsed out of the article wikitext, and articles are reached
through the multistream index.**

- The archive is many independent bzip2 streams concatenated, each holding
  about a hundred articles; the index lists `byte_offset:page_id:title`. So an
  article is reached by seeking to its stream and decompressing that stream
  alone. Measured: **1.9 MB of transfer for one article** rather than 27 GB.
- Wanted articles are grouped by stream, so a stream holding several of them is
  read once.
- **The index and the archive must come from the same dated run.** The
  `latest-*` symlinks are rebuilt at different moments and their offsets
  disagree. An index that locates articles which then are not at their offsets
  is treated as an error rather than as an empty result, because writing that
  result would silently replace good prose with nothing.
- Which articles to fetch comes from `artist_wikidata.enwiki_title`, captured
  during the Wikidata pass — so this import needs no second look at that dump.

**The parser is written against measured shapes, not assumptions.** 867 real
articles were surveyed first:

| Shape | Frequency | Consequence |
| --- | --- | --- |
| Wikilinks | 97% | Flattened to their display words |
| `<ref>` footnotes | 64% | Removed with their nested templates |
| **Nested templates** | **13%** | **Brace counting, not regular expressions** |
| Parenthesis of pure pronunciation | 11% | Punctuation repaired after removal |
| HTML comment mid-sentence | 5% | Removed before anything else |

The nesting figure is the load-bearing one: a pattern that stops at the first
`}}` truncates one lead in eight.

**Not every template may be deleted.** `{{spaced ndash}}` between two years,
`{{convert|103.9|sqmi|sqkm}}`, `{{circa|1856}}` and `{{lang|de|…}}` are part of
the sentence. They are resolved to their words; citations, infoboxes,
maintenance banners and pronunciation are dropped.

**Attribution is stored in the same row as the text.** CC BY-SA obliges credit
to travel with each snippet, so `artist_prose` holds the article title, its
URL, the licence, the dump version and the revision id alongside the extract.
No query can select the prose and forget the licence, because they are one row.

## Consequences

- Prose is only as fresh as the dump; a claim in a card can be dated to a
  revision.
- Coverage follows `enwiki_title`, so an artist with no Wikipedia article has
  no prose — the same dark matter the rest of the canon already has.
- The parser is a heuristic over a format with no specification. `source_chars`
  and `extract_chars` are stored so a later pass can spot leads it mangled
  without re-reading 27 GB.
- Only English Wikipedia is imported. Other languages are the same mechanism
  with a different dump, and are not in this stage.
- One class of bug this design cannot avoid: a template resolved to nothing
  when it carried meaning. The mitigation is the survey, and adding cases as
  real articles show them.
