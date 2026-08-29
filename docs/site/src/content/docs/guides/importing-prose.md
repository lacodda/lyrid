---
title: Importing prose
description: Add Wikipedia lead paragraphs, with their CC BY-SA attribution, without reading 27 GB.
---

A card of identifiers is a database row. This import gives each artist the
words that make it a page.

Run the [Wikidata import](/lyrid/guides/importing-facts/) first: the article
titles come from its sitelinks and are already waiting in the canon.

## Get the dump

Two files, from the **same dated run** — this matters, see below:

```sh
base=https://dumps.wikimedia.org/enwiki/20260801
curl -O "$base/enwiki-20260801-pages-articles-multistream.xml.bz2"        # 26.7 GB
curl -O "$base/enwiki-20260801-pages-articles-multistream-index.txt.bz2"  # 284 MB
```

## Run the import

```sh
lyrid import wikipedia \
  --dump  ./enwiki-20260801-pages-articles-multistream.xml.bz2 \
  --index ./enwiki-20260801-pages-articles-multistream-index.txt.bz2
```

```
INFO lyrid::import::wikipedia: looking for articles named by the canon wanted=…
INFO lyrid::import::wikipedia: index read index_lines=… articles_located=… streams=…
INFO lyrid::import::wikipedia: articles read streams_read=… extracted=… skipped_without_lead=…
INFO lyrid::import::wikipedia: prose import complete version=20260801 rows=…
```

## The 27 GB is not read

The archive is not one bzip2 stream but many, concatenated — each holding about
a hundred articles. The index lists, for every article, the byte offset of the
stream it lives in:

```
161410708:21231:Nirvana (band)
```

So an article is reached by seeking to that offset and decompressing that one
stream. Measured against the real dump: **1.9 MB of reading for one article**.
Wanted articles are grouped by stream, so a stream holding several of the
canon's artists is read once.

:::caution[The index and the dump must be from the same run]
The `latest-*` filenames are symlinks rebuilt at different moments, so an index
from one rebuild points into the wrong places in an archive from another. Use a
dated prefix like `20260801` for both.

If the offsets disagree, the import **fails** rather than writing an empty
result — silently replacing good prose with nothing would be worse than
stopping.
:::

## What the parser does

There is no dump of ready-made extracts any more, so the lead is parsed out of
the article source. The rules were written against a survey of **867 real
articles**, not against assumptions:

| Found in the leads | Frequency | What happens |
| --- | --- | --- |
| Wikilinks `[[a\|b]]` | 97% | Flattened to the displayed words |
| `<ref>` footnotes | 64% | Removed, nested templates and all |
| **Nested templates** | **13%** | Removed by counting braces |
| Parenthesis of pure pronunciation | 11% | Removed, punctuation repaired |
| HTML comment mid-sentence | 5% | Removed |

That 13% is why the parser counts braces instead of matching a pattern: a
pattern that stops at the first `}}` truncates one lead in eight.

**What the survey did not catch.** The first two full runs both died on the
same class of mistake — a fixed byte window landing inside a multi-byte
character, once on an en dash and once on an `á` opening an article — and
neither shape appears in 867 articles. The rule now is that a fixed byte count
never indexes a string, and each article is parsed behind a guard: a panic
costs that one article, counted and named in the log, instead of the hours
already spent. Measured on the full run afterwards: **181,491 articles located,
177,976 with prose, 3,515 without a usable lead, zero panics.**

**Some templates are kept**, because deleting them breaks the sentence:

| Template | Becomes |
| --- | --- |
| `{{spaced ndash}}` | ` – ` (between two years) |
| `{{convert\|103.9\|sqmi\|sqkm\|1}}` | `103.9 sqmi` |
| `{{circa\|1856}}` | `c. 1856` |
| `{{lang\|de\|Lancaster}}` | `Lancaster` |
| `{{nowrap\|…}}` | its contents |

Citations, infoboxes, maintenance banners and pronunciation contribute nothing
and are dropped.

## Attribution travels with the text

CC BY-SA requires credit wherever the words go, so the credit is **in the same
row** as the words rather than attached when something is rendered:

```sql
SELECT a.name, p.source_title, p.source_url, p.licence, p.dump_version, p.revision_id
FROM artist_prose p JOIN artist a ON a.id = p.artist_id
ORDER BY a.name;
```

```
     name     |  source_title  |                 source_url                   |   licence    | dump_version
--------------+----------------+----------------------------------------------+--------------+--------------
 Led Zeppelin | Led Zeppelin   | https://en.wikipedia.org/wiki/Led_Zeppelin   | CC BY-SA 4.0 | 20260801
 Nirvana      | Nirvana (band) | https://en.wikipedia.org/wiki/Nirvana_(band) | CC BY-SA 4.0 | 20260801
```

No query can select the prose and forget the licence, because they are one row.
Displaying an extract means displaying a link to `source_url` and naming the
licence; the `revision_id` dates the claim to an exact revision.

## Judging the result

`source_chars` and `extract_chars` are stored so mangled leads can be found
without re-reading the dump:

```sql
SELECT a.name, p.source_chars, p.extract_chars,
       round(100.0 * p.extract_chars / NULLIF(p.source_chars, 0)) AS pct
FROM artist_prose p JOIN artist a ON a.id = p.artist_id
ORDER BY pct;
```

A ratio far below the rest is the signature of a lead the parser cut short —
usually a template it did not know. The parser is a heuristic over a format
with no specification, and this column is how the next case gets found.

## Re-importing

Importing again replaces every extract and updates the `dump_import` record
rather than adding a second one. Verified: a second run over the same input
produces identical rows.
