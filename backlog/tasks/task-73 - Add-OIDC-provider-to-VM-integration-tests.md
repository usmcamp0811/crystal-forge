---
id: TASK-73
title: Add OIDC provider to VM integration tests
status: Done
assignee: []
created_date: '2026-02-20 14:28'
updated_date: '2026-03-13 01:24'
labels:
  - testing
  - infra
  - auth
  - oidc
  - vm-tests
dependencies:
  - TASK-65.2
  - TASK-72
priority: medium
ordinal: 67000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
VM integration tests lack real OIDC authentication testing. Auth flows are not tested in VM environment.

Goal
Integrate OIDC provider (Keycloak/Authentik) into VM test infrastructure to test full authentication flows.

Non-Goals:
- Complex multi-provider scenarios
- Production-grade OIDC setup in tests
- UI-level auth testing (focus on API/backend flows)

Architectural Constraints:
- OIDC provider must be declared in NixOS test VM configuration
- Should use nixpkgs OIDC provider module (Keycloak or Authentik)
- Test realm/client pre-configured via NixOS module
- VM tests must verify full OIDC handshake (discovery, token exchange, validation)

Verification Plan:
- nix flake check includes VM tests with OIDC
- VM test spawns OIDC provider alongside Crystal Forge server
- Test verifies OIDC discovery endpoint reachable
- Test completes full OIDC login flow programmatically
- Test verifies JWT token validation and role extraction

Impact Areas:
- Infrastructure, Testing, Security

Risk Level:
- Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 VM integration tests start an OIDC provider service (Keycloak or Authentik) in the test topology
- [ ] #2 The provider is preconfigured with test realm/client metadata required by Crystal Forge
- [ ] #3 A VM test asserts OIDC discovery endpoint reachability before login
- [ ] #4 A VM test completes an auth flow and verifies JWT claim extraction/validation used by server auth
- [ ] #5 A VM test verifies server-side session creation for an authenticated OIDC user
- [ ] #6 `nix flake check` includes and executes the OIDC VM auth coverage path
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode-gpt-5.3-codex on gray in /home/mcamp/code/crystal-forge/TASK-73-add-oidc-vm-tests

### 2026-02-21: OIDC module wiring
Continuing TASK-73 to add NixOS module OIDC options and wire them into crystal-forge-server systemd environment for VM auth tests.
<!-- SECTION:NOTES:END -->
