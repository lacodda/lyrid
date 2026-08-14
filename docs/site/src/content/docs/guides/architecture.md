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
labels, and the accessible text overlay.

During development Vite proxies `/health` and `/api` to the API, so the
browser stays on one origin and no CORS handling exists on either side.

## Decisions

Technical decisions live as ADRs in the repository, so the reasoning stays
next to the code it constrains:

- [0001 · Server stack and product form](https://github.com/lacodda/lyrid/blob/main/docs/adr/0001-server-stack.md)
- [0002 · The universe comes from open dumps](https://github.com/lacodda/lyrid/blob/main/docs/adr/0002-universe-from-open-dumps.md)
- [0003 · Sky map: tile pyramid and a custom WebGL2 renderer](https://github.com/lacodda/lyrid/blob/main/docs/adr/0003-sky-map-architecture.md)
- [0004 · Similarity from a published dataset, brightness from the graph](https://github.com/lacodda/lyrid/blob/main/docs/adr/0004-similarity-from-a-published-dataset.md)

## Releases

Every stage of work is a version, closed by a tag. Pushing a tag builds
release artefacts, generates notes with git-cliff, and publishes the crate —
so a release is the tag, not a sequence of manual steps.
