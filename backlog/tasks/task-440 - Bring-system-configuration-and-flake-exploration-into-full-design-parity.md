---
id: TASK-440
title: Bring system configuration and flake exploration into full design parity
status: To Do
assignee: []
created_date: '2026-08-28 03:43'
labels:
  - design-parity
  - web-ui
  - backend
  - systems
  - flakes
dependencies: []
references:
  - git commit eb5a18513623890e9dac1e8a74565078243288a8
  - git parent cfae4f4a33815c72059d309a547672f8c9039747
documentation:
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/components/AddSystemModal.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/data-config.js
  - docs/design/CrystalForge/data-flake-explorer.js
  - docs/design/CrystalForge/data-flakes.js
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/src/state/navigation_focus.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/assets/app.css
  - packages/default/crates/cf-server/src/
  - packages/default/crates/cf-builder/src/
  - packages/default/crates/cf-protocol/src/
  - checks/web-ui/
priority: high
type: feature
ordinal: 449000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the complete Rust frontend and backend product behavior represented by design commit `eb5a18513623890e9dac1e8a74565078243288a8` (compared with parent `cfae4f4a33815c72059d309a547672f8c9039747`). The design commit is the authoritative visual and interaction reference; fixture randomness and the changed `.thumbnail` are not product requirements.

System Config scope:
- Replace the static rendered-module presentation with a revision-aware Evaluated options explorer.
- Support Generations and Commits modes, current/historical/never-deployed revision context, deep-linked revisions, and Back to current.
- Provide server-backed debounced search, All/Overridden/Changed filters with counts, bounded pagination, stale-response protection, loading/empty/error states, and a table aligned with the Modules/Evaluation/Drift cards without an unintended inner scroller.
- Show expandable option details with declared type, before/after change, complete winning and overridden definition provenance, source input/revision/path, and winner notes.
- Render scalar, package, structured list/attribute-set, submodule, opaque/function, and failed-evaluation values safely and according to declared type. Unknown or failed values must not be fabricated.
- Add real Modules and Evaluation summary cards plus the read-only module source tray. Tracked provenance can open the relevant flake/revision; untracked provenance remains visibly unavailable.
- Keep Drift behavior accurate and add the Overview store-path link into the current evaluation.

Flake drawer scope:
- Add revision-scoped Commits, Systems, Modules, and Inputs tabs with counts and alert states.
- Make the selected commit govern every output pane and show revision identity plus host/module/input deltas against the preceding commit.
- Reconcile declared configurations with managed systems, including managed, declared-but-unmanaged, and managed-but-undeclared states; include warnings for output collapse and systems pinned to older revisions.
- Let managed rows open that system's Config tab at the selected revision and unmanaged rows open system registration prefilled with configuration/hostname, flake, and branch.
- Show exported modules, declared options, consumer/blast-radius counts, and expandable declaration details.
- Show direct and resolved inputs, lock revisions, source, age, follows/transitive data, tracked/channel state, revision bumps, stale inputs, and multiple nixpkgs revision warnings.
- Derive all flake system totals consistently from the authoritative managed-system relationship across subtitles, lists, cards, rollout displays, and removal warnings.

Cross-surface and smaller parity scope:
- Make environment flake chips open the flake and restore the originating environment panel when the drawer closes.
- Preserve Config-to-flake provenance navigation and Flake-to-Config revision context.
- Deep-link pending deployment approval notifications to the exact system Deploy tab.
- Remove duplicate System Detail header Deploy/Rollback actions; route History rollback into Deploy with the exact previous generation selected; rename selectors to New commit and Previous generation.
- Add the `auto_latest` manual-deployment warning with Cancel, Continue on auto_latest, and Convert to manual and deploy outcomes, with truthful persisted/result state on failure.
- Remove only the duplicate inner Compliance bundle Edit action while retaining the authorized outer action.
- Ensure the file diff modal layers and receives input above the flake tray.
- Preserve stable commit identity even where fixture or abbreviated SHAs collide.

Backend and domain outcome:
- Expose authorized, revision-specific, cached system evaluation data and revision-scoped flake output data required by these views.
- Evaluation data must come from the NixOS options tree rather than serializing `config`; retain declared types, safe values/errors, definition provenance, package metadata, module origins, evaluation identity/metrics, and comparison data.
- Search/filter/pagination and cross-revision diffs must be computed server-side with bounded responses rather than transferring complete snapshots to the browser.
- Flake snapshots must cover declared systems, exported modules and declarations/consumers, resolved lock inputs, and deltas against the prior commit without triggering per-host work while browsing.
- Preserve environment visibility, existing mutation authorization, deployed agent/builder compatibility, and secret redaction. Never expose secret option values, repository credentials, authorization data, or hidden-environment information.

