---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 03:27'
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
Repair the active `cf_config::config::DatabaseConfig` serializer used by the running cf-server. Preserve the documented NixOS Unix socket default `/run/postgresql`; do not switch to TCP. The server config module reexports this cf-config type. The similarly named `cf-server/src/config/database.rs` is not in the active module graph and must remain untouched unless the import graph proves otherwise.

The fix belongs in the shared serializer so both tokio_postgres connection validation and SQLx pool creation receive the intended configuration. Keep database clients in cf-server; cf-config may use URI encoding support but must not depend on either DB client. The regression table must call the real serializer, parse each result with both clients, and assert recovered values for socket, ordinary TCP, and space/literal-plus/reserved-character values on a nondefault port. Do not change NixOS database.host to TCP.

This repair is separately committed from TASK-490.1 test/doc work. The user explicitly authorized it on 2026-10-17. Current commits: `ca46cdd5` serializer + two-client round-trip tests; combined package/Access verification remains in progress. Keep the original startup assertion; coordinator owns combined VM and owner verification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The documented /run/postgresql socket path is serialized and parsed as a Unix socket by tokio_postgres and SQLx
- [x] #2 An ordinary network host and nondefault port round-trip through both client parsers
- [x] #3 Space, literal plus, URI delimiters and percent in username/password/database/path round-trip as actual values, not a string-only comparison
- [x] #4 Database-client dependencies remain in cf-server; cf-config has no tokio_postgres or SQLx dependency
- [ ] #5 The coordinator's combined Access scenario proves startup/migrations and records actual VM evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the same result. Evidence output is /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. Test remains failing, no production fix.

Final exact-head rerun on combined commit fc930335878279ac426510359c039dfd8961dc13 reproduces the startup failure. Evidence: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. The socket-path-to-empty-host URI defect remains unmodified.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. The user explicitly authorized this narrow production repair on 2026-10-17 and directed that it be committed separately on the TASK-490.1 foundation branch (not a separate worktree). Active type is cf_config::config::DatabaseConfig; cf-server/src/config/database.rs is an uncompiled duplicate.

Repair commit ca46cdd5 (full: verify with `git rev-parse ca46cdd5^{commit}` in the foundation worktree) changes the active cf-config serializer and adds the two-client regression table. `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests` passed (2/2). Combined Access VM proof is pending.
<!-- SECTION:NOTES:END -->
