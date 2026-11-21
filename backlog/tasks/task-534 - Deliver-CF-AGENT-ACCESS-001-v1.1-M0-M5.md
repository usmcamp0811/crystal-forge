---
id: TASK-534
title: Deliver CF-AGENT-ACCESS-001 v1.1 M0-M5
status: Backlog
assignee: []
created_date: '2026-11-16'
labels: [security, agent-access, mcp, specification]
dependencies: []
references:
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
  - /home/mcamp/code/crystal-forge/crystal-forge.html
  - TASK-271
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Agent access needs an explicit, bounded authorization contract and a durable remediation-context specification. Token authorization must not expand during POA&M role refresh or continue using stale managed OIDC entitlements. The supplied design export supplies the updated reference; the user subsequently approved replacing split runtime assets with a single readable HTML artifact while preserving exported behavior, written contracts and independent fixtures.

## Goal

Deliver CF-AGENT-ACCESS-001 v1.1 through M0-M5, with acceptance-test evidence for every milestone. The authoritative specification destination is `docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md`. The local export `/home/mcamp/code/crystal-forge/crystal-forge.html` is a design reference, not the security authority. TASK-534.1 preserves the full specification and integrates the design; TASK-534.2 begins the independent M0 authorization groundwork. Both children were selected for the sprint by explicit user delegation and are In Progress in their existing dedicated worktrees. This parent remains Backlog.

Allocation provenance: our old TASK-526, TASK-526.1 and TASK-526.2 names were session allocation collisions resolved after rebasing onto origin/dev at `881dbc76`. TASK-526 belongs to the existing farm parser-deadline task. The delivery family is now TASK-534, TASK-534.1 and TASK-534.2. Historical commit hashes, branch names and worktree paths remain provenance; the owner will handle Git renaming separately.

## Required delivery contract

- Operator, Viewer, and Admin owners have token self-service. Never require an Admin gate for owner self-service. Admin oversight is a separate capability and cannot broaden a token's grant.
- Owner token management uses an authenticated browser session and CSRF protection. CFAT bearer credentials do not substitute for this management session.
- CFAT secrets are stored hash-only, revealed only at issuance, redacted from logs and audit payloads, and limited to audience `mcp`. Grants contain explicit environment IDs and scopes and bounded expiry; no wildcard or implicit future-environment expansion.
- Persist an immutable token grant separately from mutable POA&M role/authorization refresh. Effective authority cannot exceed the original grant or current owner authority. Managed OIDC entitlement reconciliation is authoritative, fresh, and fail-closed.
- Revoke and rotate are transactional. Rotation must not leave both old and new credentials usable after success or a partial replacement after failure. Exercise concurrent use, revoke, and rotate.
- Initial MCP delivery is read-only and exposes exactly the four tools defined by v1.1. Preserve their exact names, schemas, scope checks, environment boundaries, denial behavior, and read-only side-effect guarantees from the full specification; do not invent replacements.
- Remediation context includes CVE/package/module evidence and provenance linked to TASK-271. Distinguish observed evidence, inferred attribution, and missing provenance; never fabricate module ownership.
- UI delivery is additive and matches the supplied design. Include owner self-service, admin oversight, and an authorization inspector without removing legacy screens or interactions.
- Mutation tools are gated for later delivery. CLI and service principals are later phases, not authorization shortcuts for the initial MCP release.

## Milestone delivery map

This is the backlog coverage map; preserve the user-provided source specification's exact milestone requirements and numbering rather than silently rewriting them.

