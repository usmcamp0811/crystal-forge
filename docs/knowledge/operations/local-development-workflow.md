---
type: Operator Guide
title: "Local development workflow, patterns, and common tasks"
description: "Describes running Crystal Forge locally, key directories, database notes, request flow, error handling, testing, common tasks, and a key files table; open it when starting development on the backend or web UI."
tags:
  - crystal-forge
  - operations
  - development
  - workflow
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# Local Development Workflow, Patterns, and Common Tasks

> **Status:** partial. Paths in this guide follow the old single-crate layout. The backend now lives in `packages/default/crates/cf-server/src/` (see [Backend Cargo workspace](../architecture/backend-cargo-workspace.md)) and migrations live in `packages/default/crates/cf-server/migrations/`. The commands and file names are verification candidates.

## Development Workflow

### Running Locally

```bash
# Start database
db-only up

# Start API server (from packages/default)
cargo run

# Start web UI (from packages/web-ui)
cargo run --serve
```

### Key Directories

| Path | Purpose |
|------|---------|
| `packages/default/src/` | Backend code |
| `packages/default/src/handlers/` | API endpoints |
| `packages/default/src/queries/` | Database queries |
| `packages/default/src/models/` | Data models |
| `packages/default/src/builder/` | Builder worker logic |
| `packages/default/src/deployment/` | Deployment logic |
| `packages/web-ui/src/` | Frontend code |
| `packages/web-ui/src/views/` | Page components |
| `packages/web-ui/src/components/` | Reusable UI components |
| `migrations/` | Database migrations |

### Database

- **PostgreSQL** is the single source of truth
- All data flows through the API (no direct DB access from UI)
- Migrations live in `packages/default/migrations/`
- Run with: `sqlx migrate run`

## Important Patterns

### Request Flow

```
HTTP Request
    ↓
Middleware (logging, auth)
    ↓
Handler (route logic)
    ↓
Query (database access)
    ↓
Response (JSON)
```

### Error Handling

- All errors return JSON: `{"error": {"code": "...", "message": "..."}}`
- Use `anyhow::Result` for fallible operations
- `?` operator for error propagation
- No `unwrap()` in production code

### Testing

- Unit tests in `tests/` modules
- Integration tests with test database
- Run with: `cargo test`

## Common Tasks

### Adding a New API Endpoint

1. **Define DTO** in `api/models.rs`
2. **Add query** in `queries/*.rs`
3. **Add handler** in `handlers/api/*.rs`
4. **Register route** in `server/mod.rs`
5. **Add frontend** in `web-ui/src/`

### Adding a New UI View

1. **Create component** in `views/`
2. **Add route** in `main.rs`
3. **Add navigation** in `AppShell`
4. **Add API calls** in `api/client.rs`

### Database Migration

1. Create SQL file in `migrations/`
2. Run: `sqlx migrate add migration_name`
3. Apply: `sqlx migrate run`

## Key Files Reference

| File | Purpose |
|------|---------|
| `src/server/mod.rs` | HTTP server setup, route registration |
| `src/handlers/api/mod.rs` | All API route handlers |
| `src/queries/mod.rs` | Database query modules |
| `src/models/mod.rs` | Data structures |
| `src/config/mod.rs` | Configuration loading |
| `src/builder/mod.rs` | Builder worker orchestration |
| `src/deployment/agent.rs` | Agent-side deployment logic |

## Related concepts

- [Backend Cargo workspace](../architecture/backend-cargo-workspace.md) - current crate layout and targeted checks
- [Server configuration reference](server-configuration-reference.md) - configuration used when running locally
- [System overview](../overview/system-overview.md) - product orientation
