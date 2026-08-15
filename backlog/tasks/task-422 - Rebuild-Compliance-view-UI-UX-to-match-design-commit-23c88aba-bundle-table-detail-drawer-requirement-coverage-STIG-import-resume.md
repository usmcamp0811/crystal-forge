---
id: TASK-422
title: >-
  Rebuild Compliance view UI/UX to match design commit 23c88aba (bundle table,
  detail drawer, requirement coverage, STIG import resume)
status: In Progress
assignee:
  - Matt Camp
created_date: '2026-08-15 17:41'
updated_date: '2026-08-15 23:24'
labels: []
milestone: m-22
dependencies:
  - TASK-418
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/315'
  - 23c88aba
  - docs/design/CrystalForge/screens/compliance-redesign/cb-1.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-2.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-3.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-4.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-5.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-6.png
  - docs/design/CrystalForge/screens/compliance-redesign/cb-7.png
documentation:
  - backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/ImportStigModal.jsx
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/data-compliance.js
  - docs/design/CrystalForge/styles.css
  - docs/design/CrystalForge/app.jsx
priority: high
type: enhancement
ordinal: 417000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rebuild the production Compliance view (Dioxus `web-ui`) so it matches the refreshed compliance UI/UX design example introduced in commit `23c88aba` ("refinement of the compliance ui/ux"). Add or extend backend APIs only where the frontend cannot be built correctly without them.

**Blocked on MR !315 being merged.** <https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/315> (TASK-418) delivers the normalized framework/requirement/policy-mapping model, the bundle requirement-coverage endpoint, and the focused web-ui check harness this work consumes. Do not start until it is merged into `dev`; branch from `dev` after the merge.

## Why

Today the Compliance view is a fixed 320px bundle rail plus an inline detail column. It does not scale past a handful of bundles, has no requirement-coverage surface, has no way to see which policies enforce a requirement, and loses all STIG import progress if the operator closes the wizard. The design example replaces that with a dense, searchable bundle table, a single bundle detail drawer that also hosts requirement coverage, direct policy drill-in from a requirement, and a pausable/resumable STIG import.

## Source of truth

The design example at commit `23c88aba` is the visual and interaction source of truth. The full specification — component-by-component layout, exact typography/spacing/colour values, design-mock-to-Crystal-Forge domain mapping, verbatim CSS to add, required backend changes, and the reviewer verification endpoint table — is in the linked spec document:

`backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md`

Read that document and the design files at `23c88aba` before planning. Do not copy mock-only data shapes (`groupBundlesByLineage`, `POLICIES`, `bundleQuickStats`) into production; map them to real server models per §2 of the spec.

## Scope

1. Replace the bundle rail with a full-width, framework-filtered, searchable bundle table.
2. Move bundle detail into a right-hand drawer with `overview` and `coverage` views.
3. Add the requirement-coverage view backed by the authoritative coverage endpoint, including "Enforced by" policy chips that open the policy drawer as an overlay.
4. Densify the per-system drilldown table inside the drawer.
5. Add STIG import pause/resume with the paused-import callout.
6. Add the new CSS utilities in both themes.
7. Two additive backend changes: per-bundle aggregate score + applicable system count on the bundle list, and `policy_id` on coverage mapping rows.

Existing production functionality that has no design counterpart (XCCDF version selector, bundle version trust/publish/create-draft actions, assignment panel, export modal, new/edit bundle modals, evidence drawer) must be preserved and relocated per spec §5.2 — not removed.

## Reviewer verification

Spec §9 contains the endpoint/URL table a reviewer uses to put the running design example and a running Crystal Forge side by side (design example on port 8081 via `docs/design/CrystalForge/serve.sh 8081`, Crystal Forge via `run-ui-dev` on port 8080 with the same golden fixture), the 13 UI states to compare in dark and light, and the backing API endpoints to confirm the UI is rendering server data.

## Notes for the implementer

