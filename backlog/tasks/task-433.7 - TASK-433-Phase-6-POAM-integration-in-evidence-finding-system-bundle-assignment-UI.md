---
id: TASK-433.7
title: >-
  TASK-433 Phase 6: POA&M integration in evidence/finding, system, bundle,
  assignment UI
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-09-01 13:35'
labels:
  - design-parity
  - poam
  - web-ui
  - compliance
  - phase-6
dependencies:
  - TASK-433.6
references:
  - TASK-433
  - TASK-433.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
documentation:
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 439000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 6 of 8 (contextual only). Wires the Phase 5 POA&M API into compliance/evidence, system detail, and bundle compliance views, and makes assignment POA&M references first-class relationships.

## Explicit scope
- Compliance/evidence: failed rows stay FAIL and expose Create/Link POAM with exact navigation to/from finding/bundle/system/evidence, with real prefilled context.
- System compliance: real committed POAM filters/counts and exact finding navigation.
- Bundle compliance: real open/on-POAM/no-POAM/overdue/awaiting-verification/closed rollups with no N+1 visible-list queries (uses Phase 5 batched rollup APIs).
- Assignment POAM references are first-class relationships and do not mutate immutable assignment versions.

## Explicit non-scope
No dashboard/notifications/coach work (Phase 7). No POA&M API changes (Phase 5 owns the API; this subtask consumes it).

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Evidence supports Create POAM and Link existing with real prefilled context and exact navigation to/from finding/bundle/system/evidence.
- [x] #2 System compliance provides real committed POAM filters/counts and exact finding navigation.
- [x] #3 Bundle compliance provides real open/on-POAM/no-POAM/overdue/awaiting-verification/closed rollups and no N+1 visible-list queries.
- [x] #4 Assignment POAM references are first-class relationships and do not mutate immutable assignment versions.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 6 implementation plan

1. Add the explicitly authorized minimal Phase-5 contract correction, preserving server authority and authorization: bounded batch finding-remediation lookup by authoritative assessment IDs (active plus completed history), server-filtered compatible POA&M search from an authoritative failing assessment, and bounded assignment-version relationship lookup. Reuse existing visibility, compatibility, one-active-remediation, list/detail summary, and pagination logic; add focused HTTP/DB tests and record the correction on TASK-433.6.
2. Add one typed Web UI POA&M API module mirroring Phase-5 DTOs and error envelopes. Centralize authenticated GET and CSRF mutations, revisions, 403/404/409/412 mapping, bounded batch chunking (100 IDs), stale-response suppression, and authoritative-response reconciliation.
3. Build a shared route/state-driven POA&M detail tray used by compliance, system, bundle, and assignment surfaces. Include metadata, lifecycle, verification/closure/reopen, findings, exact evidence navigation, assignment references, plan, milestones, activity/notes, and deliberate loading/not-visible/error/conflict states.
4. Integrate failed evidence rows with a separate remediation dimension: active and historical POA&M lookup, real-context Create, server-compatible Link Existing, exact stable-ID navigation, and explicit copy/tests proving FAIL remains FAIL and waiver actions remain separate.
5. Integrate System Detail compliance with committed server rollup counts and Open/Overdue/Closed/All filters, compact real POA&M rows, common detail, and exact linked-finding navigation. Use batched system rollups for visible lists rather than per-row calls.
6. Integrate bundle compliance with authoritative open/on-POA&M/no-POA&M/overdue/awaiting-verification/completed rollups and filtered real rows. Chunk visible bundle IDs by 100 and add request-count instrumentation proving no N+1.
7. Integrate immutable assignment-version relationships in the existing assignment surface using the separate relationship API only. Display and open referenced POA&Ms; link/unlink against exact assignment version/revision; prove assignment identity/digest/config and current-version pointer remain unchanged.
8. Add pure DTO/presentation/navigation/error tests plus real server-backed browser workflows for finding create/link, detail edits/milestones/activity, closure rejection while FAIL, system integration, bundle rollups/no-N+1, and assignment link/unlink immutability. Extend the coverage manifest and critical Web UI checks; capture supported screenshots.
9. Run exact TASK-433.7 verification, focused Phase-5 contract tests, syntax/diff checks, then perform independent requirements/API-authority/UI-state/E2E/performance/regression/design-comparison reviews. Resolve all P0/P1/P2.
10. Only after AC1-AC4 are objectively proven: check exactly four ACs, move TASK-433.7 to Review, update TASK-433.6 correction evidence and MR !318, commit/push the rebased branch, confirm clean local/remote SHA and CI, then stop before Phase 7.

