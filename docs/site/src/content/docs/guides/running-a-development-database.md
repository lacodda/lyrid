---
title: Running a development database
description: Bring up PostgreSQL for local work, apply migrations, reset it, and read what /health tells you when it breaks.
---

lyrid keeps everything except the database on the host: only PostgreSQL runs in a container, and only for development.

## Bring it up

```sh
docker compose up -d db
```

The compose file publishes `5432` and creates the `lyrid` database, user, and password — matching the `DATABASE_URL` in `.env.example`. Wait for the health check to pass before starting the server:

```sh
docker compose ps
```

## Point the server at it

```sh
cp .env.example .env
cargo run
```

Migrations in `migrations/` are applied on start, so a fresh container becomes a usable database the first time the server runs. There is no separate migration command to remember.

## Read the health endpoint

`/health` answers two different questions in one response:

```sh
curl -i http://127.0.0.1:8080/health
```

| Response | Meaning |
| --- | --- |
| `200` with `"status": "ok"` | The process is alive **and** a round-trip to the database succeeded. |
| `503` with `"status": "degraded"` | The process is alive, the database is not reachable. |
| No response at all | The server is not running or not bound where you are looking. |

The distinction matters for deployment: a degraded server should be pulled out of a load balancer without being restarted, because restarting it will not fix a database that is down.

## Start over

To throw away the data and begin from an empty database:

```sh
docker compose down -v
docker compose up -d db
```

The `-v` is the point — without it the volume survives and the old data comes back.

## Connect by hand

```sh
docker compose exec db psql -U lyrid -d lyrid
```

## Using a database you already have

Nothing binds lyrid to the compose file: point `DATABASE_URL` anywhere and the server will use it.

```sh
DATABASE_URL=postgres://user:password@host:5432/database cargo run
```

Keep real credentials in `.env`, which is not committed — never in `.env.example`, a config file, or a test fixture.

## Running the tests that need a database

Most of the suite runs against no database at all. The account rules are the
exception: constraints, cascades and the query deciding whether a saved camera
still means anything live in the schema and in SQL, and none of that can be
checked without a real PostgreSQL.

Those tests read `LYRID_TEST_DATABASE_URL` — deliberately not `DATABASE_URL`,
so that inheriting the variable already in `.env` cannot point them at a
database someone cares about.

```sh
LYRID_TEST_DATABASE_URL=postgres://lyrid:lyrid@localhost:5432/lyrid cargo test
```

Every test does its work inside a transaction and rolls it back, so a run
leaves the database as it found it.

Without the variable they **fail** rather than skipping. A suite that skips
itself reports success for code nobody executed, and the first defect these
tests were written for was found by hand precisely because nothing was
checking that path. To run only the suites that need no database:

```sh
cargo test --bins
```

CI does both: the cross-platform job runs `cargo test --bins`, and a
Linux-only job with a PostgreSQL service container runs the rest.