- **M0 — Authorization groundwork:** immutable grant/context separation; fail-closed OIDC managed entitlement reconciliation and freshness; regression and acceptance tests. TASK-534.2.
- **M1 — Reusable PAT service and UI:** hash-only credentials, explicit scopes/environment IDs, `mcp` audience, bounded expiry, session+CSRF management, transactional revoke/rotate, Operator-first Profile self-service and Connect agent entry points, negative/race and local/OIDC browser tests.
- **M2 — Read-only MCP:** four specified tools, protocol and authorization contracts, cross-environment isolation, secret redaction, and no mutation side effects.
- **M3 — Remediation context/provenance:** exact context/evidence contracts and TASK-271 attribution integration, including unknown and stale evidence tests.
- **M4 — Controlled remediation operations:** explicitly enabled scans:request and poams:write policy, scan results, narrow progress updates, revisions, idempotency and audit. No general deployment, closure or risk acceptance.
- **M5 — Future CLI and service principals:** dedicated cli audience and scopes, independent audience tests, first-class admin-managed service principals with their own environment/capability grants. Evaluate OAuth only for named client needs; never broaden existing PATs silently.

Additive owner/admin/inspector UI and retained legacy behavior belong to the applicable M1–M3 deliveries, not a replacement milestone. End-to-end security, upgrade, operational documentation and the release gate apply across all milestones.

No milestone is complete without passing acceptance tests tied to its requirements. Documentation or mockups alone do not prove implementation.

## Non-goals

This parent does not authorize an unbounded implementation task, deployment, automatic mutation access, or changes to unrelated backlog locks. The user-approved single-file design-reference replacement is scoped to TASK-534.1, not production application replacement. Split later milestones into sprint-ready children before implementation.

## Architectural constraints

Keep authorization and persistence server-owned. Reuse existing Rust, SQLx, Nix, session, CSRF, and four integration-owner mechanisms. Use forward migrations and update SQLx metadata when required. Use task-owned isolated databases and dedicated implementation worktrees. Preserve supported clients and legacy design functionality. Keep OKF metadata truthful about proposed versus implemented behavior.

## Verification

Each child must specify acceptance tests and exact runnable commands before implementation. Use component/database tests for policy and transactions, the smallest applicable existing `integration-access` scenario for access/session/UI behavior, and affected Assurance scenarios for remediation provenance. Use `nix build .#okf-knowledge -L` for OKF changes. Record commands, actual results, skipped cases, and blockers per milestone; no completion on unchecked requirements.

## Impact, risk, and dependencies

- Impact: identity/authorization, OIDC, POA&M context refresh, token persistence/API, MCP, CVE provenance, design examples, Web UI, audit, and OKF.
- Risk: high; stale authority, credential disclosure, environment leakage, role escalation, and credential lifecycle races.
- Dependencies: TASK-534.1 and TASK-534.2 are first slices. TASK-271 is a provenance integration dependency, not a prerequisite for independent M0 policy work. Later tasks must identify exact predecessor milestones.
- Source: user-provided CF-AGENT-ACCESS-001 v1.1 message dated 2026-11-16, available in the main agent's conversation. The previous missing-source claim was incorrect due to incomplete delegation context. Use the complete source for exact tool contracts, numerical expiry/freshness limits, and milestone mapping; this backlog summary is not the full specification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 TASK-534.1 preserves the full CF-AGENT-ACCESS-001 v1.1 requirements in the authoritative OKF path and normalizes the supplied export into the user-approved single-file design reference, with preservation checks and explicit design/spec gaps.
- [ ] #2 M0-M5 each have scoped delivery tasks, traceable specification requirements, acceptance tests, actual passing evidence, and truthful implementation status before milestone completion.
- [ ] #3 Operator, Viewer, and Admin owner self-service passes without an Admin gate; session+CSRF management and separate admin oversight pass positive and denial tests.
- [ ] #4 CFAT hash-only storage, mcp audience, explicit environment IDs/scopes, bounded expiry, immutable grants, fresh fail-closed OIDC authority, and transactional revoke/rotate pass security and concurrency acceptance tests.
- [ ] #5 Exactly four specification-defined read-only MCP tools pass protocol, scope/environment isolation, denial, redaction, and no-mutation acceptance tests.
- [ ] #6 Remediation evidence and TASK-271 provenance pass accuracy/unknown-evidence tests; additive owner/admin/inspector UI passes design comparison and owning integration tests.
- [ ] #7 Mutations, CLI, and service principals remain explicitly gated for later approved delivery; release documentation and M0-M5 acceptance evidence are complete before this parent can finish.
<!-- AC:END -->
