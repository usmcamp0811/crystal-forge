---
type: Operator Guide
title: Development environment commands
description: Lists the nix develop shell commands for starting the local service stacks, running the agent, building and serving the web UI, running test suites, and generating UI screenshots; open it when starting local development.
tags:
  - crystal-forge
  - development
  - testing
  - web-ui
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Development environment commands

All commands run inside the development shell. Enter it first:

```bash
nix develop
```

The shell defines the aliases below (`shells/default/default.nix`). The aliases call the scripts in `packages/devScripts/default.nix`.

## Development

```bash
# Start core services (choose one)
full-stack up          # Postgres + server + agent (process-compose)
server-stack up        # Postgres + server + builder (process-compose)
server-stack-mock up   # Postgres + server + builder with mock eval/build execution (fast UI testing)
oidc-stack up          # Postgres + Keycloak + server in OIDC mode
db-only up             # Postgres only

# Run the agent (runs with sudo; the server must run first)
run-agent
run-agent --dev        # run the agent from local code

# Run the server or builder directly
run-server             # packaged server binary
run-server --dev       # server from local code (nix run .#server)
nix run .#devScripts.runBuilder -- --dev   # builder from local code (nix run .#builder)
```

`run-builder` has no shell alias. Call it through `nix run .#devScripts.runBuilder`, as shown. For a builder that talks to a running server, `start-builder-api` prompts for the builder credentials and starts an API builder.

Other helpers: `simulate-push` simulates a webhook push event, and `sqlx-prepare` and `sqlx-refresh` re-run SQLx preparation. Run `sqlx-refresh` only against the isolated local database that the dev stack started. It drops the database.

## Web UI Development

The web UI uses Dioxus. The development servers use `dx serve`, not Trunk.

```bash
# Build the production web UI
nix build .#packages.x86_64-linux.web-ui

# One command: starts the database, seeds fixture data, starts the server
# in the background, and runs the Dioxus dev server in the foreground
run-ui-dev

# Frontend only: pins wasm-bindgen, builds Tailwind CSS, and runs
# `dx serve` from packages/web-ui. It needs the server that run-ui-dev starts.
run-ui-frontend
```

`run-ui-frontend` serves the UI at `http://localhost:8080` with hot reload. It proxies `/api` to the Crystal Forge server on `http://localhost:3445`. The server in `run-ui-dev` uses `local` authentication with a bootstrap `admin` user. Both scripts require Dioxus CLI `0.7.3` and fail with a version message otherwise.

## Testing

```bash
# All flake checks (long; it boots VMs)
nix flake check

# Specific checks (see Flake checks for the complete list)
nix build .#checks.x86_64-linux.server-regressions   # PostgreSQL-backed Rust regression tests
nix build .#checks.x86_64-linux.integration          # VM: server, database, and dashboard behavior
nix build .#checks.x86_64-linux.web-ui               # VM: production UI through a real browser
nix build .#checks.x86_64-linux.ui-screenshots       # fixture-driven screenshots, no backend
nix build .#checks.x86_64-linux.okf-knowledge        # knowledge corpus validation

# Python tests (database, server, builder) against the dev database
# (start `server-stack up` first)
nix run .#cf-test-suite.runTests -- -vvv -m database
```

The checks named `database`, `server`, and `builder` do not exist. Their tests run through `integration` or through `cf-test-suite` markers. See [Flake checks](../testing/flake-checks.md) for what each check covers.

### Screenshots

`ui-screenshots` writes one PNG per view and theme into its build output. Build it and inspect `result/`:

```bash
nix build .#checks.x86_64-linux.ui-screenshots --print-build-logs
```

The `web-ui` check runs a `ci_fast` profile by default. The `CF_UI_TEST_PROFILE` variable selects the profile of `checks/web-ui/tests/integration-test.js`. See the [Web UI check](../testing/web-ui-check.md) for the profile rules.

## Related concepts

- [Local development workflow, patterns, and common tasks](local-development-workflow.md)
- [Mock execution mode](mock-execution-mode.md)
- [Web UI check](../testing/web-ui-check.md)
- [Flake checks](../testing/flake-checks.md)
- [Frontend development overview](../ui/frontend-development-overview.md)
- [Contributing guide](contributing-guide.md)
