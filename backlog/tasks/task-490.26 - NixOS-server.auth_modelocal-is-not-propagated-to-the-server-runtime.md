---
id: TASK-490.26
title: NixOS server.auth_mode=local is not propagated to the server runtime
status: In Progress
assignee: []
created_date: '2026-10-17 20:28'
updated_date: '2026-10-18 02:48'
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
Current authorized repair (2026-10-18; supersedes the earlier prohibition below): the production server unit now sets `AUTH_MODE = cfg.server.auth_mode` for both local and OIDC; OIDC-only variables remain conditional. Targeted NixOS evaluation returns local for local and oidc for oidc. Separate commit `136348e8` integrates this repair. Combined Access VM verification remains pending; there is no test-only override.

The TASK-490.1 Access scenario configures the normal module option services.crystal-forge.server.auth_mode = "local". The generated config summary contains [server] but no auth_mode key, and `systemctl show crystal-forge-server.service -p Environment --value` reports `AUTH_MODE=unset`. At the reviewed baseline, cf-config `default_auth_mode()` selects the AUTH_MODE environment variable and defaults to "oidc". The module Nix option defaults to local but its generated config/systemd environment does not convey local mode. This conflicts with the accepted local bootstrap expectation and current operations guide.

Evidence from final combined test commit fc930335878279ac426510359c039dfd8961dc13 (application baseline 33de73b013b735304dfac9c69745a074a3e76ebd): `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; generated configuration capture: /nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/generated-config-summary.txt. Expected: generated runtime configuration selects local auth. Actual generated config omits auth_mode and the service environment leaves AUTH_MODE unset. The server did not reach route registration or respond to setup-status in this run because the separate TASK-490.25 database socket URL failure occurred first; do not claim an observed OIDC HTTP response. The scenario's local-mode/startup assertion remains executable.

This is a product fix, explicitly out of scope for TASK-490.1. Do not add a test-only AUTH_MODE override or change application behavior as part of the integration overhaul.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The normal NixOS module option auth_mode=local results in the server runtime selecting local mode
- [x] #2 The fresh setup-status endpoint reports auth_mode local and requires_setup true on an empty database
- [x] #3 The local registration and sign-in routes are registered through the normal module configuration without a test-only environment override
- [x] #4 Targeted module evaluation verifies local and OIDC AUTH_MODE and OIDC-only environment settings
- [x] #5 The combined Access VM proves the local first-admin browser/session workflow without a test-only environment override
<!-- AC:END -->
