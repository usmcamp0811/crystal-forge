---
id: doc-14
title: Parity execution playbook (agent-proof)
type: guide
created_date: '2026-06-10 13:25'
tags:
  - playbook
  - parity
  - execution
  - agent-guide
---
# Parity execution playbook (agent-proof)

This is the single source of truth for marching to CrystalForgelatest parity quickly. It is written so any agent can pick the next task and finish it correctly with minimal human help.

Design source (authoritative): `/home/mcamp/code/crystal-forge/CrystalForgelatest`
Repo UI root: `packages/web-ui/src`
Checks harness: `checks/web-ui/tests/integration-test.js`

## How to pick the next task (do this first, every time)
1. Open the **Order of execution** list below. Pick the first surface that is not Done.
2. Open that surface's **umbrella task** in backlog. Pick its first unchecked child task in `To Do`.
3. If there are no child tasks yet, create them from the surface's **Definition of parity** checklist below (one child task per bullet group), status `Backlog`, then ask a human to promote the ones to run.
4. Follow **Standard task procedure**.

## Standard task procedure (same for every UI task)
1. Create worktree: `git worktree add -b TASK-ID-short-slug ../TASK-ID-short-slug dev`
2. Move task to `In Progress`, add LOCK note.
3. Open the matching design file in `CrystalForgelatest/components/` and the repo view file. Compare side by side.
4. Implement ONLY that task's scope. Keep data real (call the API client; no mock/fallback in production path).
5. Add/extend a `checks/web-ui` step (see **How to add a check step**). Name it `NN-surface-thing`.
6. Verify (see **Verification**).
7. Open MR with screenshot from the web-ui check. Move task to `Review`, paste MR link.
8. After merge: move to `Done`, remove worktree.

## Verification (default for UI tasks)
Run from repo dev shell:
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
Only run `nix flake check` if you touched Nix/modules/migrations/cross-crate contracts.

## How to add a check step (copy the existing pattern)
- Steps live in the `steps` array in `checks/web-ui/tests/integration-test.js`.
- Each step is `{ name, description, action: async (page) => { ... } }`.
- Mock APIs with `page.route(...)`, navigate with `page.goto(\`${baseUrl}/route\`)`, assert with `assertVisible(...)`.
- To run in CI-fast, also add the step `name` to `CI_FAST_STEP_NAMES`.
- Screenshots are captured per step automatically; reference the PNG name in the MR.

## Repo route + file map (ground truth)
| Surface | Route enum | View file | Design file |
|---|---|---|---|
| Dashboard | `DashboardView` `/` | `views/dashboard.rs` | `components/DashboardView.jsx` |
| Systems | `SystemsView` `/systems` | `views/systems.rs` + `systems_list*.rs` | `components/Systems.jsx`, `app.jsx` |
| System Detail | `SystemDetailView` `/systems/:id` | `views/system_detail.rs` | `components/SystemDetail.jsx` |
| Environments | `EnvironmentsView` `/environments` | `views/environments.rs` | `components/EnvironmentsView.jsx` |
| Flakes | `FlakesView` `/flakes` | `views/flakes.rs` (legacy `flakes_list.rs`) | `components/FlakesView.jsx` |
| Evaluations | `EvaluationsView` `/evaluations` | `views/evaluations.rs` | `components/EvalsView.jsx`, `EvalDrawer.jsx` |
| Builds | `BuildsView` `/builds` | `views/builds.rs` | `components/BuildsView.jsx` |
| Scanning | `ScanningView` `/scanning` | `views/scanning.rs` | `components/ScanningView.jsx` |
| CVEs | `CvesView` `/cves` | `views/cves.rs` (legacy `cves_old.rs`) | `components/CvesView.jsx` |
| Policies | `PoliciesView` `/deployment-policies` | `views/policies.rs` | `components/PoliciesView.jsx` |
| Builders | `BuildersView` `/builders` | `views/builders.rs` | `components/BuildersView.jsx` |
| Caches | `CachesView` `/caches` | `views/caches.rs` | `components/CachesView.jsx` |
| Admin | `AdminView` `/admin` | `views/admin.rs` | `components/AdminView.jsx` |
| Compliance | MISSING | MISSING (create `views/compliance.rs`) | `components/ComplianceView.jsx` |
| Profile | MISSING | MISSING (create `views/profile.rs`) | `components/ProfileView.jsx` |
| Shell/Sidebar/Topbar | n/a | `components/layout/*` | `components/Shell.jsx` |

