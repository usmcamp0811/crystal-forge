---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 02:15'
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
The normal NixOS module defaults local-database=true and database.host=/run/postgresql, documented as a local socket. The generated TOML contains [database] host = "/run/postgresql". cf-config DatabaseConfig::to_url formats the value as a PostgreSQL authority URI with no host, and production cf-server-drv exits before migrations with `both host and hostaddr are missing`.

Reproduced by the native Access VM at final combined commit fc930335878279ac426510359c039dfd8961dc13. Command: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Evidence: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal (server failure at 7.451s). Expected: the documented local socket configuration starts the server and applies migrations. Actual: URI parser rejects empty host. The test assertion remains failing. This is a product fix outside TASK-490.1; do not change it in the test overhaul.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The supported local PostgreSQL socket configuration starts the production server without an empty-host URI failure
- [ ] #2 Fresh database migrations complete and server readiness is observable through the normal NixOS module
- [ ] #3 A regression test proves the socket host is transported as a socket path rather than an authority URI host
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the same result. Evidence output is /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. Test remains failing, no production fix.

Final exact-head rerun on combined commit fc930335878279ac426510359c039dfd8961dc13 reproduces the startup failure. Evidence: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. The socket-path-to-empty-host URI defect remains unmodified.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. The user explicitly authorized this narrow production repair on 2026-10-17 and directed that it be committed separately on the TASK-490.1 foundation branch (not a separate worktree). Active type is cf_config::config::DatabaseConfig; cf-server/src/config/database.rs is an uncompiled duplicate.
<!-- SECTION:NOTES:END -->
