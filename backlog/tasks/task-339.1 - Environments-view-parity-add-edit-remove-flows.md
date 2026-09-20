---
id: TASK-339.1
title: Bring Environment Add/Edit modal to current design parity
status: In Progress
assignee:
  - gpt-5.6-terra
created_date: '2026-06-10 13:35'
updated_date: '2026-09-20 04:23'
labels:
  - design-parity
  - environments
  - web-ui
  - modals
  - accessibility
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - e79d0ad6
  - ad6589e1
  - TASK-358
  - TASK-446
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/styles.css
documentation:
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/styles.css
  - packages/web-ui/src/components/environments/environment_form_modal.rs
  - packages/web-ui/src/components/environments/mod.rs
  - packages/web-ui/src/views/environments_list.rs
modified_files:
  - packages/web-ui/src/components/environments/environment_form_modal.rs
  - packages/web-ui/src/components/environments/mod.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: enhancement
ordinal: 1711
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The production Environment Add/Edit flow is materially behind the current committed design in `docs/design/CrystalForge/components/EnvironmentsView.jsx`. The active Dioxus `EnvironmentFormModal` is a single vertically scrolling form without the design's persistent editor shell, navigation rail, section state, header context, and section-specific interaction model. Its binary-cache section is still a placeholder and is not connected to the existing authoritative environment/cache assignment contract. The policy and compliance controls are partly wired, but their presentation and assignment loading/error behavior must match the design without losing current version-aware semantics.

A legacy `edit_environment_modal.rs` remains in the source tree but is not re-exported by the active environments module. The implementation must confirm the active path before changing or removing it; it is not an alternative owner for this task.

## Goal

Make Create and Edit use one accessible, persistent sectioned environment editor that matches the current design hierarchy and uses authoritative production APIs. Preserve real server validation, authorization, environment metadata, cache assignment, deployment settings, gate-policy assignment, versioned compliance-bundle assignment, and delete safeguards.

## Current design scope

The shared modal has a persistent header, left rail, section body, and footer. Sections are Basics, Binary cache, Deployment, Policy enforcement, and Edit-only Danger zone. The header shows the selected color, environment name or `Add environment`, production/framework context when applicable, unnamed/validation state, concise mode-specific help, and an explicit close control. The rail keeps section selection visible, shows relevant badges/state, and preserves the draft while switching sections.

Basics includes name, preset colors, custom color, visible selected hex, description, production toggle/help, and validation. Deployment includes persisted Manual, Auto latest, and Pinned values with truthful explanations, Auto-sync flakes, and Require approval before deploy. Policy enforcement includes searchable gate-policy selection with removable chips and a real required compliance-bundle selector/detail state. The design's bundle fixture is not authoritative: retain current published-version identity, assignment mode, overlays, CAS/version behavior, authorization, and load/retry/error semantics.

Binary cache must use the existing API contract rather than a presentation-only selector. The current server exposes authenticated `GET /api/environments/:id/caches` and admin-authorized `PUT /api/caches/:id/environments`; the task must reconcile the environment's assigned cache set using these APIs, preserve no-cache state, show selected cache metadata without secrets, and distinguish assignment from optional cache creation/navigation. If an atomic environment-scoped assignment contract is proven necessary, record that concrete gap before adding backend work; do not fabricate cache state.

Danger zone is Edit-only. It must retain server-authoritative delete eligibility, system-assignment blockers, type-to-confirm behavior where supported by the current contract, truthful blocker errors, and no destructive action in Create mode.

## Lifecycle and safety

Create opens a blank design-default draft, validates required name, duplicate names, and valid color, submits the real mutation, keeps the modal open on errors, closes only after successful persistence, and refreshes the Environment view. Edit hydrates all authoritative values before allowing save; unresolved assignment data shows loading/error/retry and cannot be silently overwritten. Switching sections preserves unsaved values. Reopen proves persisted values. Reuse an existing repository dirty-state pattern only if one already governs these modals; do not invent a broad unsaved-change framework solely for this task.

