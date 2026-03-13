---
id: TASK-72
title: Add Keycloak/Authentik to process-compose for OIDC testing
status: Done
assignee: []
created_date: '2026-02-20 14:28'
updated_date: '2026-03-13 01:24'
labels:
  - devex
  - infra
  - auth
  - oidc
dependencies: []
priority: medium
ordinal: 50000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Local development lacks a real OIDC provider for testing authentication flows. Development is currently forced to use auth bypass.

Goal
Add a containerized OIDC provider (Keycloak or Authentik) to the process-compose stack for local OIDC testing.

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

Risk Level:
- Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `process-compose` includes an `oidc-stack` profile with Keycloak or Authentik
- [ ] #2 Provider starts with preconfigured realm/client metadata for Crystal Forge
- [ ] #3 Test users (admin/operator/viewer) are pre-seeded for local auth validation
- [ ] #4 Server with `AUTH_MODE=oidc` can complete login against local provider
- [ ] #5 Local dev docs explain startup and usage of the OIDC stack
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: codex on gray in /home/mcamp/code/crystal-forge/TASK-72-add-oidc-process-compose

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/122
<!-- SECTION:NOTES:END -->
