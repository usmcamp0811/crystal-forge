---
type: Operator Guide
title: Mock execution mode (dev only)
description: Describes the deterministic mock execution mode for local eval and build queue validation, its safety restrictions, configuration, simulated behavior, and intended use.
tags:
  - crystal-forge
  - development
  - mock-mode
  - configuration
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/mock-execution-mode.md at commit 3b23d36f"
    title: "Mock Execution Mode (Dev Only)"
---

# Mock Execution Mode (Dev Only)

Crystal Forge supports a deterministic mock execution mode to speed up local workflow validation for eval/build queue behavior.

## Safety Model

- Default mode is `real`.
- `mock` mode is only allowed when `server.auth_mode = "local"`.
- `mock` mode requires a local database host (`localhost`, `127.0.0.1`, or `::1`).

This prevents accidental mock execution in production.

> **Status:** implemented. `packages/default/crates/cf-config/src/config/server.rs`
> rejects `execution_mode = "mock"` when `auth_mode` is not `local`. The local
> database host check exists as a `localhost` / `127.0.0.1` / `::1` match in
> `packages/default/crates/cf-server/src/bin/server.rs` and
> `packages/default/crates/cf-builder/src/bin/builder.rs`.

## Configuration

Set the following in your config (or equivalent env-backed config):

```toml
[server]
auth_mode = "local"
execution_mode = "mock"
```

To return to real execution:

```toml
[server]
execution_mode = "real"
```

## What Mock Mode Simulates

- Eval phase:
  - deterministic per-system progression (~30s total per eval run with default 3 systems)
  - streaming eval logs and status updates
  - deterministic mixed system outcomes (includes policy-failed systems)
  - derivation rows are inserted and moved to `DryRunComplete`
- Build phase:
  - deterministic fast build progression in both API-builder and legacy-builder modes
  - deterministic mixed outcomes (includes failed builds)
  - synthetic store paths only for successful mock builds
  - signing and cache-push side effects are skipped for mock API builds
  - normal job completion API path remains in use

## UI Indicator

- Evaluations view shows a `MOCK MODE` badge when the server reports mock execution mode.

> **Status:** unverified detail. `packages/web-ui/src/components/layout/dev_banner.rs`
> reads `execution_mode` from the server status and renders environment marker
> banners when the mode equals `mock`. The migration found no `MOCK MODE`
> string in the Evaluations view code at the migration base commit.

## Intended Use

- Fast validation of queue ordering, websocket log UX, retries, and state transitions.
- Reproduce UI/queue bugs without waiting for real `nix-eval-jobs` and `nix build` runtime.
- Manual flake sync in mock mode injects a synthetic commit when source has no new commits, so each sync can drive a fresh eval/build run.
- `server-stack-mock` bootstraps a local admin account on startup: username `admin`, password `password`.
- `server-stack-mock` runs the builder in API mode only (legacy direct-DB builder mode is deprecated).
