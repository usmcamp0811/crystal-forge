# Server Regressions Check

This check compiles and runs a curated set of `cf-server` Cargo integration
and library tests against a disposable PostgreSQL instance, using
`postgresqlTestHook`. It is a Nix `buildRustPackage` derivation whose
`checkPhase` is the entire point of the build; nothing under `installPhase`
is meant to be consumed as a package.

## What it verifies

- A populated pre-TASK-433 (migration `0232` and earlier) database upgrades
  cleanly through the full migration set, and every populated row family
  (systems, policies, CVE scans, desired targets, compliance bundle
  assignments and versions, system states, attention occurrences, and
  notifications) survives the upgrade with the expected post-migration
  columns and indexes present. This is an upgrade rehearsal, not just a
  from-empty migration run.
- A focused list of `cf-server` Cargo integration test binaries: assignment
  semantics, composite policy, evidence-for-ATO, framework version ID
  lifecycle, policy counts, policy editor phase 2, POA&M workflows, TASK-433
  assignment visibility and CSRF, and time-window policy behavior.
- A curated list of `#[ignore]`d library tests covering POA&M
  authorization/CSRF, setup-wizard progress counting, policy-requirement
  identity hydration, notification queries and email tasks, POA&M overdue
  attention reconciliation, composite AC3 pure validation and
  JSON/TOML/CF-native interchange, the composite AC3 Nix-executor matrix,
  resolver enforcement, immutable deletion lifecycle, trusted STIG
  mapping-pair rules, bundle requirement baseline lifecycle, bundle summary
  query-count bounds, and system agent key rotation authorization/audit
  behavior.

## Why it is a separate check

`packages/default/default.nix` builds and tests `cf-server` with
`--lib --bins` only; Cargo integration targets under
`crates/cf-server/tests/` are not compiled by `nix build .#server`, and the
NixOS `integration` check runs the Python `cf_test` suite, not these Rust
targets. This check exists to run PostgreSQL-backed Rust regressions that
would otherwise never execute anywhere. It is intentionally a curated list,
not `--all-targets --ignored`: the repository also contains manual, slow, and
environment-specific ignored tests that do not belong in this gate.

## Run it

```sh
nix build .#checks.x86_64-linux.server-regressions --print-build-logs
```

No VM is booted. `postgresqlTestHook` starts a local disposable PostgreSQL
instance for the build sandbox; the test role has `LOGIN SUPERUSER CREATEDB`
so migration and immutability-trigger tests can exercise the same DDL
privilege level as production migrations. `SQLX_OFFLINE=true` is set, so
SQLx's compile-time query verification uses committed offline metadata
rather than a live database connection during compilation.

## Out of scope

- General server/database/dashboard behavior belongs to the `integration`
  check; this check is deliberately narrow and additive to it.
- This is not a general "run every ignored test" gate. Extending the list
  requires judgment about whether a test is a genuine data-integrity
  regression versus a slow or environment-specific manual test.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix
(`CHECK_NAME: server-regressions`), so it runs on every merge request and on
`main`.
