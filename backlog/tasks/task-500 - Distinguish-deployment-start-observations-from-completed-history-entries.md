---
id: TASK-500
title: Distinguish deployment-start observations from completed history entries
status: In Progress
assignee:
  - openai
created_date: '2026-10-20 05:20'
updated_date: '2026-10-20 21:15'
labels: []
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/knowledge/ui/deployment-history-rendering-rules.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: medium
type: bug
ordinal: 551000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Correct the verified SystemDetail History and rollback-plan presentation defects against the approved reference and authoritative history contract. After two actual deployments the current view shows four deploys and renders Started observations as Deployed/success/no generation activated. The rollback plan renders Strategy rollback instead of approved switch_to_generation. The owner now authorizes only these bounded UI repairs, count semantics grounded in accepted authority, and behavioral Started/terminal/count/reload assertions. No icon, diagram, unrelated layout or design-reference changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted History presentation distinguishes Started from actual completed activation and defines the displayed deployment count.
- [ ] #2 The rollback plan's strategy label follows the approved presentation or an explicitly recorded product decision.
- [ ] #3 Behavioral browser coverage verifies Started versus completed presentation and durable reload behavior without synthetic completion evidence.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Dedicated worktree/branch TASK-500-history-presentation, clean base df11c800; Sol owns persistent preview and task lock. UI writes are blocked until that preview is verified/published. No competing default-port stack or shared user database.
1. Read approved SystemDetail and authoritative history mappings. State the existing metric's accepted meaning; count distinct deployment attempts rather than Started and terminal observations twice, retaining failures where the approved contract includes them. A Started observation cannot be labelled completed Deployed/success.
2. Make only the bounded History mapping/count and rollback Strategy presentation repair, matching switch_to_generation. Do not change icons, layout, unrelated sections or design references. Extend pure UI model regressions for Started/terminal/failed/same-deployment counts and reload identity.
3. Coordinator extends existing Delivery browser assertions to inspect all contradictory cards/counts, exact event/deployment identity and approved rollback plan. No duplicate lifecycle/new VM. Verify preview current, then normal affected UI/component/Delivery checks and screenshots.
4. Commit separately; Sol integrates and records missing/extra/reordered/merged/metadata/interaction/hierarchy comparison disposition explicitly. No Done or merge.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner authorization: current TASK-490 assignment selects TASK-500 for bounded product UI repair. Baseline hosted screenshot /tmp/opencode/hosted-delivery-history-df11c800.png and MR335 upload show the defect. Start a verified task-owned live preview before UI edits. Existing user's long-running :8080/:3445/:5432 instances must not be reused or stopped. Use dedicated worktree and native devenv isolation; coordinator owns preview processes/database and integration assertions.

Bounded UI implementation committed as 39b5844b on TASK-500-history-presentation (base df11c800), dedicated /home/mcamp/code/crystal-forge/TASK-500-history-presentation. Only packages/web-ui/src/views/system_detail.rs and docs/knowledge/ui/deployment-history-rendering-rules.md changed. No push/MR/status transition. Sol retains lock and preview ownership.

Approved-reference comparison (source-level): docs/design/CrystalForge/components/SystemDetail.jsx at df11c800, buildHistory/HistoryTab and DeployTab previous-generation plan. Metric counts the reference's deploy category: CF attempts including failures, plus local rebuilds. Each CF deployment_id counts once across Started and terminal events. Persisted event IDs deduplicate repeated observations; no-deployment-ID rows remain separate (no guessed commit/path/time correlation). Started remains a separate nonterminal card, never activation/success; no activation generation or rollback action; reconstructed Logs emits only its start observation. Persisted event IDs and optional deployment IDs are exposed on cards and used for Logs jump anchors. Strategy now switch_to_generation. Missing/extra/reordered/merged: source changes add no sections or reordering; retain separate observations, deduplicate repeated event IDs; count merges lifecycle observations only for metric. Metadata: preserve lifecycle and event identity. Interaction: disable Started rollback, retain exact Logs anchor. Visual hierarchy: existing card/classes with nonterminal blue accent/info chip; rendered comparison remains unverified pending Sol's browser check/screenshots. No additional product decision required; legacy missing-ID count uncertainty is documented.

Verification on final implementation: nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml views::system_detail:: -> exit 0, 58 passed, 708 filtered. Three added tests cover Started-only/explicit-type authority, Started+success and Started+failure shared-ID counts, failure/local rebuild inclusion, duplicate event IDs, no-ID compatibility, JSON reload and reordered count stability. nix develop -c rustfmt --edition 2024 --check packages/web-ui/src/views/system_detail.rs -> exit 0. nix build .#okf-knowledge -L --no-link -> exit 0, 58 validator tests, 192 concepts validated, 215 Mermaid blocks parsed with zero errors. git diff --check -> exit 0. Earlier cargo test with --lib failed because UI has no library target; corrected command above passed. Package-wide cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check failed on existing caches.rs/cves.rs/poams.rs formatting; scoped rustfmt passed. Earlier documentation builds found a renamed historical anchor; restored original heading, final validation passed.

Preview takeover: user/Sol verified isolated task preview before edits, frontend http://127.0.0.1:8081/systems, API http://127.0.0.1:3446/status both returned HTTP 200 in this session. Reused owner arrangement, started/stopped no processes, touched no DB/data. Backend source unchanged; Sol-owned tmux cf-task500-private-api and task PG :5433 retained. Frontend source edits require compilation; exact refreshed browser bundle/rendering unverified here. Sol must perform coherent-change browser verification, integration-delivery browser assertion integration/execution and MR screenshots. Known relative evaluation-queue read preview gap remains outside this repair.
<!-- SECTION:NOTES:END -->
