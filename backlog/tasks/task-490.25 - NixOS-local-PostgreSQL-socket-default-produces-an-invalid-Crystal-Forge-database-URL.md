---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 03:33'
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
Repair the active `cf_config::config::DatabaseConfig` serializer used by the running cf-server. Preserve the documented NixOS Unix socket default `/run/postgresql`; do not switch to TCP. The server config module reexports this cf-config type. The similarly named `cf-server/src/config/database.rs` is not in the active module graph and remains untouched.

The shared serializer serves both tokio_postgres connection validation and the SQLx pool. Database-client dependencies remain in cf-server; cf-config uses only URI encoding support. The regression table calls the real serializer, parses each output through both clients, and asserts recovered socket, ordinary TCP, and encoding-sensitive values including space/literal-plus/reserved characters with nondefault port.

Separately committed as `ca46cdd584c7cef6071d1ee3662453faa51912b6`. Targeted two-client tests passed. The combined Access scenario passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`; evidence `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/` shows the production module's `/run/postgresql` host, successful startup and migrations. The normal Access aggregate also passed. The coordinator recorded unrelated first-boot Postgres Jobs defect TASK-492; it is not authorized under this repair.
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
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the same result. Evidence output is /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. Test remains failing, no production fix.

Final exact-head rerun on combined commit fc930335878279ac426510359c039dfd8961dc13 reproduces the startup failure. Evidence: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. The socket-path-to-empty-host URI defect remains unmodified.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. The user explicitly authorized this narrow production repair on 2026-10-17 and directed that it be committed separately on the TASK-490.1 foundation branch (not a separate worktree). Active type is cf_config::config::DatabaseConfig; cf-server/src/config/database.rs is an uncompiled duplicate.

Repair commit ca46cdd5 (full: verify with `git rev-parse ca46cdd5^{commit}` in the foundation worktree) changes the active cf-config serializer and adds the two-client regression table. `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests` passed (2/2). Combined Access VM proof is pending.

Combined runtime proof passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Exact command: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Evidence `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`; result.json passes fresh startup/migrations, and redacted generated-config-summary.txt shows `/run/postgresql`. The NixOS test uses the production server through both connection consumers; all five cases passed.
<!-- SECTION:NOTES:END -->
