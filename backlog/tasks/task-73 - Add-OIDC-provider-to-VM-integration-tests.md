---
id: TASK-73
title: Add OIDC provider to VM integration tests
status: Backlog
assignee: []
created_date: '2026-02-20 14:28'
labels:
  - testing
  - infra
  - auth
  - oidc
  - vm-tests
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: VM integration tests lack real OIDC authentication testing. Auth flows are not tested in VM environment.

Goal: Integrate OIDC provider (Keycloak/Authentik) into VM test infrastructure to test full authentication flows.

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

Dependencies:
- TASK-65.2 (OIDC provider integration foundation)
- TASK-72 (Keycloak/Authentik integration knowledge)

Acceptance Criteria:
- VM test includes OIDC provider (Keycloak or Authentik)
- Provider configured with test realm and client
- Test verifies OIDC discovery endpoint
- Test completes full authorization code flow
- Test verifies JWT validation and role extraction
- Test verifies user session creation
- nix flake check includes OIDC auth tests
<!-- SECTION:DESCRIPTION:END -->
