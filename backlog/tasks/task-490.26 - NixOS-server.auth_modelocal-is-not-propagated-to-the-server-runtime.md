---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 14:01'
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
parent_task_id: TASK-490
priority: high
type: bug
ordinal: 539000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Propagate `services.crystal-forge.server.auth_mode` to the effective production server configuration through the normal NixOS unit. Set `AUTH_MODE` from the option for local and OIDC; keep OIDC-specific variables conditional and preserve existing EnvironmentFile precedence. Do not add a test-only override or change authentication policy.

Repair is separate commit `76e514644505f232e4e47f085db801913d8449ab` after rebase. Targeted NixOS evaluation verified local/OIDC AUTH_MODE and presence/absence of OIDC-only variables. The combined Access scenario and owner aggregate passed on rebased source `bba718befbc183863781656df2df272e5f735d5a`: setup-status reports local and requires_setup on an empty DB; the production embedded UI registers/signs in the first administrator; reload preserves the session; wrong password is rejected without a new session. Evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. TASK-492 is a separate unauthorized startup-order issue.
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
Current verification: separate rebased repair commit 76e514644505f232e4e47f085db801913d8449ab is included in tested foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064. Focused Access scenario and owner aggregate passed; runtime riky1mm0nn185wb26abw7zzshnbml0fp confirms local setup-status, first-admin production-browser workflow/session assertions and 292 migrations. Earlier pending-verification statements are historical observations. Status is preserved for the coordinator.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. User authorized this narrow production repair as a separate foundation-branch commit on 2026-10-17.

### Historical
At 39a6a54b651a8b77fc3416b73195e363a782fa2e, generated [server] omitted auth_mode and the unit reported AUTH_MODE=unset. Evidence vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt. Exact-head fc930335878279ac426510359c039dfd8961dc13 repeated that observation; evidence 1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. The config default would select oidc, but .25 prevented migrations/route registration, so no OIDC HTTP response was observed. The old product-fix prohibition preceded separate user authorization; no test-only AUTH_MODE override was used.

Pre-rebase repair 136348e8 propagated module-derived AUTH_MODE for local/OIDC while retaining conditional OIDC variables and EnvironmentFile precedence. Targeted module evaluation returned {"local":"local","oidc":"oidc"}. Combined runtime proof was pending at that point only.

The combined proof later passed at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56: empty-DB setup-status local/requires_setup, embedded-UI first Admin registration/sign-in/reload, persistent session and rejected wrong password without a new session. Evidence xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/ includes result.json, browser/bootstrap-sign-in/result.json, signed-in-after-reload.png and redacted generated config. ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 also passed; evidence 0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. Rebase rewrote the repair to 76e514644505f232e4e47f085db801913d8449ab and produced the current bba718be result. Evidence identifiers above name outputs beneath the Nix store; Description records the exact current runtime. TASK-492 is independent and outside this repair.
<!-- SECTION:NOTES:END -->