Interrupted-agent recovery: inspect the full worktree diff, move assignment relationship/link UI into `AssignmentListPanel` with its existing local signals and generation guards, normalize new Dioxus data attributes, fix only compile-required `EvidenceDrawer` call interfaces, then run fmt/check/tests and focused prohibited-global scans before reporting without committing.

Focused browser-failure adjustment: identify the added milestone by the authoritative POST response ID instead of unsupported display-value locator composition; explicitly open the evidence policy target before asserting remediation navigation; initialize the assignment-reference scope from the exact compliance route system ID and render the relationship panel independently of the Systems Matrix fetch, while only auto-opening evidence for `view=evidence`. Verify these fixes with formatting, compilation, and focused workflows before rerunning all Phase-6 workflows.

Phase-6 review remediation at exact head `fd7548ad`: (1) prove and close the production legacy failing-policy finding gap without fabricating composite assessments, using the smallest authoritative source-agnostic finding/observation contract and a real legacy FAIL → Create POA&M → still FAIL browser regression; (2) add cursor-aware incremental loading for findings, activity, and verification attempts in the shared detail tray; (3) map durable activity events to timestamp, actor, and meaningful user-facing descriptions, retaining raw payload only as optional diagnostics; (4) route assignment finding navigation through Dioxus while preserving exact bundle/version/system/policy/evidence query identity; (5) add discriminating unit/server/browser coverage; (6) rerun server POA&M, Web UI unit/package, workflows 29g–29m, and full Web UI checks; (7) run the same full harness at pre-Phase-6 baseline `42488e89` and record exact shared failure names/status; (8) synchronize TASK-433.6/.7 task files into the final remediation commit, update MR !318, and return TASK-433.7 to Review only after AC1 is re-proven. Phase 7 remains prohibited.

Legacy observation design decision: preserve the deployed Phase-5 composite-assessment API for compatibility, and add a narrow source-neutral observation reference for evidence-origin actions. Each failed control exposes its stable finding identity plus an authoritative observation reference (`source_kind`, source identifier, and semantic observation token). The server resolves that reference against the latest deployed derivation/CVE scan or exact composite assessment, rechecks current effective policy/version and FAIL state, and never derives outcome from POA&M state. Stable `poam_findings` rows are idempotently materialized for resolved evidence controls; relationship and compatible-search APIs accept finding IDs so legacy and composite sources share the same remediation identity without fabricating composite assessments. Create/link mutations accept the observation reference while retaining `assessment_id` compatibility. Closure/waiver behavior remains unchanged unless discriminating tests show the review finding requires extending those Phase-5 workflows.

Implement this as a focused additive contract: shared canonical legacy-observation snapshot/token helpers; evidence DTO fields; finding-ID batch relationship and compatible lookup; authoritative create/link validation; Web UI adapter and evidence-bar wiring; server/unit/browser regression proving a persisted legacy failure can create a POA&M and remains FAIL afterward. Then implement the independent pagination/activity/typed-routing findings and verify serially.

Phase-8 visual remediation: correct dark-theme contrast for the shared POA&M detail tray's Expand and Reopen actions. Keep behavior and lifecycle semantics unchanged. Verify with focused TASK-433 POA&M browser captures before returning the owner to Review.

Review remediation at candidate `da7ae7de`: separate metadata and remediation-plan persistence in `PoamDetailTray`. Keep `Save metadata` scoped to title, owner, target date, and risk. Add an adjacent `Save plan` action to the Remediation plan section that PATCHes only the plan with the current optimistic revision, uses existing stale/error reconciliation, and remains disabled for viewers or active mutations. Update the existing server-backed POA&M detail and canonical lifecycle browser workflows so plan persistence uses the labeled local action and survives reload. Run Web UI formatting, Node syntax/static contracts, targeted Rust tests, and focused POA&M browser verification through Nix before returning this owner to Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Preflight: rebased `TASK-433-policy-poam-workflows` onto current `origin/dev` (`79153f74`) without discarding later legitimate changes. Replayed Phase 1-5 implementation; Phase-5 commit is now `42488e89` (rebased equivalent of accepted `68904343`) and remains the branch head before Phase 6. TASK-433.6 is Review with all six ACs checked and successful exact-head pipeline 2794857147 recorded. Dedicated worktree is `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`; scope is exactly AC1-AC4, with Phase 7 dashboard/notifications/coach explicitly excluded.

