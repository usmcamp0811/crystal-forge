---
type: Operator Guide
title: "Adding a New API Endpoint"
description: "Step-by-step developer guide for adding a backend API endpoint (DTO, query, handler, route, authorization) plus the source file organization used by the original specification."
tags:
  - crystal-forge
  - api
  - developer-guide
  - rust
  - axum
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
---

# Adding a New API Endpoint

## Step 1: Define the DTO

In `src/api/models.rs`:

```rust
#[derive(Serialize, Deserialize)]
pub struct NewWidget {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WidgetResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

## Step 2: Add Query (if database needed)

In `src/queries/widgets.rs`:

```rust
pub async fn create_widget(
    pool: &PgPool,
    data: NewWidget,
) -> Result<WidgetResponse> {
    let row = sqlx::query_as!(
        WidgetResponse,
        "INSERT INTO widgets (name, description) 
         VALUES ($1, $2) 
         RETURNING id, name, description, created_at",
        data.name,
        data.description
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}
```

## Step 3: Add Handler

In `src/handlers/api/widgets.rs`:

```rust
pub async fn create_widget(
    State(state): State<AppState>,
    Json(data): Json<NewWidget>,
    require_operator: RequireOperator,  // Middleware
) -> Result<Json<WidgetResponse>, Error> {
    let widget = queries::widgets::create_widget(&state.pool, data)
        .await
        .map_err(Error::from)?;

    Ok(Json(WidgetResponse { data: widget }))
}
```

## Step 4: Register Route

In `src/server/mod.rs`:

```rust
Router::new()
    .route("/api/v1/widgets", post(handlers::widgets::create_widget))
    // ... other routes
```

## Step 5: Add Authorization

```rust
// In server/mod.rs
.route(
    "/api/v1/widgets",
    post(handlers::widgets::create_widget)
        .layer(RequireOperator::new())  // Only Operator/Admin
)
```

## File Organization

```mermaid
%% diagram-id: core-backend-api-file-tree
flowchart TD
    root["src/"]
    root --> main["main.rs — Entry point"]
    root --> server["server/"]
    server --> server_mod["mod.rs — Route setup, middleware"]
    root --> handlers["handlers/"]
    handlers --> handlers_mod["mod.rs"]
    handlers --> api_handlers["api/"]
    api_handlers --> systems["systems.rs"]
    api_handlers --> flakes["flakes.rs"]
    api_handlers --> builders["builders.rs"]
    api_handlers --> admin["admin.rs"]
    api_handlers --> api_ellipsis["..."]
    handlers --> agent["agent/"]
    agent --> heartbeat["heartbeat.rs"]
    agent --> agent_ellipsis["..."]
    root --> queries["queries/"]
    queries --> queries_mod["mod.rs"]
    queries --> qsystems["systems.rs"]
    queries --> qflakes["flakes.rs"]
    queries --> queries_ellipsis["..."]
    root --> models["models/"]
    models --> models_mod["mod.rs"]
    models --> system_model["system.rs"]
    models --> models_ellipsis["..."]
    root --> api["api/"]
    api --> api_models["models.rs — DTOs (Data Transfer Objects)"]
    root --> config["config/"]
    config --> config_mod["mod.rs — Configuration"]
    root --> error["error.rs — Error types"]
```

> **Status:** The examples and the file tree above use the original document's `src/...` paths and a `RequireOperator` middleware layer. The crate lives under `packages/default/crates/cf-server/src/` (with `handlers/api/`, `queries/`, `models/`, `api/`, `auth/`, and `bin/server.rs` where routes are registered), and the authorization extractors are in `auth/extractors.rs`. The example code was not compiled against the current code. Not reconciled in this migration.

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](../api/api-overview-errors-and-streaming.md)
- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)
