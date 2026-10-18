---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: Backlog
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 00:15'
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
The TASK-490.1 Access scenario uses the normal module option services.crystal-forge.server.auth_mode = "local". Generated config contains [server] without auth_mode; `systemctl show crystal-forge-server.service -p Environment --value` reports AUTH_MODE=unset. cf-config default_auth_mode() reads AUTH_MODE and defaults to oidc, so the module's local choice is not represented in generated runtime configuration.

Observed configuration in the native Access VM at final combined commit fc930335878279ac426510359c039dfd8961dc13: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt. The same run failed first on TASK-490.25's malformed database URL; no route registration, setup-status response, or OIDC HTTP behavior was observed. Expected: normal module auth_mode=local selects local auth. Actual: generated TOML omits the key and systemd env omits AUTH_MODE. Keep the local-mode assertion executable. No test-only override or production fix belongs in TASK-490.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The normal NixOS module option auth_mode=local results in the server runtime selecting local mode
- [ ] #2 The fresh setup-status endpoint reports auth_mode local and requires_setup true on an empty database
- [ ] #3 The local registration and sign-in routes are registered through the normal module configuration without a test-only environment override
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Final clean combined source commit 39a6a54b651a8b77fc3416b73195e363a782fa2e reproduces the generated-config mismatch: summary shows `[server]` with no auth_mode and systemd AUTH_MODE=unset. The server failed earlier on TASK-490.25, so this VM did not observe an auth-mode response or route registration. Evidence is in /nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt.
<!-- SECTION:NOTES:END -->
