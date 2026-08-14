# 0001 · Server stack and product form

Date: 2026-08-13. Status: accepted.

## Context

lyrid is a public, multi-tenant web service: a canonical "music universe" shared by all users, with per-user progress. The lacodda product line already has a production-proven server template (kasl-server): Rust backend, PostgreSQL, React SPA, CI with an MSRV job, tag-driven releases with trusted publishing.

## Decision

- Backend: Rust, **axum** + **sqlx**/PostgreSQL.
- Frontend: **React SPA**; the sky itself is a WebGL scene (see ADR 0003).
- Production rails follow the kasl-server template: fmt/clippy/test/msrv CI, releases by tag, git-cliff notes, Starlight docs site with Diátaxis structure.
- Edition 2024 across the workspace; MSRV is measured, not assumed.

## Consequences

- One production template across the line: infrastructure work is copying a known-good shape, not inventing one.
- Multi-tenant from the start: user data is isolated by user id; the canonical universe is shared read-only data.
- The heavy client scene (WebGL) lives entirely in the SPA; the server serves data and static tiles, not rendering.
