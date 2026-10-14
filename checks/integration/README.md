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

The driver timeout is 1200 seconds (20 minutes). It starts after prerequisite
builds and before VM startup; prerequisite builds are outside this budget.
Driver cleanup can continue after the test timer is released. The VM uses a writable Nix store and is
seeded with the toplevel system closure, the core server derivation, and the
agent derivation so the test flake's NixOS configuration can build inside
the VM without network access.

## Runtime diagnostics

The fixture sets the module's bare log level to `info`. Server and enabled
SQLx-owning workers override `RUST_LOG` with
`info,crystal_forge=debug,server=debug,sqlx=warn`. The agent retains `info`.
Runtime checks verify the exact filter in each process without printing other
environment values. SQLx DEBUG statements and binds are suppressed; SQLx
warnings, including slow-query warnings, can still appear.

Each pytest phase emits START and END with monotonic elapsed time, the original
exit status (or exception), and completed outcome counts. Each test emits actual
setup/call/teardown outcomes and completion elapsed time. Added progress records
omit parameter values and failure bodies. These events are not periodic
heartbeats and do not change test selection, skips, assertions, or timeouts.
Phase END runs in `finally`; pytest failures retain their original exit status.

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

CI runs this same attribute through `packages/ci/public-cache-build.sh` with
`--builders '' --max-jobs 1 --cores 2`. Only `integration`, `attic-metrics`, and
`attic-publication` receive these local execution arguments. Other jobs retain
inherited builder controls; production builders are unaffected.

Pipeline 2944979038 failed before observed VM execution or assertions: remote
`QueryValidPaths` closed its socket for integration and Attic publication, and
remote `FetchNars` reported a missing publication driver for Attic metrics.
These remote execution failures do not prove a server certificate defect.
The wrapper's substitution fallback retains builders and does not automatically
retry remote builder errors locally. The scoped arguments select local execution
without changing TLS verification, signatures, test attributes, or assertions.
