---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 14:01'
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
Repair the active `cf_config::config::DatabaseConfig` serializer used by cf-server. Preserve the documented `/run/postgresql` Unix socket; do not switch to TCP. `cf-server/src/config/mod.rs` reexports and uses the cf-config type. The similarly named `cf-server/src/config/database.rs` is not in the active module graph and remains untouched.

One shared serializer serves tokio_postgres validation and SQLx pool creation. Keep both database clients in cf-server; cf-config has only URI encoding support. Regression table calls the real serializer, parses with both existing clients, and asserts recovered values for the default socket path, ordinary TCP, and space/literal-plus/reserved values with a nondefault port.

Repair is separate commit `52128b3a63175b712f7a83860355fdeec658a230` after rebase. Targeted table passed 2/2 before rebase. Combined Access scenario and owner aggregate passed on rebased source `bba718befbc183863781656df2df272e5f735d5a`; evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/` shows `/run/postgresql`, successful startup and migrations (292). Normal server package and parser checks passed before rebase; VM build on rebased source also succeeds. TASK-492 is a separate unauthorized startup-order issue.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The documented /run/postgresql socket path is serialized and parsed as a Unix socket by tokio_postgres and SQLx
- [x] #2 An ordinary network host and nondefault port round-trip through both client parsers
- [x] #3 Space, literal plus, URI delimiters and percent in username/password/database/path round-trip as actual values, not a string-only comparison
- [x] #4 Database-client dependencies remain in cf-server; cf-config has no tokio_postgres or SQLx dependency
- [x] #5 The coordinator's combined Access scenario proves startup/migrations and records actual VM evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current verification: separate rebased repair commit 52128b3a63175b712f7a83860355fdeec658a230 is included in tested foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064. The focused Access scenario and owner aggregate passed. Runtime riky1mm0nn185wb26abw7zzshnbml0fp records /run/postgresql startup and 292 successful migrations. No pending combined verification remains from the earlier observations. Status is preserved for the coordinator.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. User authorized this narrow repair on the foundation branch as a separate commit on 2026-10-17. Active type is cf_config::config::DatabaseConfig; cf-server/src/config/database.rs is an uncompiled duplicate.

### Historical
At combined source 39a6a54b651a8b77fc3416b73195e363a782fa2e, startup failed before migrations/HTTP; evidence vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. The exact-head fc930335878279ac426510359c039dfd8961dc13 repeat failed identically; evidence 1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. At those revisions the serializer emitted an empty-host URI for /run/postgresql; PostgreSQL itself listened on that socket. The old test-only prohibition applied before the separate user authorization; switching to TCP would not prove the default socket path.

Pre-rebase repair ca46cdd5 added the real serializer/two-client regression table. `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests` passed 2/2. The statement that combined VM proof was pending described that point in time only.

Combined runtime proof subsequently passed at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56 with `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; evidence xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/ (all five cases). A pre-rebase ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 repeat was also recorded as passing. Those observations preceded the current bba718be verification. Rebase rewrote the repair to 52128b3a63175b712f7a83860355fdeec658a230. Evidence identifiers above name outputs beneath the Nix store; the exact current runtime is recorded in Description. TASK-492 is an independent startup-order defect outside this repair.
<!-- SECTION:NOTES:END -->
