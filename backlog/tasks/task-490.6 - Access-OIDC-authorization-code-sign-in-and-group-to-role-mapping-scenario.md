---
id: TASK-490.6
title: 'Access: OIDC authorization-code sign-in and group-to-role mapping scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
labels:
  - testing
  - integration
  - luna
dependencies: []
parent_task_id: TASK-490
priority: high
type: task
ordinal: 518000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract ID: access.lifecycle.oidc. Scenario: tests/integration/access/scenarios/lifecycle-oidc.{nix,py,cjs} plus fixture tests/integration/access/fixtures/oidc-realm.json (exclusive write paths).

WHY: legacy checks/oidc-auth stops at the 302 redirect; it never completes the callback, never creates a CF session, and never proves group mapping. The retirement map replaces it with this scenario.

SCOPE: browser authorization-code flow against a local OIDC provider on the services role; callback creates a CF session (user_sessions with auth_source oidc, external_identities row); bootstrapAdminGroup and an admin-created OIDC group mapping produce the expected role and environment scope; a user in no mapped group gets no role; sign-out invalidates the session. Configure the server only through services.crystal-forge.server.auth_mode = "oidc" and server.oidc.* (documented options). Non-goals: external providers; token refresh timing.

READY WHEN: TASK-490.7 provides the services role with the OIDC provider and a browser-trusted origin for the redirect URI. Source to read then: cf-server src/handlers/api/auth_oidc*.rs, auth/dev_mode.rs (OIDC bootstrap admin group), handlers/api/admin.rs (oidc-mappings), docs/knowledge/security/oidc-role-mapping.md, docs/knowledge/operations/authentication-modes-and-oidc-configuration.md, checks/oidc-auth/realm-crystal-forge.json (legacy fixture to adapt).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Browser authorization-code sign-in creates a CF session and external identity
- [ ] #2 Bootstrap admin group and admin-created group mappings yield the expected role and environment scope; unmapped users get no role
- [ ] #3 Sign-out invalidates the OIDC session
- [ ] #4 Executed in a real VM topology with recorded evidence; legacy oidc-auth assertions are superseded
<!-- AC:END -->
