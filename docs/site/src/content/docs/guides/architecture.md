---
title: How lyrid is put together
description: The pieces of the repository, how they fit, and where the architecture decisions are recorded.
---

lyrid is one repository holding four things: the API, the data pipelines, the
SPA, and this site.

```
lyrid/
├── src/          the axum server
├── migrations/   sqlx migrations, applied on start
├── web/          the React SPA (Vite, TypeScript)
├── docs/
│   ├── adr/      architecture decision records
│   └── site/     this documentation site (Astro + Starlight)
└── assets/       the brand mark and banner
```

## The API

Rust with **axum** over **PostgreSQL** through **sqlx**. Configuration comes
from the environment and nothing else; there is no config file to drift from
the deployment. Migrations run at startup, so a fresh database and an upgraded
one reach the same state by the same path.

## The pipelines

The universe is built offline: dumps are imported, similarity is computed, and
the sky layout is projected into 2D and **versioned**. The output is a tile
pyramid of static binary blobs. Nothing here runs per request, and browsing
the sky never touches the database.

## The SPA

React and TypeScript, built by Vite. The sky is not a DOM tree — it is a
WebGL2 scene with its own renderer, drawing all visible stars in a single
instanced draw call. React handles everything around it: pages, panels,
labels, and the list that makes the canvas reachable.

Everything that is not the canvas is built from [dowel](https://lacodda.github.io/dowel),
the line's design system: its tokens are the vocabulary and its primitives are
copied into `web/src/components/ui`. No colour is written down in a screen, so
a screen is correct in whatever accent and theme the product turns out to have.
Two tokens are lyrid's own — the `--void` behind the stars, and the `--glass`
that floating chrome is made of, because an opaque panel over a map hides the
map. Dark is pinned rather than followed: the product is a night sky, and a
light theme over it would be white panels in space. See
[ADR 0013](https://github.com/lacodda/lyrid/blob/main/docs/adr/0013-interface-on-dowel.md).

The interface speaks English and Russian, through i18next. English is the
source language and never a fallback: `web/tools/check-locales.mjs` runs in the
lint gate and fails the build on a missing key rather than letting an English
line reach a Russian screen. The canon is not in that layer — an artist's
name or a Wikipedia extract arrives in whatever language its source wrote it.

**Browsing the sky is almost free of the API.** Panning and zooming fetch tiles
and nothing else, with one exception: the list of stars in view beside the
canvas asks `/api/nearby` once per settled view, because the tiles carry no
names. That list is the keyboard's and the screen reader's only path to a star
— a canvas is one element with no children. See
[ADR 0014](https://github.com/lacodda/lyrid/blob/main/docs/adr/0014-the-sky-as-a-list.md).

During development Vite proxies `/health` and `/api` to the API, so the
browser stays on one origin and no CORS handling exists on either side.

## Decisions

Technical decisions live as ADRs in the repository, so the reasoning stays
next to the code it constrains:

- [0001 · Server stack and product form](https://github.com/lacodda/lyrid/blob/main/docs/adr/0001-server-stack.md)
- [0002 · The universe comes from open dumps](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md)
- [0003 · Sky map: tile pyramid and a custom WebGL2 renderer](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md)
- [0004 · Similarity from a published dataset, brightness from the graph](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)
- [0005 · Genres from Discogs, not MusicBrainz](https://github.com/lacodda/lyrid/blob/main/docs/adr/0005-genres-from-discogs.md)
- [0006 · Facts and influence from the Wikidata dump](https://github.com/lacodda/lyrid/blob/main/docs/adr/0006-facts-from-the-wikidata-dump.md)
- [0007 · Prose parsed from wikitext, attribution stored with it](https://github.com/lacodda/lyrid/blob/main/docs/adr/0007-prose-parsed-from-wikitext.md)
- [0008 · The layout is written here](https://github.com/lacodda/lyrid/blob/main/docs/adr/0008-layout-written-here.md)
- [0009 · The renderer is measured, not estimated](https://github.com/lacodda/lyrid/blob/main/docs/adr/0009-renderer-measured.md)
- [0010 · A slice for a small stand](https://github.com/lacodda/lyrid/blob/main/docs/adr/0010-a-slice-for-a-small-stand.md)
- [0011 · Listening from the canon, not from an API](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md)
- [0012 · Accounts with a password, sessions in the database](https://github.com/lacodda/lyrid/blob/main/docs/adr/0012-accounts-with-a-password.md)
- [0013 · The interface is dowel, pinned dark, in two languages](https://github.com/lacodda/lyrid/blob/main/docs/adr/0013-interface-on-dowel.md)
- [0014 · The sky is also a list, and the server answers what is in view](https://github.com/lacodda/lyrid/blob/main/docs/adr/0014-the-sky-as-a-list.md)

## Releases

Every stage of work is a version, closed by a tag. Pushing a tag builds
release artefacts, generates notes with git-cliff, and publishes the crate —
so a release is the tag, not a sequence of manual steps.
