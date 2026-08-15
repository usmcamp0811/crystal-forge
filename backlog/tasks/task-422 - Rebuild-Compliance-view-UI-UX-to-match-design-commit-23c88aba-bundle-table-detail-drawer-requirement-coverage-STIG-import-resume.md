---
id: TASK-422
title: >-
  Rebuild Compliance view UI/UX to match design commit 23c88aba (bundle table,
  detail drawer, requirement coverage, STIG import resume)
status: Backlog
assignee: []
created_date: '2026-08-15 17:41'
updated_date: '2026-08-15 18:51'
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
- [ ] #1 Bundle list renders as a full-width card table (5 columns: Bundle, Framework, Version, Score, action) replacing the 320px BundleCatalog rail, matching spec section 4 including colgroup widths, score dot, control/revision counts, framework chip, version + publication-state chip, score % + system count, and the icon-only row action
- [ ] #2 Framework filter chips (All + one per distinct framework, each with a count, ordered by descending count) and a 'Search bundles…' input filter the table together with AND semantics; the clear button and '{matching} of {total}' count appear only while the query is non-empty
- [ ] #3 When filters match no bundle the table is replaced by the q-empty state reading 'No bundles match “{query}”.' and no bundle rows are rendered
- [ ] #4 Clicking a bundle row or its action opens a right-hand drawer (fl-tray backdrop + aside, width min(900px,96vw)) in overview view; clicking the action does not double-fire the row handler; backdrop click and the header close button close the drawer
- [ ] #5 Drawer overview renders, in order: card-less BundleHeader, a 5-column stat-strip-flush strip (Overall score, Pass, Warn, Fail, Waiver) using the section 4.4 colour thresholds, a collapsed-by-default Revisions disclosure shown only when the bundle has more than one version, the requirement-coverage summary row, and the Systems drilldown
- [ ] #6 Selecting a revision in the drawer re-scopes the stat strip, requirement coverage, and Systems table to that bundle version
- [ ] #7 The requirement-coverage summary row shows '{framework} · {total} requirements · derived from mapped policies, not policy tags' with full/partial/unmapped chips whose counts come from GET /api/v1/compliance/bundle-versions/{id}/requirement-coverage, and renders the 'No requirement catalog modeled for {framework} yet.' block when the report has zero requirements
- [ ] #8 Clicking the requirement-coverage row switches the drawer to the coverage view with a back arrow header titled 'Requirement coverage' plus the bundle name; the back arrow returns to overview
- [ ] #9 Coverage view renders the All/Full/Partial/Unmapped segmented filter with live counts, a 'Filter requirements…' search matching external ID and title, rows grouped by top-level requirement ancestor with the group heading suppressed when the group is a single root row, per-row Fully covered / Partially covered / Unmapped chips, and the centred 'No requirements match.' state when filters exclude everything
- [ ] #10 Requirement rows with mappings render an 'Enforced by' label followed by one cf-policy-link chip per mapped policy; clicking a chip opens the policy drawer as an overlay over the Compliance view, hides the bundle drawer, and closing it restores the bundle drawer in coverage view with filters intact
- [ ] #11 The Systems drilldown inside the drawer uses sys-table compact sys-table-dense with the spec colgroup, right-aligned mono Pass/Warn/Fail/Waiver cells with the specified conditional colours, a 40px score bar, and an icon-only 'View evidence' action that opens the existing evidence drawer without double-firing the row click
- [ ] #12 GET /api/v1/compliance/bundles returns applicable_system_count and optional aggregate_score computed set-based for the current published (else current draft) version, consistent with ComplianceRollupTotals.overall_score for that version; the bundle table issues no per-row request to populate the Score column
- [ ] #13 BundleCoverageMapping includes policy_id on both server and web-ui models, populated from the existing coverage query, and the Enforced-by chips resolve the policy through it
- [ ] #14 STIG import wizard persists progress under the localStorage key 'cf-stig-import-draft', is size-guarded, resumes at the stored step, exposes 'Discard draft' and a pause-titled close button on the review/reconcile/refine steps, no longer closes on backdrop click, and clears the draft on completion or on returning to an empty upload step
- [ ] #15 When a paused draft exists and the modal is closed, the Compliance page renders the sd-callout-warn paused-import callout with Discard and Resume actions and the Import/Export menu item label changes to 'Resume STIG import…'
- [ ] #16 When a persisted draft cannot restore its parsed benchmark payload, resume reopens at the upload step preserving bundle name and environments with an explicit re-select-file prompt; corrupt draft data is treated as no draft and never panics; no credentials, tokens or authorization material are persisted
- [ ] #17 The CSS additions in spec section 6 (cf-policy-link, cf-fw-chip, stat-strip-flush, sys-table-dense, sys-table-fixed, sd-callout-warn, anchor colours) are added to packages/web-ui/assets/app.css and render correctly in both dark and light themes
- [ ] #18 Pre-existing Compliance functionality is preserved and relocated per spec section 5.2: XCCDF version selection, bundle version trust/publish/create-draft actions with admin gating, assignment panel, export modal, new/edit/delete bundle modals, and the evidence drawer all still work
- [ ] #19 Loading, empty, error, admin-authorization and stale-response behaviour is preserved for bundle list, systems rollup, evidence and coverage fetches, including generation-guarded responses so a slow reply cannot overwrite a newer selection
- [ ] #20 Existing compliance steps in checks/web-ui/tests/integration-test.js (29, 29a, 29b, 29c, 29d, 29e) are updated to assert the new layout, and a focused NixOS check following the checks/web-ui-reconciliation pattern proves the new bundle table, drawer, coverage view and paused-import callout with passing results.json and dark + light screenshots
- [ ] #21 checks/web-ui/coverage-manifest.json is updated for any added or renamed step, and checks/web-ui/design-parity/manifest.json still resolves the compliance view to /compliance and the refreshed ComplianceView.jsx
- [ ] #22 The spec document backlog/docs/doc-22 is updated if any specified behaviour is intentionally changed during implementation, so it stays an accurate description of the shipped UI
- [ ] #23 nix build .#web-ui passes; nix build .#server passes when server code changed; cargo fmt --all --check passes; git diff --check passes; SQLx offline metadata is regenerated when query shapes change; no println!/dbg!/eprintln! in production paths
- [ ] #24 The merge request records the reviewer verification results from spec section 9: dark and light screenshots for the compared states and confirmation that the bundle table Score column is served by a single bundle list request
<!-- AC:END -->
