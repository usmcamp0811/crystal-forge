---
type: API
title: "Agent API (machine auth) and Cache API"
description: "Describes the machine-authenticated agent routes, the API-only builder job endpoints, and the implemented cache-destination and cache-push-job APIs; open it when working on machine-authenticated calls or cache administration."
tags:
  - crystal-forge
  - api
  - agent
  - builder
  - cache
  - machine-auth
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
---

# Agent API (machine auth) and Cache API

## Agent API (machine authentication)

Agent requests use machine authentication, not browser user sessions. Builder
requests use the separate Ed25519-signed builder API and session checks below.

### How It Works

1. Builder/Agent registers with a public key
2. Each request includes signature in header
3. Server verifies signature before processing

### Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/agent/heartbeat` | Agent machine auth | Report heartbeat |
| POST | `/agent/state` | Agent machine auth | Report observed system state |
| POST | `/system_state` | Agent machine auth | Compatibility state-report route |
| POST | `/agent/deployment-started` | Agent machine auth | Report deployment start |
| POST | `/agent/deployment-failed` | Agent machine auth | Report deployment failure |

## Builder job API

Builder work uses `/api/v1/builders/:id/...`, not the agent namespace. The
builder signs requests with `X-Builder-ID`, `X-Timestamp`, and `X-Signature`;
it also sends its established `X-Builder-Session-ID` on session-bound calls.
`GET` and `POST /api/v1/builders/:id/next-job` both request a job. Job
completion and failure use
`POST /api/v1/builders/:id/jobs/:job_id/complete` and `/fail`. The complete
route records the derivation and verifies builder-reported cache publication.
See [Builder job lifecycle API](builder-job-lifecycle-api.md) for the request
contract and route details.

## Cache administration API

The server registers the following cache-destination and cache-push-job routes.
These routes are not a server-side cache-push worker; builder-side publication
is described in [Cache push process](../caches/cache-push-process.md).

| Method | Endpoint | Description |
| --- | --- | --- |
| GET, POST | `/api/v1/caches` | List or create cache destinations |
| POST | `/api/v1/caches/test-credentials` | Test destination credentials |
| GET, PUT, DELETE | `/api/v1/caches/:id` | Read, update, or delete a destination |
| GET, PUT | `/api/v1/caches/:id/environments` | Read or assign destination environments |
| GET | `/api/v1/environments/:id/caches` | List an environment's cache destinations |
| GET | `/api/v1/cache-push-jobs` | List cache publication records |
| GET | `/api/v1/cache-push-jobs/:id` | Read one cache publication record |
| POST | `/api/v1/cache-push-jobs/:id/retry` | Request retry |
| POST | `/api/v1/cache-push-jobs/:id/cancel` | Cancel a pending job |
| POST | `/api/v1/cache-push-jobs/bulk-retry` | Retry selected jobs |
| POST | `/api/v1/cache-push-jobs/bulk-cancel` | Cancel selected jobs |

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](api-overview-errors-and-streaming.md)
- [Builders, queues, environments, dashboard, and admin APIs](builders-queues-environments-dashboard-admin-api.md)