Authorization remains server-owned. Read-only users must not receive false-active mutation controls. Dialog semantics, heading, close button, Escape/backdrop behavior, focus trapping and restoration, keyboard rail/field navigation, visible focus, labels for icon-only controls, and narrow-layout reachability are required.

## Non-goals

- Redesigning the full Environments cards/table surface beyond modal open, close, and refresh behavior.
- Redesigning the cache subsystem or cache destination editor; TASK-446 is narrowed to that separate surface.
- Redesigning policy editors or the compliance bundle lifecycle.
- Changing deployment engine semantics, assignment authorization, or version/CAS rules.
- Removing server safety checks to match a design fixture.
- General CSS cleanup, onboarding coach work, or unrelated environment API refactors.
- Creating another Environments parity umbrella task.

## Architectural constraints

- Use the active shared `EnvironmentFormModal` path for Create and Edit; do not create divergent modal implementations.
- Keep business decisions and validation in existing domain/API layers. UI rendering must not invent persisted values.
- Preserve exact policy and bundle version identity, published/current-version rules, assignment overlays, enforcement mode, CAS behavior, authorization, and retry/error states.
- Preserve cache secret redaction and admin authorization. Do not expose cache credentials or infer assignment from display-only environment fields.
- Keep delete eligibility and system-assignment guards authoritative on the server.
- Follow existing Dioxus component, API client, CSS token, modal, and test conventions.

## Dependencies

