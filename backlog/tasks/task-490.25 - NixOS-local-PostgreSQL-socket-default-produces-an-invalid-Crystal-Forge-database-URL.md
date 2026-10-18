---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 02:48'
labels:
  - bug
  - nixos
  - database
  - testing
dependencies: []
references:
  - TASK-490.1
  - TASK-491
  - modules/nixos/crystal-forge/default.nix
  - packages/default/crates/cf-config/src/config/database.rs
parent_task_id: TASK-490
priority: high
type: bug
ordinal: 538000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current authorized repair (2026-10-18; supersedes the earlier prohibition below): active `cf_config::config::DatabaseConfig::to_url` percent-encodes URI components and preserves `/run/postgresql` as a Unix socket. Rebased separate commit `52128b3a` adds the real-serializer regression table parsed by both tokio_postgres and SQLx. The targeted tests passed before rebase; the combined Access VM passed after rebase at bba718befbc183863781656df2df272e5f735d5, including socket startup and migrations. Keep the startup assertion.

On the TASK-490.1 native Access startup slice, the controller uses the normal NixOS module defaults: local-database=true and database.host=/run/postgresql (documented as the local socket path). The generated config reports [database] host = "/run/postgresql". cf-config DatabaseConfig::to_url formats that string as `postgres://user:password@/run/postgresql:5432/database`, which has no URI host. The real production cf-server-drv exits during startup with `invalid configuration: both host and hostaddr are missing`, before migrations, bootstrap, or HTTP routes.

Observed on final combined test commit fc930335878279ac426510359c039dfd8961dc13 (application baseline 33de73b013b735304dfac9c69745a074a3e76ebd): `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` fails; scenario evidence /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt shows host /run/postgresql, port 5432, database crystal_forge, user crystal_forge, password redacted. Journal shows PostgreSQL listening on /run/postgresql and server startup error at 7.451s (`crystal-forge.journal`, lines 72-79). Expected: the documented local-database socket configuration starts the server and applies migrations. Actual: URI parsing rejects its empty host.

Test-only overhaul must not fix production code. Keep the Access scenario's startup assertion executable and failing until a separately authorized product fix. Do not replace the configuration with an AUTH_MODE workaround. Production correction needs its own task/review; TASK-490.1 may use only a documented non-socket database host as test setup if the owner accepts that it no longer proves the module's default socket path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The supported local PostgreSQL socket configuration starts the production server without an empty-host URI failure
- [x] #2 Fresh database migrations complete and server readiness is observable through the normal NixOS module
- [x] #3 A regression test proves the socket host is transported as a socket path rather than an authority URI host
- [x] #4 The real serializer round-trips socket, network and encoding-sensitive fields through both tokio_postgres and SQLx without adding database clients to cf-config
- [x] #5 The coordinator records the combined Access VM result and evidence
<!-- AC:END -->
