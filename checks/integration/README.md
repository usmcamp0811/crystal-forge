# Integration Check

This check boots a NixOS VM running the Crystal Forge server, its embedded
PostgreSQL database, an agent, and Grafana dashboards, plus a second VM
serving a real Git repository the server polls as a watched flake. It then
runs the `cf_test` pytest suite's `database`, `dashboard`, and `server`
markers against that live stack.

## What it verifies

- Migrations apply and the server starts under systemd with the expected
  resource-control settings (`MemoryHigh`, `MemoryMax`, `MemorySwapMax`,
  slice assignment) and hardening slice limits.
- Grafana starts, its health endpoint responds, and its Crystal Forge
  PostgreSQL datasource is provisioned.
- Database-backed server behavior exercised by the `cf_test` package's
  `database`, `dashboard`, and `server` pytest markers, using commit metadata
  read from the tracked test flake (`MAIN_HEAD`, `DEVELOPMENT_HEAD`,
  `FEATURE_HEAD`, and their commit lists).

## Why it is a separate check

This is the general-purpose server/database/dashboard integration surface.
The builder is deliberately disabled here (`build.enable = false`) "to
prevent race conditions with server state transition tests" — builder
behavior has its own separate coverage inside `web-ui`'s mega VM, so state
transitions driven by a real build do not interleave with the assertions
this check makes about server-only state changes.

## Run it

```sh
nix build .#checks.x86_64-linux.integration --print-build-logs
```

Global timeout is 1200 seconds (20 minutes): database plus dashboard plus
server startup, with overhead. The VM uses a writable Nix store and is
seeded with the toplevel system closure, the core server derivation, and the
agent derivation so the test flake's NixOS configuration can build inside
the VM without network access.

## Out of scope

- OIDC authentication is not exercised here; see the `oidc-auth` check.
- The builder component never starts in this VM; no build, evaluation, or
  cache-push behavior is covered.
- The server uses the core build (`cf-server-core-drv`), not the production
  embedded-UI build, so this check never serves or exercises the web UI. See
  the `web-ui` check for that.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix (`CHECK_NAME: integration`),
so it runs on every merge request and on `main`.
