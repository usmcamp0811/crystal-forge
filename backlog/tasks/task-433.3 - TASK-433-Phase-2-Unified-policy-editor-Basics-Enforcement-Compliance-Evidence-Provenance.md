---
id: TASK-433.3
title: >-
  TASK-433 Phase 2: Unified policy editor
  (Basics/Enforcement/Compliance/Evidence/Provenance)
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-08-23 16:20'
labels:
  - design-parity
  - policy
  - web-ui
  - server
  - phase-2
dependencies:
  - TASK-433.2
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/components/PolicyEditor.jsx
  - docs/design/CrystalForge/data-mappings.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 435000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 2 of 8 (contextual only). Implements one common policy editor for every policy origin (custom, imported/STIG, cross-framework) from `PolicyEditor.jsx` in the design delta, replacing/unifying any existing per-origin editing paths without rewriting immutable version history or provenance.

## Explicit scope
- One editor shell with Basics, Enforcement, Compliance, Evidence tabs/sections and a read-only Provenance section for imported policies.
- Category changes preserve every existing rule and change guidance only; rules remain composable across categories.
- Zero mappings save as a valid "Unmapped" state; "mapped but no enforcement" and "No enforcement" are visually and semantically distinct states.
- Manual mappings support full CRUD; imported mappings/provenance remain read-only and survive reload.

## Explicit non-scope
No new enforcement kinds, Nix metadata typing, or composite execution (that is TASK-433 Phase 3/4). Do not make provenance editable. Do not rewrite unrelated TASK-422/compliance architecture.

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
- [ ] #1 All policy origins use one editor with Basics, Enforcement, Compliance, Evidence and read-only Provenance.
- [ ] #2 Category changes preserve every rule and change guidance only; cross-category rules remain composable.
- [ ] #3 Zero mappings save as valid Unmapped; mapped/no-enforcement and No enforcement are distinct states.
- [ ] #4 Manual mappings have permitted CRUD; imported mappings/provenance remain read-only and survive reload.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 2 remediation plan

1. Add authoritative policy-origin provenance to deployment-policy version summaries using one batched recursive query over all hydrated version IDs. Resolve direct source artifacts/source-object mappings and same-lineage `derived_from_version_id` ancestry without loading artifact bytes. Add additive server/web DTOs and exact-version frontend projection.
2. Preserve imported provenance into derived drafts through ancestry resolution, not copied mutable metadata. Add DB-backed query tests for custom/no provenance, imported provenance, derived-draft reload, and unchanged immutable source/artifact.
3. Restore honest no-enforcement semantics: use the existing minimal `custom_check { mode: all, rules: [] }` sentinel for custom no-enforcement, keep imported unbound/not-applicable semantics, remove extra fields the editor cannot round-trip, and add validator/evaluator/persistence regressions proving runtime skips rather than passes. Remove unsavable default UI-only rules.
4. Refactor the common PolicyEditorModal semantics only: semantic order Basics, Enforcement, Compliance, Evidence, Provenance; single four-value PolicyCategory; category-specific recommendations/off-category notices without rule mutation; independent origin/enforcement/mapping states; visible loading/error/empty states; exact policy version initialization.
5. Align mapping editability with the server whitelist (`provenance == manual`), render actual provenance labels, keep all non-manual rows read-only, and add DB mutation regressions for manual update/delete plus non-manual rejection/unchanged row/digest.
6. Preserve existing safe serializer blockers and Evidence persistence/validation/required_fields behavior; do not add Phase 3 rule kinds or execution.
7. Extend repository-backed browser workflows for custom Unmapped reload, category preservation, manual mapping CRUD reload, imported provenance/draft lineage, non-manual read-only mappings, imported needs-refinement, and mapped-without-enforcement.
8. Run required formatting, web/server/unit/DB/evaluator/browser/Nix checks. Perform independent requirements/backend/performance/UI/error-state/E2E review, update MR metadata and task evidence, then move TASK-433.3 to Review only if every AC and required check is proven.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 2 implementation started. Added server-side enforcement that policy requirement mapping update/delete operations only affect provenance=manual mappings; imported mappings now return HTTP 409 Conflict with a read-only error. Verified with `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check` and `nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-server`. Phase 1 GitLab CI remains pending/unverified per user authorization.

Phase 2 implementation committed as 4f38564a and pushed to origin/TASK-433-policy-poam-workflows (MR !318). Unified editor now presents Basics, Compliance, Enforcement, and Evidence sections; imported mapping provenance is read-only; mapping/enforcement state is explicit; empty enforcement saves as an explicit empty custom_check rule set; existing rules remain untouched by category changes. `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` passed; `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml` passed (201 passed, 1 ignored); targeted server test passed. The Nix web-ui package build completed; the full web-ui VM check was started but exceeded the execution timeout before completion and is unverified.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-23 14:19
---
Started Phase 2 under explicit user override of the pending Phase-1 CI gate. Phase-1 head is present and working tree is clean; MR !318 pipeline remains pending/running at this time. Scope is limited to unified policy editor and mapping/provenance behavior; later phases will not be implemented.
---
<!-- COMMENTS:END -->
