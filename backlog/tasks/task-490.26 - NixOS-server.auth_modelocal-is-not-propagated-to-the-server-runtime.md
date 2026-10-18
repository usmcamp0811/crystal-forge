---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 13:47'
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
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the generated-config mismatch: summary shows `[server]` with no auth_mode and systemd AUTH_MODE=unset. The server failed earlier on TASK-490.25, so this VM did not observe an auth-mode response or route registration. Evidence is in /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt.

Final exact-head rerun on combined commit fc930335878279ac426510359c039dfd8961dc13 confirms generated [server] omits auth_mode and systemd AUTH_MODE is unset: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt. The DB URL defect prevented route registration; runtime auth response remains unobserved.

LOCK: claude-sonnet-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. The user explicitly authorized this narrow production repair on 2026-10-17 and directed that it be committed separately on the TASK-490.1 foundation branch (not a separate worktree).

Repair commit 136348e8 integrates the module-derived AUTH_MODE in the production server unit for both auth choices. Targeted NixOS evaluation returned `{"local":"local","oidc":"oidc"}`. Combined Access VM proof remains pending.

Combined runtime proof passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Setup-status returned local mode with `requires_setup=true` on the empty database; the embedded-UI browser registered the first Admin, signed in, reloaded with the persistent session, and the wrong password was rejected without creating another session. Evidence: `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/` (`result.json`, `browser/bootstrap-sign-in/result.json`, `browser/bootstrap-sign-in/signed-in-after-reload.png`, generated-config-summary.txt). No test-only environment override was used.

Exact final branch-head repeat at `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5` passed the local-mode setup-status and browser/session assertions; evidence is `/nix/store/0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. No further rerun needed.

Rebase to origin/dev `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064` rewrote this repair commit to `76e514644505f232e4e47f085db801913d8449ab`. The focused Access scenario and owner aggregate passed on rebased source `bba718befbc183863781656df2df272e5f735d5a`; runtime setup-status and browser assertions confirm effective local mode, with evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
<!-- SECTION:NOTES:END -->
