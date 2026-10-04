---
type: Operator Guide
title: Development environment commands
description: Lists the nix develop shell commands for starting the local service stacks, running the agent, building and serving the web UI, running test suites, and refreshing documentation screenshots; open it when starting local development.
tags:
  - crystal-forge
  - development
  - testing
  - web-ui
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:25:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Development environment commands

> **Status:** partial. This concept holds the `Development`, `Web UI Development`, and `Testing` sections of the repository `README.md`. Most commands match the dev shell (`shells/default/default.nix` defines `server-stack-mock` and `oidc-stack` aliases; `packages/devScripts/default.nix` defines `run-agent`, `run-server`, `run-builder`, and the stacks). Verification candidates:
>
> - `trunk serve` in `packages/web-ui`: the package has `Dioxus.toml` and no `Trunk.toml`. The current web UI tooling is described in [Frontend development overview](../ui/frontend-development-overview.md).
> - The `--dev` flags of `run-server` and `run-builder` (`packages/devScripts/default.nix`).
> - The `nix run .#devScripts.oidc-stack` flake path and the existence of a `db-only` and `server-stack` command.
> - The check attribute names (`checks/` and `flake.nix`).
> - Screenshot refresh: `result/screenshots/*.png` and the `CF_UI_TEST_PROFILE=full` variable in `checks/web-ui/tests/integration-test.js`.

## Development

```bash
# Enter development shell
nix develop

# Start core services (choose one)
server-stack up        # Postgres + server + builder
server-stack-mock up   # Postgres + server + mock builder/eval (fast UI testing)
db-only up             # Postgres only

# Start with local OIDC (Keycloak)
nix run .#devScripts.oidc-stack -- up

# Run agent
run-agent

# Development mode with live reload
run-server --dev
run-builder --dev
```

## Web UI Development

```bash
# Build web UI
nix build .#packages.x86_64-linux.web-ui

# Run with hot reload
cd packages/web-ui
trunk serve
```

## Testing

```bash
# All tests
nix flake check

# Specific suites
nix build .#checks.x86_64-linux.database
nix build .#checks.x86_64-linux.server
nix build .#checks.x86_64-linux.builder
nix build .#checks.x86_64-linux.web-ui

# Refresh docs screenshots from web-ui check output
nix build .#checks.x86_64-linux.web-ui
cp result/screenshots/*.png docs/screenshots/

# Optional: full screenshot pass (not ci_fast subset)
CF_UI_TEST_PROFILE=full node checks/web-ui/tests/integration-test.js \
  http://127.0.0.1:3000 docs/screenshots
```

## Related concepts

- [Local development workflow, patterns, and common tasks](local-development-workflow.md)
- [Mock execution mode](mock-execution-mode.md)
- [Web UI check](../testing/web-ui-check.md)
- [Flake checks](../testing/flake-checks.md)
- [Frontend development overview](../ui/frontend-development-overview.md)
- [Contributing guide](contributing-guide.md)
