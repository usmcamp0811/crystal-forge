---
id: TASK-529.1
title: Preserve agent-access specification and single-file design export
status: In Progress
assignee: []
created_date: '2026-11-16'
labels: [security, specification, design, okf]
parent_task_id: TASK-529
dependencies: []
references:
  - TASK-529
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
  - /home/mcamp/code/crystal-forge/crystal-forge.html
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

CF-AGENT-ACCESS-001 v1.1 needs a canonical OKF record. The supplied standalone HTML export contains the updated design reference. The user subsequently approved replacing the split runtime design with one readable HTML artifact and removing superseded split design assets, so future exports have useful Git diffs.

## Goal

Preserve the complete v1.1 specification at `docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md`, with truthful OKF metadata, source provenance, security index links, and requirement/test traceability. Normalize the supplied local export into the single readable runtime artifact `docs/design/CrystalForge/crystal-forge.html`. Preserve its screens and document missing specification surfaces. The specification controls security behavior when a mockup differs.

## Non-goals

Implementing production token APIs, MCP, OIDC, migrations, or Web UI; claiming the design proves implemented authorization; deleting authoritative written contracts or independent test fixtures merely because the runtime no longer needs split source files.

## Architectural constraints

Follow existing OKF schema/index patterns and Mermaid rules. Preserve source requirements, exact four MCP tool contracts, numerical bounds, and M0-M5 criteria. Identify proposed versus current behavior. Use `/home/mcamp/code/crystal-forge/crystal-forge.html` as the supplied export source and retain its provenance. Inventory components, screens, fixtures, navigation, interactions and external consumers before cleanup. Inline readable source in stable template order without compressed payloads or UUID-driven diff churn. Migrate independent fixture/test consumers before deleting their inputs. Preserve written contract/history references. User explicitly approved a static fixture-only preview instead of a Dioxus/database stack for this design-only task.

## Verification plan

1. Compare the complete v1.1 source with the OKF concept section by section; record source location and any deliberate formatting-only transformations.
2. Run `nix build .#okf-knowledge -L` and inspect the generated concept/index links.
3. Record a before/after exported-resource and design interaction inventory. Exercise the supplied owner workflow in the task-owned static preview; record missing admin/inspector/remediation surfaces as gaps, not passing flows.
4. Map each v1.1 requirement to acceptance tests or an explicitly tracked later milestone; do not check implementation tests as passed for design-only work.

## Impact, risk, and dependencies

- Impact: the canonical security concept, security indexes/cross-links, and `docs/design/CrystalForge` example assets and interactions.
- Risk: medium; lost specification detail, misleading implementation status, or regression from replacing a bundled design.
- Dependencies: parent TASK-529 provides the delivery scope; local export is available and has been inspected as an HTML bundle. No dependency on M0 implementation.
- Source: user-provided CF-AGENT-ACCESS-001 v1.1 message dated 2026-11-16, available in the main agent's conversation. The earlier missing-source claim resulted from incomplete delegation context and is incorrect. Use that complete message for preservation and comparison; do not reconstruct the specification from this backlog summary. There is no missing-source blocker.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The complete user-provided v1.1 source message dated 2026-11-16 is preserved at the authoritative path with its identifier/version/provenance, exact tool contracts and bounds, M0-M5 requirements, truthful OKF metadata, and navigable index links.
- [ ] #2 A requirement comparison demonstrates that no normative source requirement was omitted or weakened; security authority is explicitly the spec path, while the local export is identified as a design reference.
- [ ] #3 Supplied export is normalized into one readable HTML runtime artifact; superseded split runtime design files are removed after consumer migration, with preserved exported behavior, written contracts and independent test fixtures.
- [ ] #4 Owner self-service for Operator/Viewer/Admin is compared with v1.1; missing admin oversight, authorization inspector and remediation panels are explicitly tracked as later design/implementation gaps rather than falsely attributed to the supplied export.
- [ ] #5 OKF validation passes, affected design interactions are exercised, and requirement-to-milestone/acceptance-test traceability is recorded. Failed verification prevents completion.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-526.1-agent-access-design

Allocation provenance: old TASK-526.1 was a session allocation collision resolved as TASK-529.1 after the origin/dev `881dbc76` rebase; parent is TASK-529. Status remains In Progress. The lock path and historical branch below retain their original names; Git/worktree renaming is owner-managed.

Activated on 2026-11-16 by explicit user direction. Dedicated worktree verified clean before backlog mirroring; branch `TASK-526.1-agent-access-design`, base `dev` at `bd8d068dc287d222baeea9c7336363a03da55371`. User subsequently approved single-file runtime replacement, legacy runtime cleanup, static-only preview and regular commits/pushes. Specification is recorded; design migration is underway. No runtime milestone or security test has been marked complete.

Checkpoint: specification and task records committed as `43b27c53` and pushed to origin. Default SSH port 22 timed out; command-scoped Git URL rewriting to GitLab's alternate SSH endpoint on port 443 succeeded without changing persistent Git configuration.

Design comparison: 65 existing bundled resources matched byte-for-byte; new Access Tokens source plus a Profile insertion after Active sessions. Normalized 67 resources, 71 scripts and one unchanged CSS block into readable HTML. Removed 75 split/duplicate runtime assets and repointed current test/design consumers; written contracts, review assets and independently consumed golden JSON remain. No existing Profile section was reordered or merged by the export. The supplied export lacks admin oversight, inspector, MCP status, Connect agent shortcuts and remediation panels; TASK-529 M1–M3 retains these obligations. The mockup is not evidence of backend authorization.

Verification so far: six importer unit tests pass; exact normalization/idempotency and script order/content/CSS preservation pass; manifest contracts and reference resolution pass; consent banner extraction is byte-identical; web-ui oracle/derivation Nix evaluation passes. Browser smoke passed Profile navigation, create, one-time reveal acknowledgement and resulting row with zero page errors/failed requests. Full persona/mobile/legacy-flow matrix remains unverified. Static preview is `http://127.0.0.1:8526/crystal-forge.html`, task-owned loopback PID 28238, no DB/backend.

Known checks: broader static harness fails on missing `Inventory only` relation coverage, reproduced against the baseline harness/manifest. The exported HTML retains seven source trailing-whitespace lines; these are not security verification failures but are reported by `git diff --check`. Initial OKF attempts found arrow-heading diagram candidates and unsupported source metadata; corrected. Final `nix build .#okf-knowledge -L` passed: 58 validator tests, 197 concepts, 219 Mermaid blocks, zero unclassified/stale diagram candidates. Full production Web UI, SQLx/PostgreSQL and NixOS integration checks were not run for this design-only change; no production authentication behavior changed.
<!-- SECTION:NOTES:END -->
