---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
status: In Progress
assignee:
  - openai
created_date: '2026-10-17 16:35'
updated_date: '2026-10-24 23:06'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/ui/frontend-navigation-and-shared-patterns.md
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/ProfileView.jsx
modified_files:
  - checks/integration-access/scenarios/fragments/shell-navigation.py
  - checks/integration-access/scenarios/fragments/shell-navigation.cjs
  - checks/integration-access/scenarios/fragments/preferences.py
  - checks/integration-access/scenarios/fragments/preferences.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 516000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Selected Access shell/profile-preference coverage is integrated in the foundation MR335. Preserve the narrowly owner-authorized preference queue-subscription and mobile dismissal/stacking corrections, real per-user persistence, semantic state/request-bound assertions, and keyboard/sidebar proofs. Recorded Access37 results prove the selected subset, not all shell states, the cross-user delayed-response race, or desktop visual parity. Keep the remaining criteria and canonical UI punch-list scope open for future work. Coordinator owns composition/registration; fragments extend the existing Access lifecycle without a new scenario or check.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes the named shell/preferences fragments into lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 Every accepted AppShell route is reached through navigation and renders its heading; unknown routes show NotFound/accepted return path and protected routes redirect unauthenticated users to /login
- [ ] #3 Keyboard Tab/Enter, sidebar persistence and narrow mobile menu/Close menu satisfy accepted requirements; unresolved states remain explicitly blocked
- [ ] #4 At least one approved desktop-dark Shell.jsx comparison executes with a justified unchanged threshold and retained reference/actual/diff images
- [ ] #5 Profile and notification preferences persist per user through database read/reload without changing user B; invalid-input behavior is asserted only under accepted authority
- [ ] #6 The coordinator executes the focused lifecycle-local-bootstrap gate with these fragments and records exact revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Dedicated worktree/branch TASK-490.4-access-preferences, clean base df11c800. Sol owns shared helpers/facts/composition/registration and VM execution. Worker owns only shell-navigation.py/.cjs and preferences.py/.cjs.
1. Use actual approved Shell/Profile references and existing real same-run users/receipts to implement compatible local-auth route, return-navigation, protected/unknown-route, profile and notification-preference persistence/identity-isolation cases. Preserve Access23 and no product changes.
2. Add cancellation and relevant failed-read/save, empty/loading/denied assertions only where existing API/design authority is explicit. Report exact decision/helper gaps without weakening other cases. Use browser semantic locators and actual requests/persistence; no mocked producers/results.
3. Define required authored fact/receipt interfaces for coordinator. Keep independent browser contexts and one existing local lifecycle; no task-specific derivation. Include approved desktop shell comparison with a justified fixed threshold if supported; otherwise record exact missing authority rather than guessing.
4. Run cheap syntax/static tests only in worker. Sol composes cases and runs complete Access once coherent; commit separately after execution evidence. Worker starts no server/VM.

Owner explicitly authorized the confirmed preference pending-queue reactive-subscription repair and the mobile backdrop's intended fixed/full-viewport geometry. Preserve account ownership, generation fencing, serialized in-flight/queued updates, errors/retry and desktop/keyboard behavior. Prove actual changed-value PATCH/authoritative persistence/reload/user-B isolation and no self-sustaining save loop; actual normal backdrop/Close/link dismissal, reopening, controls above backdrop and restored page interaction. Also correct only the invented zero-PATCH expectation on already-selected segments: approved components invoke change callbacks and promise state correctness, not a no-request transport contract. Do not add production deduplication to satisfy that test. Keep product corrections and expectation correction separately reviewable, existing Access lifecycle/component tests only, focused then normal owner and affected shared owners. No API/schema/state framework or UI redesign.

