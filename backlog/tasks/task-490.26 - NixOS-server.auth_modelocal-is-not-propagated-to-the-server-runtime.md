---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 03:33'
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
Propagate `services.crystal-forge.server.auth_mode` through the normal NixOS production server unit. Set `AUTH_MODE` from the module option for both local and OIDC; keep OIDC-specific variables conditional and preserve existing EnvironmentFile precedence. No test-only override or authentication-policy change.

Separately committed as `136348e8f1772372dc96630112902265853e3656`. Targeted NixOS evaluation verified local/OIDC AUTH_MODE and OIDC-only variable presence. The combined Access scenario passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`: empty-DB setup-status reports local mode, the production embedded UI registers/signs in the first administrator, reload preserves the active session, and wrong password is rejected without another session. Evidence: `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. No test-only environment override was used. The separate TASK-492 Postgres Jobs startup-order defect is not included in this authorization.
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
<!-- SECTION:NOTES:END -->
