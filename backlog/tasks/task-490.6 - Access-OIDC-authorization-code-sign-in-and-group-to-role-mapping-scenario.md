---
id: TASK-490.6
title: 'Access: OIDC authorization-code sign-in and group-to-role mapping scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-17 17:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 518000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18): this task owns only OIDC Python/browser fragments and the local provider fixture under owner-local fragment/fixture paths. The coordinator owns `lifecycle-oidc.nix`, composition, browserSpecs mapping, and registration. A distinct fresh VM remains necessary because `auth_mode=oidc` and provider/redirect configuration differ from the local-auth bootstrap. Do not create a derivation per packet or reuse local-bootstrap runtime state.

Owning check: integration-access. Contract ID: access.lifecycle.oidc. Exclusive writes: checks/integration-access/scenarios/lifecycle-oidc.{nix,py,cjs} and checks/integration-access/fixtures/oidc-realm.json. Legacy checks/oidc-auth stops at the redirect and does not prove callback, CF session or mapping; retirement map replaces it with this scenario only after evidence passes.

Scope: real browser authorization-code flow against a local services-role OIDC provider. Callback creates user_sessions with auth_source oidc and external_identities. bootstrapAdminGroup and admin-created group mappings yield authored role/environment scope; an unmapped user gets no role. Sign-out invalidates the session. Configure through services.crystal-forge.server.auth_mode = "oidc" and server.oidc.* only. Non-goals: external providers and token-refresh timing. TASK-490 test-only scope applies.

Organization: checks/integration-access/default.nix is native Snowfall { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios, and normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Owner-local scripts/fixtures/assets; no root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix, shared lib and CI/docs; explicit registration required. Meaningful WIP may be discoverable; only Access exists now, no placeholders.

Prerequisite: TASK-490.7 provides OIDC provider and browser-trusted redirect origin. Read cf-server handlers/api/auth_oidc*.rs, auth/dev_mode.rs, admin.rs oidc-mappings, docs/knowledge/security/oidc-role-mapping.md, operations/authentication-modes-and-oidc-configuration.md and legacy checks/oidc-auth/realm-crystal-forge.json as adaptation evidence. As-built D1 is not accepted authority; uncertain cases are blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-oidc -L; append .run for evidence; coordinator aggregate nix build .#checks.x86_64-linux.integration-access -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence notes; preserve correct product-defect assertions and link Backlog bugs, escalate harness/authority gaps.

READINESS: foundation dependency commit unresolved, two existing Rust package tests block foundation, no VM execution. Backlog, not ready. Actual dependency commit(s), implemented signatures/fixtures/registration and verified commands are required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Browser authorization-code sign-in creates a CF session and external identity
- [ ] #2 Bootstrap admin group and admin-created group mappings yield the expected role and environment scope; unmapped users get no role
- [ ] #3 Sign-out invalidates the OIDC session
- [ ] #4 Executed in a real VM topology with recorded evidence; legacy oidc-auth assertions are superseded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior paths and registration model. Actual foundation commit unresolved; foundation blocked by two existing Rust package tests, no VM execution. Remain Backlog; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
