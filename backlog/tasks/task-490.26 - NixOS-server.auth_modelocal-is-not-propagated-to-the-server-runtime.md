---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: Review
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 14:38'
labels:
  - bug
  - nixos
  - auth
  - testing
dependencies: []
references:
  - TASK-490.1
  - TASK-490.25
  - modules/nixos/crystal-forge/default.nix
  - packages/default/crates/cf-config/src/config/server.rs
  - docs/knowledge/operations/authentication-modes-and-oidc-configuration.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
parent_task_id: TASK-490
priority: high
type: bug
ordinal: 539000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Propagate services.crystal-forge.server.auth_mode to the effective production server configuration through the normal NixOS unit. AUTH_MODE comes from the option for local and OIDC; OIDC-only variables remain conditional and existing EnvironmentFile precedence is preserved. No test-only override or authentication policy change is used.

The separate rebased repair commit is 76e514644505f232e4e47f085db801913d8449ab. Targeted NixOS evaluation verified both local/OIDC AUTH_MODE and presence/absence of OIDC-only variables. Final tested source 81d65be745cc9f9777d368900d01fe0f207cb052 passed focused lifecycle-local-bootstrap and Access aggregate. The first five cases prove empty local setup-status, embedded-UI first-admin registration/sign-in/reload, persistent admin/session and wrong-password rejection without a new session; five subsequent session cases also pass. Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json records 10 passed cases, 292 migrations, database.unixSocketClients=22 and exact identity receipts. Normal production checks remained enabled: 1830 passed, 0 failed, 767 ignored.

All four own requirements are complete and verified. Awaiting review in existing draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. TASK-492 is unchanged and outside this repair. Earlier pending runtime-verification statements are historical, not current blockers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The normal module's local selection produces systemd AUTH_MODE=local and the effective server reports local mode
- [x] #2 The normal module's OIDC selection produces systemd AUTH_MODE=oidc and preserves OIDC-only variables/selection
- [x] #3 Targeted evaluation verifies both module choices without a test-only override
- [x] #4 The combined Access VM proves empty-database setup-status, real local registration/sign-in, persistent admin/session and rejected wrong password with no new session
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Awaiting review in existing draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. All four criteria are verified; the active repair lock is released for review. Not merged or Done.

Final source 81d65be745cc9f9777d368900d01fe0f207cb052:
- `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`: PASS.
- `nix build .#checks.x86_64-linux.integration-access -L`: PASS.
- Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json: 10 passed cases (five bootstrap then five sessions), 292 migrations, database.unixSocketClients=22 and exact admin/session receipts. Local mode comes from the normal module, with no test-only override.
- Targeted database_url_tests passed 2/2 including the SQLx rebuilt password URL; normal production checks stayed enabled in the VM build (1830 passed, 0 failed, 767 ignored). Existing okf validation passed 58 tests, 192 concepts and 216 Mermaid blocks.
Execution outcomes were supplied by the coordinator; result.json was read directly during task finalization. No checks were rerun for these task-record edits.

Scenario artifact: https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json
Browser artifact: https://gitlab.com/-/project/70402481/uploads/5d551029e97b8b3dd534f312deb35b5f/result.json

### Historical
The user authorized the narrow production repair as a separate foundation-branch commit on 2026-10-17. The former claude-sonnet-5-5 implementation lock is no longer active.

At 39a6a54b651a8b77fc3416b73195e363a782fa2e generated [server] omitted auth_mode and AUTH_MODE was unset; evidence vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt. fc930335878279ac426510359c039dfd8961dc13 repeated that observation; evidence 1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. The config default would select oidc, but .25 prevented migrations/routes, so no OIDC HTTP response was observed. The old product-fix prohibition preceded separate authorization; no test-only AUTH_MODE override was introduced.

Pre-rebase 136348e8 propagated module-derived AUTH_MODE for local/OIDC, retained conditional OIDC variables and EnvironmentFile precedence. Targeted evaluation returned {"local":"local","oidc":"oidc"}. Combined runtime proof was pending at that historical point only.

b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56 later passed local setup-status and first Admin registration/sign-in/reload plus wrong-password rejection; evidence xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/ includes result.json, browser/bootstrap-sign-in/result.json, signed-in-after-reload.png and redacted generated config. ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 repeated the pass; evidence 0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. Rebase onto origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064 rewrote the repair to 76e514644505f232e4e47f085db801913d8449ab; bba718befbc183863781656df2df272e5f735d5a passed with runtime riky1mm0nn185wb26abw7zzshnbml0fp. The final result above also includes the selected session subset. TASK-492 remains independent.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Auth-mode propagation ready for review
The production NixOS unit passes module-selected AUTH_MODE for local and OIDC while preserving conditional OIDC variables and EnvironmentFile precedence. No test-only auth override or authentication policy change is used.

Targeted module evaluation verified both choices. At 81d65be745cc9f9777d368900d01fe0f207cb052, focused Access and aggregate passed with 10 cases, including first-admin local bootstrap/session behavior and the selected session subset. Runtime i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh records 292 migrations and exact identity receipts. Enabled production checks reported 1830 passed, 0 failed, 767 ignored. All four criteria are verified.

Review: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. TASK-492 remains unchanged. Awaiting review; not merged or Done.
<!-- SECTION:FINAL_SUMMARY:END -->