- This is a large single-view change. If the merge request grows beyond reviewable size, split it along the numbered scope boundaries above and record the split on this task before opening additional MRs — do not defer acceptance criteria to an unspecified follow-up.
- `PolicyDrawer` (`packages/web-ui/src/views/policies.rs`) is currently private and must be made reusable from the Compliance view without behavioural change.
- Browser/WASM constraints apply to the import-draft persistence; see spec §8.3.
- Ask before changing anything about the normalized requirement/mapping model itself — that is owned by TASK-418.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bundle list renders as a full-width card table (5 columns: Bundle, Framework, Version, Score, action) replacing the 320px BundleCatalog rail, matching spec section 4 incl. colgroup widths, score dot, counts, framework chip, version + publication-state chip, score % + system count, and the icon-only row action. Second bundle line shows '{requirement_count} requirements · {policy_count} policies · {n} revisions' — never display control_count or the word 'controls' (spec §2/§4.3)
- [ ] #2 Framework filter chips (All + one per distinct framework, each with a count, ordered by count descending then framework name ascending for a deterministic tie-break) and a 'Search bundles…' input filter together with AND semantics; the clear button and '{matching} of {total}' count appear only while the query is non-empty
- [ ] #3 When filters match no bundle the table is replaced by the q-empty state reading 'No bundles match “{query}”.' and no bundle rows are rendered
- [ ] #4 Clicking a bundle row or its action opens a right-hand drawer (fl-tray backdrop + aside, width min(900px,96vw)) in overview view; the action does not double-fire the row handler; backdrop click and the header close button close the drawer
- [ ] #5 Drawer overview renders, in order: card-less BundleHeader, a 5-column stat-strip-flush strip (Overall score, Pass, Warn, Fail, Waiver) using section 4.4 colour thresholds, a collapsed-by-default Revisions disclosure shown only when the bundle has more than one version, the requirement-coverage summary row, and the Systems drilldown
- [ ] #6 A single selected_bundle_version_id signal (rename the existing selected_export_version_id — do not add a second signal) is the sole source of truth: revision buttons, the XCCDF selector, systems, coverage and evidence requests, and trust/publish/draft actions all read/write it per spec §5.2a; selecting a revision re-scopes stat strip, coverage and Systems to that version
- [ ] #7 The requirement-coverage summary row shows '{framework} · {total} requirements · derived from mapped policies, not policy tags' with full/partial/unmapped chips from GET /api/v1/compliance/bundle-versions/{id}/requirement-coverage, and renders the 'No requirement catalog modeled for {framework} yet.' block when the report has zero requirements
- [ ] #8 Clicking the requirement-coverage row switches the drawer to the coverage view with a back arrow header titled 'Requirement coverage' plus the bundle name; the back arrow returns to overview
- [ ] #9 Coverage view renders the All/Full/Partial/Unmapped segmented filter with live counts, a 'Filter requirements…' search matching external ID and title, rows grouped by top-level requirement ancestor with the group heading suppressed when the group is a single root row, per-row Fully covered / Partially covered / Unmapped chips, and the centred 'No requirements match.' state when filters exclude everything
- [ ] #10 Enforced-by policy drill-in follows spec §5.4a: PolicyDrawer extracted to a shared component without behaviour change (existing Policies checks stay green), resolved through the same policies_api::load_policies() path PoliciesView uses, looked up by BundleCoverageMapping.policy_id, loaded lazily (never one request per coverage row), standard loading state, and an explicit error state when policy_id cannot be resolved
- [ ] #11 The Systems drilldown inside the drawer uses sys-table compact sys-table-dense with the spec colgroup, right-aligned mono Pass/Warn/Fail/Waiver cells with the specified conditional colours, a 40px score bar, and an icon-only 'View evidence' action that opens the existing evidence drawer without double-firing the row click
- [ ] #12 GET /api/v1/compliance/bundles returns applicable_system_count and optional aggregate_score computed set-based for the current published (else current draft) version, consistent with ComplianceRollupTotals.overall_score for that version. Reuse the existing rollup/totals functions rather than reimplementing the scoring formula in SQL; no per-bundle query or per-row request. Spec §7a required tests (no systems, none evaluated, all pass, mixed, published+draft, draft only) are covered
- [ ] #13 BundleCoverageMapping includes policy_id on both server and web-ui models, populated from the existing coverage query, and the Enforced-by chips resolve the policy through it
- [ ] #14 STIG import persistence follows spec §8.3.2: versioned draft (version: 1) under 'cf-stig-import-draft' with step, filename, expected SHA-256, bundle/environment metadata, refine cursor and selected/refined control identity, and preview payload only when it fits MAX_STIG_IMPORT_DRAFT_BYTES = 2 MiB; raw bytes never persisted; resumes at the stored step, clears on completion or empty-upload, exposes 'Discard draft' + pause close button on review/reconcile/refine, never closes on backdrop
- [ ] #15 When a paused draft exists and the modal is closed, the Compliance page renders the sd-callout-warn paused-import callout with Discard and Resume actions and the Import/Export menu item label changes to 'Resume STIG import…'
- [ ] #16 Resumed import source-file handling per spec §8.3.2: commit-capable steps require re-selecting the source file, SHA-256 matched against the persisted expected hash, different artifacts rejected, and the final import action disabled until the matching artifact is reattached. When the preview payload cannot be restored, resume reopens at the upload step preserving bundle name and environments with an explicit re-select prompt. Corrupt/wrong-version drafts are no-draft, never panic
- [ ] #17 The post-TASK-418 STIG state machine is preserved (spec §8.3.1): pause/resume wraps the production workflow (upload → native-review → review → reconcile → refine → final-review → committing → done); native reconciliation, exact technical reuse, shared-implementation reconciliation, reviewed decisions and final-review semantics remain intact and are not replaced by the simpler design-example wizard
- [ ] #18 The CSS additions in spec section 6 (cf-policy-link, cf-fw-chip, stat-strip-flush, sys-table-dense, sys-table-fixed, sd-callout-warn, anchor colours) are added to packages/web-ui/assets/app.css and render correctly in both dark and light themes
- [ ] #19 Pre-existing Compliance functionality is preserved and relocated per spec section 5.2: XCCDF version selection, bundle version trust/publish/create-draft actions with admin gating, assignment panel, export modal, new/edit/delete bundle modals, and the evidence drawer all still work
- [ ] #20 Loading, empty, error, admin-authorization and stale-response behaviour is preserved for bundle list, systems rollup, evidence and coverage fetches, including generation-guarded responses so a slow reply cannot overwrite a newer selection
- [ ] #21 Existing compliance steps in checks/web-ui/tests/integration-test.js (29, 29a, 29b, 29c, 29d, 29e) are updated to assert the new layout, and a focused NixOS check following the checks/web-ui-reconciliation pattern proves the new bundle table, drawer, coverage view and paused-import callout with passing results.json and dark + light screenshots
- [ ] #22 checks/web-ui/coverage-manifest.json is updated for any added or renamed step, and checks/web-ui/design-parity/manifest.json still resolves the compliance view to /compliance and the refreshed ComplianceView.jsx
- [ ] #23 The implementation follows the phased sequence in spec §10 (server contract → reusable drawer + version signal → table/drawer → coverage → STIG pause/resume → final pass) with a commit and verification at every STOP point, and applies the spec 'Source precedence' (production semantics override the mock; TASK-418 functionality preserved; 23c88aba authoritative for visual geometry)
- [ ] #24 The spec document backlog/docs/doc-22 is updated if any specified behaviour is intentionally changed during implementation, so it stays an accurate description of the shipped UI
- [ ] #25 nix build .#web-ui passes; nix build .#server passes when server code changed; cargo fmt --all --check passes; git diff --check passes; SQLx offline metadata is regenerated when query shapes change; no println!/dbg!/eprintln! in production paths
- [ ] #26 The merge request records the reviewer verification results from spec section 9: dark and light screenshots for the compared states and confirmation that the bundle table Score column is served by a single bundle list request
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Phase 0 baseline: verify the merged TASK-418 API/model state in this worktree and run the existing compliance browser steps 29 through 29e if the focused web-ui harness is available; retain baseline failures as evidence before UI changes.
2. Phase 1 server contract: add `applicable_system_count` and optional `aggregate_score` to bundle summaries and compute them from the existing version-specific systems rollup path without per-bundle queries; add `policy_id` to coverage mappings and mirror both contracts in web-ui models. Add focused server tests for score/count selection semantics and coverage serialization; regenerate SQLx metadata only if query shapes require it.
3. Phase 2 shared policy/version foundations: extract the existing PolicyDrawer and its helper dependencies into a reusable policy component without changing PoliciesView behavior; make PoliciesView consume it; rename the existing `selected_export_version_id` signal to `selected_bundle_version_id` and route all version-sensitive systems, evidence, coverage, export, and lifecycle actions through it.
4. Phase 3 compliance surface: replace the 320px BundleCatalog layout with the full-width searchable/framework-filtered bundle table, add deterministic filters and empty state, build the right-hand overview drawer, revisions disclosure, relocated existing version/assignment controls, and dense systems drilldown while preserving loading/error/admin/stale-response behavior.
5. Phase 4 coverage: move coverage into the drawer, add exact-version generation-guarded loading, summary and grouped/filterable requirement rows, and lazy shared-policy-library drill-in with loading and unresolved-policy error states; restore the bundle drawer state on close.
6. Phase 5 STIG pause/resume: wrap the existing TASK-418 upload/native-review/review/reconcile/refine/final-review/committing workflow with versioned browser-local metadata persistence, a 2 MiB payload guard, no raw bytes, source-file SHA reattachment before commit, corrupt/oversize recovery, paused callout, discard, and non-backdrop pause close.
7. Phase 6 verification: add the specified CSS utilities, update compliance integration steps and coverage manifests, add/extend a focused NixOS web-ui check for table/drawer/coverage/paused-import states, run dark/light screenshots where the harness supports them, then run cargo fmt, git diff check, web-ui/server builds as applicable, and record objective results in TASK-422.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation started after MR !315 was merged into dev. Dedicated worktree: /home/mcamp/code/crystal-forge/TASK-422-compliance-view-redesign; branch TASK-422-compliance-view-redesign from dev e04c4e71. Main and dev integration worktrees were clean at preflight. Following spec §10 phases.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-15 18:51
---
Captured design-example reference screenshots added under docs/design/CrystalForge/screens/compliance-redesign/cb-1.png .. cb-7.png (dark/light bundle list, drawer overview, revisions, coverage view/filters, systems drilldown). These are quick visual references for implementer and reviewer; the spec §9.4 state-by-state walkthrough against the live design example remains authoritative. See doc-22 §9.3 for the file-to-state mapping.
---
<!-- COMMENTS:END -->
