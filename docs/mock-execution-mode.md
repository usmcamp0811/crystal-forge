# Mock Execution Mode (Dev Only)

Crystal Forge supports a deterministic mock execution mode to speed up local workflow validation for eval/build queue behavior.

## Safety Model

- Default mode is `real`.
- `mock` mode is only allowed when `server.auth_mode = "dev"`.
- In release builds, the server/builder startup hard-fails if `server.execution_mode = "mock"`.

This prevents accidental mock execution in production.

## Configuration

Set the following in your config (or equivalent env-backed config):

```toml
[server]
auth_mode = "dev"
execution_mode = "mock"
```

To return to real execution:

```toml
[server]
execution_mode = "real"
```

## What Mock Mode Simulates

- Eval phase:
  - deterministic per-system progression
  - streaming eval logs and status updates
  - derivation rows are inserted and moved to `DryRunComplete`
- Build phase:
  - deterministic fast build logs
  - successful completion path with synthetic store path
  - normal job completion API path remains in use

## UI Indicator

- Evaluations view shows a `MOCK MODE` badge when the server reports mock execution mode.

## Intended Use

- Fast validation of queue ordering, websocket log UX, retries, and state transitions.
- Reproduce UI/queue bugs without waiting for real `nix-eval-jobs` and `nix build` runtime.
