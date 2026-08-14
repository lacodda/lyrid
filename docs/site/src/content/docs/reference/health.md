---
title: Health endpoint
description: The contract of GET /health — status codes, body shape, and what "degraded" means.
---

`GET /health` answers liveness and readiness in one place: the process
responds, and a database round-trip says whether it can actually do its job.

## Request

```
GET /health
```

No authentication, no parameters.

## Response

| Condition             | Status | `status`     | `database`      |
| --------------------- | ------ | ------------ | --------------- |
| Database reachable    | `200`  | `"ok"`       | `"ok"`          |
| Database unreachable  | `503`  | `"degraded"` | `"unavailable"` |

```json
{
  "status": "ok",
  "version": "0.1.0",
  "database": "ok"
}
```

`version` is the server's own package version, which makes the endpoint the
authoritative answer to "what is actually deployed here".

## Why degraded is not down

A process that answers `503` with a parseable body is telling you something a
connection refusal cannot: it started, it read its configuration, it bound its
port, and the thing it cannot reach is the database. Orchestrators can restart
on it, and dashboards can distinguish "the server is gone" from "the server is
fine and Postgres is not".