No backend implementation dependency is currently required for the modal parity slice: deployment metadata, gate-policy assignment, versioned compliance assignments, and cache/environment assignment endpoints exist on `dev`. Confirm their exact DTO and authorization behavior during implementation. TASK-446 is not a dependency; it owns only cache-destination editor parity after this ownership clarification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create and Edit use one persistent sectioned modal shell with contextual header, footer, left rail, and Basics, Binary cache, Deployment, Policy enforcement, and Edit-only Danger zone sections; the legacy edit component is not used as a competing implementation.
- [ ] #2 Basics matches the current design: name, preset/custom color picker, visible selected hex, description, production toggle/help, unnamed state, required-name validation, duplicate-name protection, valid-color validation, and server conflict/error display.
- [ ] #3 Create starts with documented defaults, preserves the draft while switching sections, closes only after a successful create, refreshes the environment list, and keeps the modal open with actionable server validation or mutation errors.
- [ ] #4 Edit hydrates all authoritative current values, including cache assignments, deployment metadata, gate policies, and versioned compliance-bundle assignments; loading or failed assignment hydration blocks unsafe save and provides retry without silently replacing existing values.
- [ ] #5 Binary cache uses the existing authoritative cache/environment APIs: no cache, select/clear an existing cache, selected cache type/status/metadata, assignment errors/retry, and a clear distinction between assignment and optional cache creation/navigation; no fake or local-only assignment state is persisted.
- [ ] #6 Deployment preserves authoritative Manual, Auto latest, and Pinned values with truthful selected-mode explanations, Auto-sync flakes, Require approval before deploy, and persisted reload behavior; it does not retain stale future/placeholder wording.
- [ ] #7 Policy enforcement provides searchable gate-policy selection, selected removable chips, clear count/state, and assignment errors/retry; required compliance bundles use real published/current version identity, enforcement mode, overlays, authorization, and existing CAS/version semantics without fabricated assignment state.
- [ ] #8 Edit-only Danger zone exposes the current server-authoritative delete eligibility and systems-assigned guard, uses the supported type-to-confirm and blocker messaging, and provides no destructive action in Create mode.
- [ ] #9 Save and reopen prove persistence for representative Basics, cache, deployment, policy, bundle, production, auto-sync, and approval values; partial assignment failure keeps the modal open and reports exactly which reconciliation failed.
- [ ] #10 The modal has accessible dialog semantics, an accessible heading and explicit Close, Escape/backdrop behavior, focus trap and focus restoration, keyboard-reachable rail and fields, visible focus, labeled icon-only controls, and no unreachable content at narrow widths.
- [ ] #11 Desktop and supported narrow/tablet/mobile widths preserve the design hierarchy without clipped rail, hidden footer, horizontal overflow, or inaccessible section controls in both light and dark themes.
- [ ] #12 Existing exact workflows are extended rather than duplicated: `14a-environments-add-modal` covers Create sections and save; the current Edit path covers hydration, section switching, save/reopen; `14b-environments-config-warning` covers missing builder/cache warning behavior; semantic assertions cover duplicate/invalid name, assignment load failure/retry, mutation failure, delete blocker/confirmation, and read-only authorization states.
- [ ] #13 Focused browser/API coverage proves real Create and Edit state transitions, cache assignment, policy/bundle assignment, server errors, delete safety, and responsive light/dark behavior; screenshots supplement semantic assertions and no production path uses fabricated fixture values.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace the active shared `EnvironmentFormModal` single-scroll layout with the committed-design editor shell: contextual header, keyboard-accessible rail, one selected section body, and persistent summary/footer. Reuse the shared dialog focus restore, initial focus, and focus sentinels.
2. Keep the one Create/Edit draft. Extend it with explicit cache destination IDs, not a singular fixture cache. Load the cache catalog and explicit assignments while Edit hydration is pending; retain global/effective cache visibility as read-only metadata. Reconcile changed cache IDs with the existing per-cache read-modify-replace API while preserving other environments.
3. Preserve existing server-owned basics/deployment/policy/bundle/delete semantics. Fix gate-policy search and replace obsolete deployment placeholder text. Keep versioned bundle reconciliation and prevent save until authoritative Edit hydration is ready.
4. Update the environments view lifecycle for cache/catalog loading, combined authoritative hydration, save-in-flight/error behavior, and refresh after successful operations; do not change cards/table behavior outside modal entry/refresh.
5. Add narrow scoped editor CSS in `app.css`, then extend workflow 14a and its manifest coverage with semantic Create/Edit/accessibility/error/responsive checks using method-specific mocks.
6. Run format, WASM check, JavaScript static check, focused workflow checks, and one selected authoritative environment workflow run. Update task notes, commit, push, open an MR to `dev`, and move the task to Review only with verified evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-20 audit: TASK-339.1 is retained and promoted as the canonical Environment Add/Edit modal task. Current active path is the unified `EnvironmentFormModal`; `edit_environment_modal.rs` is not re-exported by `components/environments/mod.rs`. Existing server contracts cover deployment metadata, gate-policy assignment, versioned compliance assignments, authenticated environment-cache reads, and admin-only cache assignment. TASK-446 is being narrowed to cache-destination editor ownership to prevent duplicate implementation.

The current cache assignment API is real but asymmetric: authenticated environment reads are exposed at `/api/environments/:id/caches`, while admin assignment is performed through `PUT /api/caches/:id/environments`. The task therefore requires UI reconciliation through the existing contract and only allows backend work if an atomic environment-scoped gap is proven. Existing version-aware bundle assignment semantics remain mandatory.

2026-09-19 preflight: Created clean dedicated worktree `/home/mcamp/code/crystal-forge/TASK-339.1-environment-modal-parity` on branch `TASK-339.1-environment-modal-parity` from `origin/dev` `e304867c43abc8a7d8efe1f01e71af64cca67d8b`. The dev worktree has an unrelated untracked session file and another task worktree has unrelated changes; this task worktree is clean.

