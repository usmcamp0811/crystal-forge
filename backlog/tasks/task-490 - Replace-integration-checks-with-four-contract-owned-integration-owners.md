---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 16:14'
labels:
  - testing
  - integration
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md
  - AGENTS.md
  - docs/agents/verification.md
  - docs/knowledge/testing/flake-checks.md
priority: high
type: task
ordinal: 512000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The current integration layer is a mix of about 14 VM checks and one 26.9k-line browser suite. 105 of its 181 browser steps are route-mocked, OIDC is never completed end to end, CVE scanning uses a fake runner, real deploy/rollback is excluded, and no real old-version upgrade is tested. The authoritative browser gate (web-ui-check) is allow_failure. Coverage ownership is unclear and duplicates proliferate per task.

The owner (Matt, 2026-10-17 session) approved adopting the integration maintenance contract (source file /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md, to be committed at docs/knowledge/testing/integration-test-contract.md). It replaces the integration layer with exactly four flake checks: integration-access, integration-delivery, integration-assurance, integration-upgrade. This parent tracks the overhaul; bounded children implement it.

OWNER AUTHORIZATION (recorded verbatim in intent): the agent may create this parent and bounded children, select them for this work, move dependency-ready children to To Do, create task branches/worktrees, lock and start the foundation, and implement it. The authorization covers this overhaul only. It does NOT authorize: merging into dev/main, deploying, marking the overall task Done without final approval, selecting unrelated backlog work, or starting product-fix tasks.

STRICT SCOPE (test-only): application functionality is read-only. Allowed: integration test code and test-only fixtures, test-only NixOS configs, shared test helpers, browser tooling, the four flake registrations and directly required execution wiring, pinned test dependencies that do not change application dependencies, the existing integration CI entry point at cutover, the canonical contract, directly affected testing docs and agent-guide links, and task records. Forbidden: production Rust/UI/assets/styles/APIs/authz/policy/deployment logic/migrations/SQLx metadata/module semantics, production test selectors, debug endpoints, auth bypasses, mock-success paths, patched application in test derivations, and runtime dependency changes. Rust unit tests stay. docs/design/CrystalForge/ stays read-only.

Overhaul integration branch: TASK-490.1-integration-foundation (the foundation branch). Child commits are integrated there. Base: origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd.

Coordinator-owned (single writer) paths: tests/integration/lib/**, tests/integration/default.nix, tests/integration/<owner>/default.nix (scenario registration lists), checks/integration-{access,delivery,assurance,upgrade}/ (created only at owner completion), flake/CI wiring, docs/knowledge/testing/integration-test-contract.md, docs/knowledge/testing/integration-coverage-map.md, AGENTS.md and docs/agents/* integration links.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The integration contract is committed at docs/knowledge/testing/integration-test-contract.md with accurate implementation_status and an AGENTS.md rule that replaces the single web-ui UI-verification rule
- [ ] #2 The coverage map records every accepted obligation with one primary owner, scenario, independent expected result, evidence, and status, plus a retirement disposition for every legacy integration check and CI job
- [ ] #3 Exactly four integration flake checks (integration-access, integration-delivery, integration-assurance, integration-upgrade) exist at cutover; non-integration checks keep their own attributes and are classified explicitly
- [ ] #4 Each exported owner executes its complete required scenario set and propagates every scenario failure; no owner is exported before its required set is implemented
- [ ] #5 Legacy integration registrations and CI jobs are retired only after their replacement owner scenarios pass or an owner-approved scope change exists
- [ ] #6 Product defects found by correct assertions are recorded as linked Backlog defects and keep their executable failing assertion; none are fixed in this overhaul
- [ ] #7 No production code, migration, SQLx metadata, UI asset, or design-handoff file is changed by any child of this task
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation (overhaul integration branch TASK-490.1-integration-foundation).
<!-- SECTION:NOTES:END -->
