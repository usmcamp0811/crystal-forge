---
id: TASK-440
title: Bring system configuration and flake exploration into full design parity
status: To Do
assignee: []
created_date: '2026-08-28 03:43'
updated_date: '2026-08-28 03:56'
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
  - docs/design/CrystalForge/app.jsx
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
- Support Generations and Commits modes, current/historical/never-deployed revision context, reload-safe deep links, and Back to current.
- Provide server-backed debounced search, All/Overridden/Changed filters with counts, bounded pagination, stale-response protection, loading/empty/error states, and a table aligned with the Modules/Evaluation/Drift cards without an unintended inner scroller.
- Show expandable option details with declared type, before/after change, complete winning and overridden definition provenance, source input/revision/path, and winner notes.
- Render scalar, package, structured list/attribute-set, submodule, opaque/function, and failed-evaluation values safely and according to declared type. Unknown or failed values must not be fabricated.
- Add real Modules and Evaluation summary cards plus the read-only module source tray. Tracked provenance can open the relevant flake/revision; untracked provenance remains visibly unavailable.
- Keep Drift behavior accurate and add the Overview store-path link into the current evaluation.

Evaluation lifecycle and comparison semantics:
- Config reads never launch Nix evaluation. Evaluation extraction occurs in the existing authorized evaluation/build job path and produces a reusable snapshot.
- When a requested never-deployed revision lacks a snapshot, Config shows an explicit unavailable state. An authorized explicit action may queue a new evaluation job or reuse an already queued/running job; the read request itself remains side-effect free. The UI distinguishes unavailable, queued, running, failed, and available states.
- In Generations mode, Changed compares the selected generation with the immediately preceding retained generation that has an evaluation snapshot. In Commits mode, Changed compares the selected commit with its Git first-parent snapshot for the same configuration. When no valid baseline snapshot exists, the UI says no comparison is available and does not report a zero-change result.
- Evaluation data required by a retained deployment generation remains queryable for as long as that generation is retained. An arbitrary commit snapshot remains queryable for as long as its flake timeline record is retained. Nix store garbage collection and branch rewrite/deletion must not silently remove retained snapshot metadata; genuinely missing or corrupt artifacts produce an explicit unavailable state.

Flake drawer scope:
- Add revision-scoped Commits, Systems, Modules, and Inputs tabs with counts and alert states.
- Make the selected commit govern every output pane and show revision identity plus host/module/input deltas against its Git first parent. Root commits and commits whose parent snapshot is unavailable show No previous revision rather than fabricated zero deltas.
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
- Add the `auto_latest` manual-deployment warning with Cancel, Continue on auto_latest, and Convert to manual and deploy outcomes, with truthful persisted/result state and idempotent retries.
- Remove only the duplicate inner Compliance bundle Edit action while retaining the authorized outer action.
- Ensure the file diff modal layers and receives input above the flake tray.
- Use full immutable commit SHA identity for persisted snapshots, API keys, caches, comparisons, navigation state, and row identity. Abbreviated SHAs are presentation only.

Backend, storage, and security outcome:
- Expose authorized, revision-specific, cached system evaluation data and revision-scoped flake output data required by these views.
- Evaluation data must come from the NixOS options tree rather than serializing `config`; retain declared types, safe values/errors, definition provenance, package metadata, module origins, evaluation identity/metrics, and comparison data.
- Search/filter/pagination and cross-revision diffs are computed server-side with bounded responses rather than transferring complete snapshots to the browser.
- Flake snapshots cover declared systems, exported modules and declarations/consumers, resolved lock inputs, and first-parent deltas without triggering per-host work while browsing.
- Snapshot persistence must provide content deduplication and bounded storage amplification equivalent to the design's content-addressed base-plus-delta intent. It must not store a separate complete multi-thousand-option corpus for every host and revision when that content is shared.
- Secret redaction occurs before unsafe data is persisted, indexed, diffed, logged, or serialized by an API. The boundary covers option values, nested/submodule values, package/collection elements, module defaults, evaluation errors, source metadata, and repository URLs containing credentials or tokens. Redacted data is neither returned nor searchable; masking only in Dioxus rendering is insufficient.
- Preserve environment visibility, existing mutation authorization, and deployed agent/builder compatibility. Hidden environments and unauthorized revisions use the application's existing non-disclosing authorization behavior.

Deep-link and interaction contract:
- The exact system identity, Config tab, revision mode, and full SHA survive hard reload and browser back/forward navigation. Unknown revisions and unavailable snapshots render explicit states; unauthorized or hidden targets do not disclose protected existence.
- A thorough UI/UX pass is part of this task. Every affected Rust surface matches the design example in the authoritative browser environment at 1920x1080 in light and dark themes, with narrow behavior exercised at 900x900. Keyboard assertions cover tab navigation, row expansion, revision controls, Escape behavior, modal focus trapping, and focus restoration.

