---
id: TASK-72
title: Add Keycloak/Authentik to process-compose for OIDC testing
status: In Progress
assignee: []
created_date: '2026-02-20 14:28'
updated_date: '2026-02-21 00:35'
labels:
  - devex
  - infra
  - auth
  - oidc
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Local development lacks real OIDC provider for testing authentication flows. Currently forced to use dev auth bypass.

Goal: Add containerized OIDC provider (Keycloak or Authentik) to process-compose stack for local OIDC testing.

Non-Goals:
- Production-grade OIDC deployment
- Multi-provider support (single provider sufficient for dev)
- Complex realm/tenant setup

Architectural Constraints:
- Must run in process-compose alongside PostgreSQL and server
- Should use containerized deployment (podman/docker)
- Pre-configured with test realm and client
- Should auto-create test users (admin, operator, viewer)

Verification Plan:
- process-compose oidc-stack up starts Keycloak/Authentik
- Server can discover OIDC endpoints
- Can complete full OIDC login flow
- Test users can authenticate

Impact Areas:
- Infrastructure, DevEx

Dependencies:
- TASK-65.2 (OIDC provider integration foundation)

Acceptance Criteria:
- process-compose includes oidc-stack profile with Keycloak or Authentik
- Provider starts with pre-configured realm and test client
- Test users (admin/operator/viewer) pre-seeded
- Server AUTH_MODE=oidc can authenticate against local provider
- Documentation explains how to use OIDC stack for local testing
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 includes an  profile with Keycloak or Authentik
- [ ] #2 Provider starts with preconfigured realm/client metadata for Crystal Forge
- [ ] #3 Test users (admin/operator/viewer) are pre-seeded for local auth validation
- [ ] #4 Server with  can complete login against local provider
- [ ] #5 Local dev docs explain startup and usage of the OIDC stack
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: codex on gray in /home/mcamp/code/crystal-forge/TASK-72-add-oidc-process-compose
<!-- SECTION:NOTES:END -->