Phase-6 contract audit found blocking Phase-5 API omissions before any implementation write. AC1 cannot be implemented without client-side approximation/N+1 because evidence DTOs expose only an assessment ID and no stable finding/remediation relationship, while Phase 5 has no batch active/historical remediation lookup for visible findings/assessment IDs. AC1 Link Existing is also directionally unsupported: `GET /poams/:poam_id/compatible` returns findings compatible with a known POA&M, but the required finding-origin flow needs server-filtered POA&Ms compatible with a supplied real finding/assessment; broad `/poams?policy_lineage_id=...` listing would shift compatibility/active-link decisions into the browser. AC4 cannot render first-class relationships from the assignment surface because there is no assignment-version filter or batch lookup; assignment references are discoverable only after fetching an already-known POA&M detail. These gaps belong to TASK-433.6 AC2 and its explicit Phase-5 scope for authenticated compatible search and batched active links/assignment relationships. Per the user's stop rule, no Phase-6 code or tests were written. Required Phase-5 correction would expose bounded authorization-scoped contracts for (1) active plus completed POA&M relationships by assessment/finding IDs, (2) compatible POA&M search by authoritative assessment/finding context, and (3) POA&M relationships by immutable assignment-version IDs. Awaiting explicit authorization to amend Phase 5 or another server-contract decision.

Review handoff: local and remote branch SHAs match at `fd7548ad0b983e2412a5d1fa2b84ac397280daaa`; worktree is clean. MR !318 has no conflicts and received Phase-6 summary plus dark/light browser screenshots. Exact-head pipeline 2799822073 started for `fd7548ad`: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2799822073 (running when handed off).

Phase-6 review remediation completed. Legacy custom-check FAIL evidence now creates POA&Ms through stable finding identity and an authoritative observation reference without creating composite assessments or changing the FAIL result. The shared detail tray adds independent cursor pagination for linked findings, activity, and verification history; activity renders actor, timestamp, meaningful descriptions, and collapsed diagnostics. Assignment finding actions use typed Dioxus routing or synchronize the existing ComplianceView state while preserving exact bundle/version/system/policy/evidence identity.

Serial verification passed: default Rust formatting (`cargo fmt --all --check`), Web UI formatting, PostgreSQL-backed `poam_workflows` 17/17, Web UI unit tests 233 passed/1 ignored, Web UI package build, focused browser workflows 29g/29i/29m independently, and the full `checks.x86_64-linux.web-ui` harness. The full harness had no failures, so no baseline comparison at `42488e89` was required. JavaScript syntax and `git diff --check` also passed.

2026-08-29 final security review and push checkpoint: evidence reads now enforce system environment visibility before finding materialization. POA&M actions and evidence publishers use a deterministic advisory-lock protocol ordered by derivation publication key, system sentinel, finding key, then row locks; agent state publication, derivation/CVE writers, composite assessment writers, target authorization, and legacy finding materialization participate. Assignment relationship panels remount on system-scope changes. Serial verification passed: offline `cf-server` check; `poam_workflows` 20/20 against repository-isolated PostgreSQL; heartbeat 14/14; state 4/4; Web UI 233 passed/1 ignored; JavaScript syntax and `git diff --check`; and an authoritative Web UI check run passed before the final backend-only lock additions. The final post-lock authoritative rerun was interrupted at user request after a prior attempt failed because the onboarding coach overlay intercepted unrelated CVE test clicks; TASK-433 workflows 29g-29m passed in that failed attempt. Task remains In Progress pending a clean final authoritative check.

Phase 8 independent artifact review found a P2 dark-theme contrast defect in completed POA&M detail captures: the desktop Expand and Reopen actions are effectively unreadable. TASK-433.7 is returned to In Progress for a CSS-only presentation correction and focused visual verification.

Phase-8 dark-theme visual remediation completed. The shared POA&M tray now gives Expand an opaque subtle surface and readable primary text, and disabled Reopen uses readable muted text. Focused `task433-canonical-poam-lifecycle` update-mode browser verification passed, the post-fix captures passed independent visual review with no remaining P0-P2 finding, and the normal strict Web UI gate matched all 60 approved TASK-433 baselines. Lifecycle behavior and persistence semantics are unchanged.

Post-candidate review found a P2 interaction defect: `Remediation plan` is a distant section from `Save metadata`, and that mislabeled control is the only persistence path. The current browser tests reproduce the hidden dependency by filling the plan and later clicking `Save metadata`. TASK-433.7 is returned to In Progress for the focused UI and regression fix.
<!-- SECTION:NOTES:END -->