The `modifiedFiles` metadata is anticipated and non-exhaustive. The implementation may modify the existing routing, modal, API, persistence, protocol, test, or migration files that actually own the required behavior, while remaining within this task's product scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The System Config tab defaults to the current deployed generation and supports generation, commit, reload-safe deep-linked revision, historical warning, never-deployed warning, unavailable local generation, and Back to current states using real revision-specific evaluation data.
- [ ] #2 Config read endpoints never launch Nix evaluation. A missing never-deployed snapshot is explicitly unavailable until an authorized evaluation action queues or reuses a job, and the UI distinguishes unavailable, queued, running, failed, and available states.
- [ ] #3 In Generations mode Changed compares against the immediately preceding retained generation with an evaluation snapshot; in Commits mode it compares against the selected commit's Git first-parent snapshot for the same configuration; a missing baseline displays no comparison available rather than zero changes.
- [ ] #4 Evaluation artifacts required by retained deployment generations remain queryable for the generation lifetime, arbitrary commit snapshots remain queryable for the flake timeline-record lifetime, and store GC or branch rewrite/deletion does not silently erase retained snapshot metadata.
- [ ] #5 Evaluated-options search is debounced and server-side across option path, safe rendered value, declared type, safe evaluation error, source path, and source input; redacted text is not searchable; filter counts remain revision-global while the result total reflects the active search and filter.
- [ ] #6 Evaluated options provide All/Overridden/Changed filters, bounded pagination, correct range/button/count behavior, loading/empty/unavailable/unauthorized/error states, and protection against older asynchronous responses overwriting a newer request.
- [ ] #7 Expandable option rows show declared type, the mode-defined comparison, complete winning and overridden definitions, source input/revision/path, and definition status without fabricating unknown or failed values.
- [ ] #8 Scalar, package, structured collection, submodule, opaque/function, and failed-evaluation values and diffs render safely and type-appropriately, including package additions/removals and explicit not-evaluated errors.
- [ ] #9 The Config Modules, Evaluation, Drift, and source-tray surfaces use real selected-revision data; tracked provenance opens the correct flake/revision, untracked provenance is non-navigable, and the Overview evaluation link opens the current revision.
- [ ] #10 Secret-bearing option values, nested values, collection/package elements, module defaults, evaluation errors, source metadata, and credential-bearing repository URLs are redacted before persistence, indexing, diffing, logging, or API serialization and cannot be recovered through API responses or search.
- [ ] #11 Snapshot persistence deduplicates shared evaluation content and bounds storage amplification so complete multi-thousand-option corpora are not independently duplicated for every host and revision where the content is shared.
- [ ] #12 The flake drawer provides Commits, Systems, Modules, and Inputs tabs whose counts, alerts, contents, selected full revision identity, and Git first-parent deltas update consistently when the selected commit changes.
- [ ] #13 A flake root commit or missing first-parent snapshot displays No previous revision and no fabricated delta; full SHAs key APIs, snapshots, caches, navigation, comparisons, and row identity, with a test proving two commits sharing a displayed SHA prefix remain distinct.
- [ ] #14 The flake Systems pane correctly classifies managed, declared-but-unmanaged, and managed-but-undeclared systems; shows output-collapse and pinned-revision warnings; opens managed systems at Config for the exact selected revision; and prefills registration for unmanaged declarations.
- [ ] #15 The flake Modules pane shows exported modules, descriptions, declaration counts, consumer/blast-radius counts, and expandable option path/type/default details from cached revision data.
- [ ] #16 The flake Inputs pane shows direct and resolved counts, source and locked revision, update age, follows/transitive details, tracked/channel state, bumps, stale-over-90-days state, and multiple-nixpkgs-revision warnings.
- [ ] #17 All flake system counts shown in page subtitles, table rows, cards, rollout denominators, drawer metrics, and removal warnings reconcile to one authoritative managed-system relationship.
- [ ] #18 The exact system, Config tab, revision mode, and full SHA survive hard reload and browser back/forward; unknown revisions and unavailable snapshots render explicit states; unauthorized or hidden targets follow existing non-disclosing authorization behavior.
- [ ] #19 Environment-to-flake navigation restores the originating environment panel on close; Config provenance opens the correct tracked flake/revision; Flake Systems opens Config at the selected revision; and unrelated later navigation does not retain stale context.
- [ ] #20 Pending deployment approval notifications open the exact system on Deploy; System Detail has no duplicate header Deploy/Rollback controls; History rollback opens Deploy with the exact generation selected; and selector labels read New commit and Previous generation.
- [ ] #21 Manual deployment from an auto_latest system offers Cancel, Continue on auto_latest, and Convert to manual and deploy; failed conversion queues no deployment; successful conversion followed by deployment failure reports the persisted manual policy and failed deployment; retries do not silently create duplicate deployments.
- [ ] #22 Only the duplicate inner Compliance bundle edit action is removed, the authorized outer edit action remains, and file diff modals render and receive interaction above the flake tray.
- [ ] #23 Authoritative browser coverage includes Config snapshot unavailable and API error, flake snapshot unavailable and API error, no previous flake revision, unauthorized/hidden environment, current/historical/never-deployed Config, expanded typed diffs, evaluation error, tracked/untracked provenance, all flake panes, cross-navigation, rollback, auto_latest, notification, modal-layering, and compliance states.
- [ ] #24 At the authoritative 1920x1080 viewport in light and dark themes and the 900x900 narrow viewport, affected states match commit eb5a1851 with no clipping, overlap, incorrect stacking, inaccessible controls, unintended inner scrolling, or table/card misalignment; assertions verify text, counts, revisions, warnings, actions, and navigation.
- [ ] #25 Keyboard coverage verifies tab order/navigation, row expansion, revision controls, drawer and modal Escape behavior, modal focus trapping, and focus restoration after closing drawers and modals.
- [ ] #26 New evaluation and flake-output APIs provide bounded server-side query/diff results, preserve supported agent/builder compatibility and environment authorization, avoid evaluation side effects on read paths and per-host evaluation during flake browsing, and return explicit unavailable/error states.
- [ ] #27 Targeted frontend and server tests, security/redaction tests, snapshot lifecycle and deduplication tests, auto_latest failure/idempotency tests, SQLx metadata/schema checks when applicable, the web-ui package build, the authoritative web-ui check, and broader Nix flake checks required by protocol, migration, packaging, or cross-package changes pass in the repository Nix development environment.
<!-- AC:END -->
