---
id: TASK-334
title: >-
  Build full Compliance view (frontend + backend) faithful to CrystalForgelatest
  design
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-21 02:06'
labels:
  - design-parity
  - compliance
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ComplianceView.jsx
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - TASK-320
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-11 - CrystalForgelatest-design-source-index.md
modified_files:
  - packages/web-ui/src/views/compliance.rs
  - checks/web-ui
priority: high
ordinal: 1660
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge has no Compliance view that matches the CrystalForgelatest design reference (`components/ComplianceView.jsx`), and there is no backend that serves compliance bundles, per-system control rollups, or per-control evidence. Today this surface does not exist as a faithful, backend-backed feature.

## Goal
Deliver the **complete Compliance surface as a single vertical slice** — the `/compliance` UI faithful to the design reference, plus **whatever backend endpoints/data are required to drive it**. This task owns BOTH the frontend and the backend it needs. Where the design shows data Crystal Forge can genuinely produce, serve it from real backend APIs. Where it cannot yet (e.g. real OSCAL export generation), render the UI faithfully and clearly mark the gap with a tracked follow-up — never fabricate data in a way that misleads.

The bias is: **most faithful possible reproduction of the design example**, end to end, in objectively reviewable and testable chunks.

## Design Reference (authoritative)
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/ComplianceView.jsx`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/data-compliance.js`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/screens/sys-compliance-tab.png`

## Scope (the full design surface)
The reference defines these pieces — all are in scope:
1. **Page head**: "Compliance" title + subtitle, "Export evidence" (ghost) and "New bundle" (primary) buttons
2. **Bundle catalog** (left rail, 320px sticky card): per-bundle entry with name, layer chip, framework · version, "N controls · M envs"; selection highlight
3. **Bundle header card**: name, framework/version/layer chips, owner + last-reviewed, required-env badges, description
4. **Score strip**: Overall score % (color-thresholded 90/70), "X of Y hosts fully compliant", plus Pass/Warn/Fail/Waiver counts
5. **Systems matrix** (BundleDrilldown): segmented filter (All/Clean/Warning/Failing), host count, info callout, table (Host, Env, Score bar, Pass, Warn, Fail, Waiver, "View evidence")
6. **Controls evidence drawer** (per host): left control nav with status dots + j/k keyboard nav, right evidence card (status/severity chips, summary, status callouts, evidence items with artifacts, framework mapping)
7. **Evidence artifacts**: collapsible bodies for config (nix), systemd_unit, audit_log, cve_scan, policy_eval, banner screenshot, waiver doc
8. **Export evidence modal**: format picker (OSCAL/JSON/CSV/PDF/SARIF), scope seg, include-waivers / include-source toggles, computed filename
9. **New bundle modal**: name/version/framework/description, applies-to-environments chips, policy picker with search + multi-select, validation (name + ≥1 policy)
10. **States**: loading, empty (no bundles), error, populated

## Backend (created as needed, owned by this task)
Implement the minimum real backend to drive the above. Expected endpoints/DTOs (adjust to fit existing patterns in `packages/default`):
- `GET /api/compliance/bundles` → bundle list (id, name, framework, version, description, layer, owner, last_review, policy_ids, required_envs, control/env counts)
- `GET /api/compliance/bundles/:id/systems` → applicable systems with per-system rollup (applies, total, pass, warn, fail, waiver, score)
- `GET /api/compliance/bundles/:id/systems/:system_id/evidence` → per-control evidence (policy id/name, status, severity, summary, evidence items + artifacts, framework mapping)
- `POST /api/compliance/bundles` → create bundle (name, framework, version, description, required_envs, policy_ids)
- Export: wire "Export evidence" to a real endpoint if feasible within scope; otherwise render the modal faithfully and gate the download behind a clearly-marked follow-up (TASK-318 family).
- Reuse existing systems/environments/policies data and the deployment-policy evaluation that already exists; do NOT build a brand-new compliance evaluator engine here — derive rollups/evidence from data Crystal Forge already has, and track deeper evaluator work as follow-ups.

## Non-Goals
- A full first-class compliance evaluator/domain engine (the TASK-320 epic). Derive from existing policy/eval/CVE/system data instead; track gaps as follow-ups.
- Real auditor-grade OSCAL/OHDF/SARIF generation (track via TASK-318) — the export modal UI is in scope; production-grade file generation is not required to satisfy this task.
- The Compliance sidebar surface (TASK-344).
- Redesigning unrelated views.

## Architectural Constraints
- No business logic in UI views; the view renders DTOs returned by the API client.
- Rollup/evidence computation lives in backend `queries`/handler/service layers, not the UI.
- Status must preserve layered outcomes (pass/warn/fail/waiver) — no lossy single-badge collapse.
- New UI components live in `packages/web-ui/src/components/compliance/`; the route view in `packages/web-ui/src/views/compliance.rs`.
- Reuse shared shell/token primitives and existing card/table/stat-strip/seg/modal/drawer patterns (page-head, stat-strip, sys-table, fl-tray) established by TASK-329 and prior parity slices.
- Any unavoidable placeholder/mock (e.g. export file body) MUST be clearly commented in code AND tracked by a follow-up Backlog task; production read paths must not silently fabricate compliance results.
- New SQL must use migrations; new queries should prefer `sqlx::query_as`; keep `cargo sqlx prepare` metadata in sync.
- Respect existing RBAC patterns for who can view/create bundles and export.

## Verification Plan
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo check --manifest-path packages/default/Cargo.toml --all-targets`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml --lib` (compliance queries/handlers + rollup mapping unit tests)
- `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml --bin crystal-forge-ui` (compliance adapter/view unit tests)
- `cargo sqlx prepare --check` (db-only up in devshell; never against shared db)
- `nix build .#checks.x86_64-linux.web-ui` — must pass and capture Compliance screenshots
- Reviewer visual diff of each numbered scope item against `ComplianceView.jsx`

