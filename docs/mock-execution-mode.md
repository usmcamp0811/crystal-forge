# Mock Execution Mode (Dev Only)

Crystal Forge supports a deterministic mock execution mode to speed up local workflow validation for eval/build queue behavior.

## Safety Model

- Default mode is `real`.
- `mock` mode is only allowed when `server.auth_mode = "local"`.
- `mock` mode requires a local database host (`localhost`, `127.0.0.1`, or `::1`).

This prevents accidental mock execution in production.

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

## Intended Use

- Fast validation of queue ordering, websocket log UX, retries, and state transitions.
- Reproduce UI/queue bugs without waiting for real `nix-eval-jobs` and `nix build` runtime.
- Manual flake sync in mock mode injects a synthetic commit when source has no new commits, so each sync can drive a fresh eval/build run.
- `server-stack-mock` bootstraps a local admin account on startup: username `admin`, password `password`.
- `server-stack-mock` runs the builder in API mode only (legacy direct-DB builder mode is deprecated).