2026-09-19 implementation progress (not review-ready): Replaced the active shared modal body with a sectioned `pe-shell` editor, contextual header/rail/footer, and Edit-only danger section. Added dialog semantics, Escape, focus sentinels/restoration, responsive editor CSS, real policy search, and truthful deployment descriptions. Extended the draft with explicit multi-cache IDs; Edit hydration now reads the cache catalog plus each cache membership, and save reconciles changed cache memberships by read-modify-replace while preserving unrelated environment IDs. This preserves the current many-to-many cache model; global caches remain global and are not modified. Bundle reconciliation remains version-aware and save stays disabled during Edit assignment hydration. Added basic 14a semantic checks for dialog/rail section navigation and Escape/focus return.

Verification completed: `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` (passed, repository warnings); `nix develop -c node --check checks/web-ui/tests/integration-test.js` (passed); `git diff --check` (passed). Preview/browser verification is blocked: port 8080 and API port 3445 are owned by TASK-440's preview (PIDs 1077 and 9406). The fixed-port launcher cannot safely start an isolated TASK-339.1 preview, so no browser workflow, screenshot, focused authoritative run, commit, push, MR, or Review transition has been performed.

2026-09-19 progress: Implemented the sectioned shared Environment editor shell, multi-cache explicit assignment hydration/reconciliation, policy search, deployment copy, dialog focus/accessibility, responsive CSS, and initial 14a semantics. Fast checks passed: WASM cargo check, Node syntax check, and git diff --check. Blocked from browser/focused workflow verification: fixed ports 8080/3445 are owned by TASK-440 (PIDs 1077/9406), so no isolated TASK-339.1 preview can start. No commit, push, MR, or Review transition performed.

2026-09-20 verification update: the single permitted selected authoritative run `CF_UI_TEST_STEPS="14a-environments-add-modal" nix build --impure .#checks.x86_64-linux.web-ui --no-link -L` executed the environment workflow but failed before screenshot capture. The rendered cache section was present, but `.field label { text-transform: uppercase; }` made Playwright's exact-case `getByText("Explicit cache destinations")` fail against rendered uppercase text. The test now uses case-insensitive regex locators for that label and the similarly styled `Default deployment mode` label. `nix develop -c node --check checks/web-ui/tests/integration-test.js` and `git diff --check` pass after the correction. No browser rerun was performed because the stated focused-build budget is exhausted. `cargo fmt --check` also reports pre-existing formatting drift in unrelated `flake_timeline.rs` and `coach_panel.rs`; no unrelated formatting was changed. TASK remains In Progress and is not review-ready until browser evidence is re-established.

2026-09-20 final authorized browser result: `CF_UI_TEST_STEPS="14a-environments-add-modal" nix build --impure .#checks.x86_64-linux.web-ui --no-link -L` failed (exit 1), with 0/1 screenshots captured. The rendered page again contained `EXPLICIT CACHE DESTINATIONS`, but the executed workflow still used the old exact-text assertion and failed with `Expected cache assignment section`. The current worktree test at `checks/web-ui/tests/integration-test.js:10887` uses `/explicit cache destinations/i` and `/default deployment mode/i`. `nix show-derivation` for the failed driver proves that it copied its tests from the pre-existing `/nix/store/v47r9vzgvazdy19wiybjm2yqjqzdryr4-tests` input, so the current corrected test was not rebuilt into the VM driver. Classification: test-harness/build-input staleness, not a product defect; the DOM body proves the cache section rendered. No additional VM run was made. TASK remains In Progress; no commit, push, MR, or Review transition was performed. Unrelated rustfmt drift remains limited to `components/flake/flake_timeline.rs` and `components/onboarding/coach_panel.rs` and was not modified.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-09-20 03:16
---
Canonical ownership note: TASK-339.1 now owns only Environment Add/Edit modal parity. TASK-446 was narrowed to Cache destination editor parity and no longer owns environment modal implementation. Archived TASK-358 recorded parity against an older design and explicitly deferred several environment contracts; the current committed design and current production contracts have diverged, so TASK-358 completion is historical evidence, not acceptance for this task.
---
<!-- COMMENTS:END -->
