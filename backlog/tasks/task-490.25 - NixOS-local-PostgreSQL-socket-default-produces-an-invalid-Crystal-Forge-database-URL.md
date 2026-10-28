---
id: TASK-490.25
title: >-
  NixOS local PostgreSQL socket default produces an invalid Crystal Forge
  database URL
status: Review
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 14:38'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
parent_task_id: TASK-490
priority: high
type: bug
ordinal: 538000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Repair the active cf_config::config::DatabaseConfig serializer used by cf-server. Preserve the documented /run/postgresql Unix socket; do not switch to TCP. cf-server/src/config/mod.rs reexports and uses the cf-config type. The similarly named cf-server/src/config/database.rs is not in the active module graph and remains untouched.

One shared serializer serves tokio_postgres validation and SQLx pool creation. Both database clients remain in cf-server; cf-config has only URI encoding support. The regression table calls the real serializer, parses with both existing clients and asserts recovered values for the default socket path, ordinary TCP, encoding-sensitive fields and a nondefault port. The SQLx rebuilt URL also preserves the password.

The separate rebased repair commit is 52128b3a63175b712f7a83860355fdeec658a230. Final tested source 81d65be745cc9f9777d368900d01fe0f207cb052 passed both focused lifecycle-local-bootstrap and Access aggregate. Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json records 10 passed cases, 292 migrations and database.unixSocketClients=22. Targeted database_url_tests passed 2/2 including the SQLx password rebuilt-URL proof. Normal production checks stayed enabled: 1830 passed, 0 failed, 767 ignored. All own requirements are complete and verified; awaiting review in existing draft MR !335, https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. TASK-492 is unchanged and outside this repair. Earlier pending-verification statements are historical, not current blockers.
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
Awaiting review in existing draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. All five own criteria are verified; the active repair lock is released for review. Not merged or Done.

Final source 81d65be745cc9f9777d368900d01fe0f207cb052:
- `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests`: PASS 2/2, including SQLx password preserved in the rebuilt URL.
- `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`: PASS.
- `nix build .#checks.x86_64-linux.integration-access -L`: PASS.
- Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json: 10 passed cases, 292 migrations, database.unixSocketClients=22, exact admin/session receipts.
- Normal production checks remained enabled in the VM build: 1830 passed, 0 failed, 767 ignored. Existing okf validation: 58 tests, 192 concepts, 216 Mermaid blocks passed.
Execution results were supplied by the coordinator; result.json was directly read during task finalization. No check was rerun for these task-record edits.

Stable scenario artifact: https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json

### Historical
The user authorized the narrow active cf_config::config::DatabaseConfig repair on the foundation branch as a separate commit on 2026-10-17; the duplicate cf-server/src/config/database.rs was outside the active module graph. The former claude-sonnet-5-5 implementation lock is no longer active.

At 39a6a54b651a8b77fc3416b73195e363a782fa2e startup failed before migrations/HTTP; evidence vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt and crystal-forge.journal lines 72-79. fc930335878279ac426510359c039dfd8961dc13 repeated the failure; evidence 1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. The serializer emitted an empty-host URI for /run/postgresql while PostgreSQL listened on the socket. The old test-only prohibition preceded separate authorization; switching to TCP would not prove the default socket.

Pre-rebase ca46cdd5 added serializer/two-client regression coverage; the targeted command passed 2/2. Combined VM proof was pending at that historical point only. b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56 later passed the focused gate (five bootstrap cases); evidence xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 repeated the pass. Rebase onto origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064 rewrote the repair to 52128b3a63175b712f7a83860355fdeec658a230; bba718befbc183863781656df2df272e5f735d5a passed with runtime riky1mm0nn185wb26abw7zzshnbml0fp, socket startup and 292 migrations. The final 81d65be result above adds the password reconstruction and session-subset evidence. TASK-492 remains an independent startup-order defect.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Socket URL repair ready for review
The active cf-config serializer preserves Unix sockets and encoding-sensitive connection values for tokio_postgres and SQLx. Added proof that the SQLx rebuilt URL retains the password; database-client dependencies remain in cf-server.

At 81d65be745cc9f9777d368900d01fe0f207cb052, targeted database_url_tests passed 2/2. Focused Access and aggregate passed with 10 cases, 292 migrations and database.unixSocketClients=22 (runtime i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh). Normal production checks remained enabled: 1830 passed, 0 failed, 767 ignored. All five criteria are verified.

Review: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. TASK-492 is unchanged and outside scope. Awaiting review; not merged or Done.
<!-- SECTION:FINAL_SUMMARY:END -->