## Known gaps found in this pass (real, verified)
- No Compliance view/route exists. Must be created.
- No Profile view/route exists. Must be created.
- Topbar is missing the notifications dropdown and classification banners present in `Shell.jsx`.
- Sidebar grouping differs from design groups (Fleet / Pipeline / Compliance / System).
- Dead/legacy files to remove or consolidate: `views/cves_old.rs`, `views/systems_mock.rs`, `views/systems_mock_data.rs`, `views/systems_mock_data_extra.rs`, `views/flakes_list.rs`, `views/environments_list.rs`, `views/policies_api.rs`, `views/register_api.rs`.

## Order of execution (top to bottom = fastest path)
Foundation first (these unblock everything; do them before deep per-view work):
1. `TASK-329` — tokens/shell/topbar/sidebar parity (foundation)
2. `TASK-328` — keep parity matrix (`doc-8`) updated as the per-view checklist source
3. `TASK-341` — remove dead/legacy files + fix duplicate metadata

Then surfaces in sidebar order (each = its umbrella task):
4. Dashboard — `TASK-342`
5. Systems — `TASK-330`
6. System Detail — `TASK-338`
7. Environments — `TASK-339`
8. Flakes — `TASK-343`
9. Evaluations — `TASK-345`
10. Builds — `TASK-347`
11. Scanning / CVEs — `TASK-348` (+ backend `TASK-327`)
12. Policies — `TASK-340`
13. Builders — `TASK-346` (+ bug `TASK-204`, `TASK-291`)
14. Caches — `TASK-349`
15. Admin — `TASK-336`
16. Compliance — `TASK-344` (needs backend `TASK-312..317`, then `TASK-319`, then `TASK-334`)
17. Profile — `TASK-335`

Final:
18. `TASK-333` — strict screenshot/assertion harness closure + rescore `doc-9`

## Definition of parity (use as the per-surface child-task checklist)
For EVERY surface, the surface is Done only when all are true:
- Layout/typography/spacing/radius/border match the design file within the tolerances in `doc-8`.
- All primary values come from the real API client (no mock/fallback in production path).
- Loading, empty, error, and populated states are styled per design.
- Each interactive control in the design file works (filters, search, view toggle, tabs, modals).
- A `checks/web-ui` step exists that screenshots the surface AND asserts at least one real interaction.
- `cargo fmt`, web-ui `cargo check` (wasm target), and `nix build .#checks.x86_64-linux.web-ui` pass.

## Surface-specific must-dos (the non-obvious parts)
- Shell/Topbar (`TASK-329`): add notifications dropdown + theme + tweaks parity; align sidebar groups to Fleet/Pipeline/Compliance/System; classification banner support if in scope.
- Systems (`TASK-330`): cards AND table modes, stat strip, filter bar, side panel, deploy/edit/add modals; remove `systems_mock*` from production path.
- CVEs (`TASK-348`): delete `cves_old.rs`; grouped + flat views; severity filter re-issues API query.
- Compliance (`TASK-344`): create route+view; bundles→controls→evidence→waiver; backed by `TASK-317` evaluator output.
- Profile (`TASK-335`): create route+view; edit/save/cancel/validation; backend account data.

## Related docs
- `doc-8` parity matrix (per-view measurable criteria)
- `doc-9` baseline scorecard (rescore after each merge)
- `doc-10` parity execution plan
- `doc-11` design source index
- `doc-12` compliance roadmap
- `doc-13` sidebar surface execution map