Verification delegation in existing locked TASK-490.1-integration-foundation: reuse verified frontend8090/API3490/PG5490 without restart/reset. Extend only existing shell/preferences fragments for serialized rapid queued saves, bounded idle request monitoring, retry, full-viewport mobile overlay and restored interaction. Retain original data/security/failure assertions. TASK-490.4 owner authorization controls the selected-segment expectation correction; compare semantic values rather than write timestamps where a permitted same-value PATCH updates metadata. Run independent real browser contexts plus read-only SQL against task fixtures; report focused proof separately from coordinator-owned normal Access.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner now explicitly selects the independent shell/profile/notification-preference contribution. Existing Access23 cases and controller setup are implemented at df11c800. No obsolete bootstrap/Rust startup blocker applies. Worker owns only the four named semantic Python/browser fragments; Sol owns facts, composition, registration and VM execution. Approved Shell/Profile references control expectations; missing decisions block only their dependent assertions. No product UI change is authorized by this test-only child.

Source-confirmed preference cause: AppShell save effect takes pending work only via write().take(), never subscribing through read(); Dioxus0.7.3 writes notify subscribers but do not create an effect dependency. Trace's first changed Light click updates DOM but emits no PATCH. Callback context remains AppShell-owned; this is not a no-op or guessed stale callback. Existing per-user/in-flight/generation guards must remain. Mobile close sizing is already repaired; the separate backdrop currently has375×0/static geometry from missing intended fixed/inset styling. New owner authorization covers those exact corrections, not broad parity work.

Authorized corrections now implemented in existing coordinator worktree at a03bb8e3 plus dirty inputs. AppShell subscribes to pending queue via read before taking it, preserving all account/generation/in-flight guards and avoiding empty-queue writes. Mobile backdrop and drawer explicitly retain intended fixed geometry and z40/z50 because packaged positioning/stacking utilities are absent. First backdrop-only proof exposed drawer interception; no assertions were weakened. A full frontend restart (same cf-environment-web session, API/PG untouched) was required because fresh contexts retained the old compiled layout after hot reload. Focused /tmp/opencode/cf-shell-preference-repair/run11/result.json passed normal mobile Close/backdrop/reopen/link/page interaction and keyboard. Earlier focused preference runs prove real changed PATCH200, exact GET/reload/SQL, serial queue coalescing, failed-write no mutation before successful retry, and bounded idle no-loop observation. Preview fixture jpark has no local password: exact B rows unchanged but independent B sign-in awaits normal Access. Unsupported selected-segment zero-PATCH assertion replaced by semantic projection/state preservation with at most one callback write, not production deduplication. Normal Access running in tmux cf-access-authorized-corrections; no pass yet. Full native UI cargo test passed774/0failed/1ignored; layout subset45passed. Preview ready http://127.0.0.1:8090/profile on sledge, API3490, owned PG5490. Durable MR images/final same-head proof remain outstanding.

Normal Access now PASSED at a03bb8e30033ef172ceea5d84f890e417e84945b-dirty: nix develop --command sh packages/ci/integration-owner-build.sh integration-access, exit0,37case records/37distinctnames, freshVMdriver120.05s. Actual independently signed-in A/B baselines, changed appearance/notification persistence, authoritativeSQL/reload and B unchanged passed. Existing grouped cases include all named failure/retry/loading/idle and keyboard/mobile/sidebar phases; receipts confirm execution, not additional case counts. First run retained31pass/1fail because the test emitted two new object values under the same immutable idle receipt key. Corrected only checkpoint-specific keys (selected-field/queue-drained/before-retry/after-retry), no helper/production/assertion weakening. Failedrun artifact /nix/store/rd3cyx0chwkf8cjrsbjfmjl1jk8xr7kb-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run; final exported result/meta under integration-evidence/integration-access/lifecycle-local-bootstrap and log/status /tmp/opencode/cf-access-authorized-corrections-v2.*. Preview remains ready8090, new frontendPID13985 samecf-environment-web, API19128/PG4962unchanged. Changes uncommitted/unpushed; finaldurableMR/designcomparison and shared-owner verification remain pending.

Merge reconciliation retains source-only historical readiness evidence: bootstrap b1095c8f, rebased bba718befbc183863781656df2df272e5f735d5a, runtime riky1mm0nn185wb26abw7zzshnbml0fp and292 migrations proved bootstrap only, not shell/preferences. Target dev's later human selection, In Progress status, narrow repair approvals and dirty a03bb8e3 Access37 proof supersede old Backlog/unselected summaries. No future criterion is checked by resolving this record.
<!-- SECTION:NOTES:END -->
