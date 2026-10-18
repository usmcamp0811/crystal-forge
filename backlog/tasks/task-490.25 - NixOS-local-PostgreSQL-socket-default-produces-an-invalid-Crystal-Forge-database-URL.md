---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 13:47'
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
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the same result. Evidence output is /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. Test remains failing, no production fix.

Final exact-head rerun on combined commit fc930335878279ac426510359c039dfd8961dc13 reproduces the startup failure. Evidence: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. The socket-path-to-empty-host URI defect remains unmodified.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. The user explicitly authorized this narrow production repair on 2026-10-17 and directed that it be committed separately on the TASK-490.1 foundation branch (not a separate worktree). Active type is cf_config::config::DatabaseConfig; cf-server/src/config/database.rs is an uncompiled duplicate.

Repair commit ca46cdd5 (full: verify with `git rev-parse ca46cdd5^{commit}` in the foundation worktree) changes the active cf-config serializer and adds the two-client regression table. `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests` passed (2/2). Combined Access VM proof is pending.

Combined runtime proof passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Exact command: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Evidence `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`; result.json passes fresh startup/migrations, and redacted generated-config-summary.txt shows `/run/postgresql`. The NixOS test uses the production server through both connection consumers; all five cases passed.

Exact final branch-head repeat at `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5` also passed the combined Access socket startup/migration case; evidence `/nix/store/0aavcw4vk32c83gljg8y4mz7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. (First exact store path remains recorded in the description; no further rerun needed.)

Rebase to origin/dev `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064` rewrote this repair commit to `52128b3a63175b712f7a83860355fdeec658a230`. The focused Access scenario and owner aggregate both passed on rebased source `bba718befbc183863781656df2df272e5f735d5a`; evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`, including 292 migrations and `/run/postgresql` configuration.
<!-- SECTION:NOTES:END -->
