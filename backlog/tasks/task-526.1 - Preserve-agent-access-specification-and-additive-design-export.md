---
id: TASK-526.1
title: Preserve agent-access specification and single-file design export
status: In Progress
assignee: []
created_date: '2026-11-16'
labels: [security, specification, design, okf]
parent_task_id: TASK-526
dependencies: []
references:
  - TASK-526
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
3. Record a before/after legacy design interaction inventory. Exercise new owner, admin oversight, and inspector flows and every impacted legacy flow in the task-owned live preview; record design comparisons and browser errors.
4. Map each v1.1 requirement to acceptance tests or an explicitly tracked later milestone; do not check implementation tests as passed for design-only work.

## Impact, risk, and dependencies

- Impact: the canonical security concept, security indexes/cross-links, and `docs/design/CrystalForge` example assets and interactions.
- Risk: medium; lost specification detail, misleading implementation status, or regression from replacing a bundled design.
- Dependencies: parent TASK-526 provides the delivery scope; local export is available and has been inspected as an HTML bundle. No dependency on M0 implementation.
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

Activated on 2026-11-16 by explicit user direction. Dedicated worktree verified clean before backlog mirroring; branch `TASK-526.1-agent-access-design`, base `dev` at `bd8d068dc287d222baeea9c7336363a03da55371`. User subsequently approved single-file runtime replacement, legacy runtime cleanup, static-only preview and regular commits/pushes. Specification is recorded; design migration is underway. No runtime milestone or security test has been marked complete.
<!-- SECTION:NOTES:END -->