## Impact Areas
- `packages/web-ui/src/views/compliance.rs`
- `packages/web-ui/src/components/compliance/` (catalog, header, score strip, systems table, evidence drawer, evidence artifact, export modal, new-bundle modal)
- `packages/web-ui/src/api/models.rs`, `packages/web-ui/src/api/client.rs`
- `packages/web-ui/assets/app.css` (compliance-specific styles, evidence artifact styles)
- `packages/default/src/handlers/api/compliance.rs`
- `packages/default/src/queries/compliance.rs`
- `packages/default/src/api/models.rs`
- `packages/default/migrations/` (new migration(s) if needed)
- `checks/web-ui/tests/integration-test.js` (screenshots + assertions)

## Risk Level
Medium-High — large vertical slice spanning new backend endpoints, possible schema/migration work, and a substantial multi-component UI (catalog, table, drawer with keyboard nav, two modals). Mitigated by deriving from existing data, building in independently-reviewable chunks, and faithful adherence to a complete design reference.

## Implementation Plan (ordered, independently reviewable chunks)
1. Backend: bundle list + bundle/systems rollup endpoints + DTOs + unit tests (migration if needed; sqlx prepare).
2. Backend: per-control evidence endpoint + DTOs + unit tests.
3. Backend: create-bundle endpoint + validation + unit tests.
4. UI: page head + bundle catalog + bundle header + score strip wired to real APIs (loading/empty/error/populated).
5. UI: systems matrix table with segmented filter + counts.
6. UI: controls evidence drawer (control nav, j/k nav, evidence card, artifacts).
7. UI: export evidence modal + new bundle modal (policy picker).
8. web-ui check: screenshots + assertions for all states/interactions.

## Dependencies
- TASK-328 — CrystalForgelatest parity spec (Done)
- TASK-329 — Foundation shell/tokens/topbar/sidebar parity (Done)
- (Self-contained: this task creates the compliance backend it needs. Deeper evaluator/export work is tracked as follow-ups, not blockers.)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Page head matches the reference: 'Compliance' title + subtitle, ghost 'Export evidence' and primary 'New bundle' buttons
- [ ] #2 Bundle catalog left rail lists bundles (name, layer chip, framework · version, 'N controls · M envs') with selection highlight, driven by GET bundles API
- [ ] #3 Bundle header card shows name, framework/version/layer chips, owner + last reviewed, required-env badges, and description from real data
- [ ] #4 Score strip shows color-thresholded overall score %, 'X of Y hosts fully compliant', and Pass/Warn/Fail/Waiver counts computed by the backend
- [ ] #5 Systems matrix renders applicable systems with segmented filter (All/Clean/Warning/Failing), host count, and table columns (Host, Env, Score bar, Pass, Warn, Fail, Waiver, View evidence) from the bundle/systems rollup API
- [ ] #6 Controls evidence drawer opens per host with control nav (status dots), j/k + arrow keyboard navigation, and per-control evidence card (status/severity chips, summary, status callouts)
- [ ] #7 Evidence items render with collapsible artifact bodies (config/systemd/audit/cve/policy-eval/banner/waiver) and framework mapping line
- [ ] #8 Export evidence modal matches the reference: format picker (OSCAL/JSON/CSV/PDF/SARIF), scope segment, include-waivers/include-source toggles, computed filename; download wired to a real endpoint or gated behind a clearly-marked tracked follow-up
- [ ] #9 New bundle modal matches the reference: name/version/framework/description, applies-to-environments chips, searchable multi-select policy picker, and validation (name + at least one policy); create persists via POST bundles API
- [ ] #10 Loading, empty (no bundles), error, and populated states render per the reference with no silently-fabricated data in production read paths
- [ ] #11 All primary displayed values are sourced from authoritative backend compliance APIs created by this task (rollups/evidence derived from existing system/policy/eval/CVE data; any unavoidable placeholder is commented and tracked by a follow-up)
- [ ] #12 checks/web-ui captures screenshots and behavior assertions for the full Compliance surface: catalog selection, score strip, systems table + filter, evidence drawer + keyboard nav, export modal, new bundle modal, and empty/error states
- [ ] #13 A human reviewer can compare the implemented /compliance surface against ComplianceView.jsx and find no remaining material parity gaps for in-scope items
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refined (2026-06-21) into a single self-contained vertical slice covering BOTH the Compliance frontend and the backend endpoints it needs. Absorbs the compliance-relevant intent of TASK-332 (shared backend API contracts). No longer blocked on the TASK-320 compliance evaluator epic — rollups/evidence are derived from existing system/policy/eval/CVE data, with deeper evaluator/export work tracked as follow-ups.
<!-- SECTION:NOTES:END -->