A thorough UI/UX pass is part of this task: every affected Rust surface must match the referenced design example pixel-for-pixel in representative light and dark states, while retaining complete loading, empty, unavailable, unauthorized, error, responsive, keyboard, and focus behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The System Config tab defaults to the current deployed generation and supports generation, commit, deep-linked revision, historical warning, never-deployed warning, unavailable local generation, and Back to current states using real revision-specific evaluation data.
- [ ] #2 Evaluated options support debounced server-side search across option and provenance fields, All/Overridden/Changed filters with accurate counts, bounded pagination, correct loading/empty/error states, and protection against stale asynchronous responses.
- [ ] #3 Expandable option rows show declared type, accurate previous-generation changes, complete winning and overridden definitions, source input/revision/path, and definition status without fabricating unknown or failed values.
- [ ] #4 Scalar, package, structured collection, submodule, opaque/function, and failed-evaluation values and diffs render safely and type-appropriately, including package additions/removals and explicit not-evaluated errors.
- [ ] #5 The Config Modules, Evaluation, Drift, and source-tray surfaces use real selected-revision data; tracked provenance opens the correct flake/revision, untracked provenance is non-navigable, and the Overview evaluation link opens the current revision.
- [ ] #6 The flake drawer provides Commits, Systems, Modules, and Inputs tabs whose counts, alerts, contents, selected revision identity, and prior-commit deltas update consistently when the selected commit changes.
- [ ] #7 The flake Systems pane correctly classifies managed, declared-but-unmanaged, and managed-but-undeclared systems; shows output-collapse and pinned-revision warnings; opens managed systems at Config for the exact selected revision; and prefills registration for unmanaged declarations.
- [ ] #8 The flake Modules pane shows exported modules, descriptions, declaration counts, consumer/blast-radius counts, and expandable option path/type/default details from cached revision data.
- [ ] #9 The flake Inputs pane shows direct and resolved counts, source and locked revision, update age, follows/transitive details, tracked/channel state, bumps, stale-over-90-days state, and multiple-nixpkgs-revision warnings.
- [ ] #10 All flake system counts shown in page subtitles, table rows, cards, rollout denominators, drawer metrics, and removal warnings reconcile to one authoritative managed-system relationship.
- [ ] #11 Environment-to-flake navigation restores the originating environment panel on close; Config provenance opens the correct tracked flake/revision; Flake Systems opens Config at the selected revision; and unrelated later navigation does not retain stale context.
- [ ] #12 Pending deployment approval notifications open the exact system on Deploy; System Detail has no duplicate header Deploy/Rollback controls; History rollback opens Deploy with the exact generation selected; and selector labels read New commit and Previous generation.
- [ ] #13 Manual deployment from an auto_latest system offers Cancel, Continue on auto_latest, and Convert to manual and deploy; each outcome preserves authorization and persisted policy/deployment truth, and partial failures are surfaced without false success.
- [ ] #14 Only the duplicate inner Compliance bundle edit action is removed, the authorized outer edit action remains, and file diff modals render and receive interaction above the flake tray.
- [ ] #15 New evaluation and flake-output APIs provide bounded server-side query/diff results, preserve supported agent/builder compatibility and environment authorization, avoid per-host evaluation during flake browsing, and never expose secrets, credentials, authorization data, or hidden-environment data.
- [ ] #16 Authoritative browser coverage exercises representative current, historical, never-deployed, expanded diff, evaluation-error, tracked/untracked provenance, flake Systems/Modules/Inputs, cross-navigation, rollback, auto_latest, notification, modal-layering, and compliance states in light and dark themes plus a narrower viewport.
- [ ] #17 For every affected state, the Rust UI matches commit eb5a1851 pixel-for-pixel with no clipping, overlap, incorrect stacking, inaccessible controls, unintended inner scrolling, or table/card misalignment, and behavioral assertions verify text, counts, selected revisions, warnings, actions, and navigation destinations.
- [ ] #18 Targeted frontend and server tests, SQLx metadata/schema checks when applicable, the web-ui package build, the authoritative web-ui check, and broader Nix flake checks required by any protocol, migration, packaging, or cross-package changes all pass in the repository Nix development environment.
<!-- AC:END -->
