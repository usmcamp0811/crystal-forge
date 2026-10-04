---
type: API
title: "Agent API (machine auth) and Cache API"
description: "Describes key-signed agent and builder endpoints, the builder job example, and the proposed binary cache management endpoints; open it when working on machine-authenticated calls or cache management routes."
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

## Agent API (Machine Auth)

These endpoints use **key-based authentication** (not user sessions). They're for builders and agents to communicate with the server.

### How It Works

1. Builder/Agent registers with a public key
2. Each request includes signature in header
3. Server verifies signature before processing

### Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/agent/heartbeat` | Builder Key | Builder reports status |
| POST | `/agent/state` | Agent Key | Agent reports state |
| POST | `/agent/report` | Agent Key | Report build/deploy result |
| GET | `/agent/job` | Builder Key | Get next build job |
| POST | `/agent/job/:id/complete` | Builder Key | Report job complete |

### Example: Builder Gets Job

**Request:**
```bash
GET /api/v1/agent/job
X-Builder-Key: builder-key-id
X-Builder-Signature: signed-timestamp
```

**Response:**
```json
{
  "data": {
    "job_id": "job-123",
    "derivation": "nixosConfigurations.production.system.built",
    "store_path": "/nix/store/xxx-nixos-system-x86_64",
    "system": "sys-456"
  }
}
```

> **Status:** Only `/agent/heartbeat` and `/agent/state` were found among the agent routes registered in `packages/default/crates/cf-server/src/bin/server.rs`; `/agent/report`, `/agent/job`, and `/agent/job/:id/complete` were not found. Builder machine calls are registered under `/api/v1/builders/:id/...` (for example `next-job`, `heartbeat`, `jobs/:job_id/complete`), and the signature headers shown above were not compared with `handlers/builder_request.rs`. Not reconciled in this migration.

## Cache API (Future - TASK-141)

Binary cache management (not yet implemented).

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/caches` | Admin+ | List caches |
| POST | `/caches` | Admin+ | Create cache |
| GET | `/caches/:id` | Admin+ | Get cache |
| PATCH | `/caches/:id` | Admin+ | Update cache |
| DELETE | `/caches/:id` | Admin+ | Delete cache |
| GET | `/environments/:id/cache-config` | Builder | Get cache for env |

> **Status:** proposed. The section above is marked future in the source document (TASK-141). The server now registers cache routes such as `/api/v1/caches`, `/api/v1/caches/:id/environments`, and `/api/v1/cache-push-jobs`, so the "not yet implemented" statement may be stale and the listed endpoints may differ from the registered routes. Not reconciled in this migration.

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](api-overview-errors-and-streaming.md)
- [Builders, queues, environments, dashboard, and admin APIs](builders-queues-environments-dashboard-admin-api.md)
