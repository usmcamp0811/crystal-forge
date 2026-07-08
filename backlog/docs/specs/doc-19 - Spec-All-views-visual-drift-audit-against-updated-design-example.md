---
id: doc-19
title: 'Spec: All-views visual drift audit against updated design example'
type: specification
created_date: '2026-07-08 07:25'
tags:
  - design-parity
  - audit
  - web-ui
---
# Spec: All-views visual drift audit vs updated design example

Implementation guide for the companion audit task. The design example at `docs/design/CrystalForge/` was updated on 2026-07-07; all previous parity work (m-19/m-20 push) targeted an OLDER snapshot. This task walks EVERY view, fixes SMALL drift inline, and files follow-up Backlog tasks for LARGE gaps. It must NOT turn into a feature task.

## 0. The rule that keeps this task focused

For every discrepancy found, classify it:
- **SMALL (fix inline in this task)**: presentational-only — text/labels, chip styles/colors, icons, spacing/typography/layout geometry, missing minor static elements, hover/selected states — confined to existing components, requiring NO new API data, NO new routes, NO new interaction flows, NO backend changes.
- **LARGE (file a follow-up Backlog task; do NOT implement)**: anything needing backend data or endpoints, new routes/views, new interaction flows, stateful widgets, or anything already owned by an open task (check first with task_search).

When unsure, classify as LARGE and file the follow-up. Every follow-up filed must use the Backlog Capture minimum (Problem + Desired Outcome) and reference the exact design file/lines.

## 1. How to compare

1. Render the design example: `docs/design/CrystalForge/serve.sh` serves the reference app in a browser (fixture-driven). Screens in `docs/design/CrystalForge/screens/` supplement it.
2. Render the implementation: `nix develop` → `run-ui-dev` (seeds fixture data, launches server + Dioxus dev server), or use the `checks/web-ui` screenshot output (`nix build .#checks.x86_64-linux.web-ui`) for stable states.
3. Compare view by view at desktop width. The design-parity harness under `checks/web-ui/design-parity/` (manifest + generate-design-targets.js + compare-design-parity.js) is the preferred objective mechanism — extend its manifest for views it does not yet cover.

## 2. The audit checklist (walk in this order)

For EACH view: compare page head (title/subtitle/actions), stat strips, filter bars, table+card geometry, chips/badges, empty/loading/error states, modals/trays/drawers reachable from the view. Record findings per view in the task notes (one bullet list per view: "matches / fixed inline: … / follow-ups filed: TASK-…").

| # | View | Design source | Implementation |
|---|------|---------------|----------------|
| 1 | Dashboard | `components/DashboardView.jsx` | `views/dashboard.rs` |
| 2 | Systems list | `components/Systems.jsx` | `views/systems_list.rs` |
| 3 | System detail (all tabs) | `components/SystemDetail.jsx`, `components/HardeningTab.jsx` | `views/system_detail.rs` |
| 4 | Flakes | `components/FlakesView.jsx` | `views/flakes_list.rs` |
| 5 | Environments | `components/EnvironmentsView.jsx` | `views/environments_list.rs` |
| 6 | Builds | `components/BuildsView.jsx` | `views/builds.rs` |
| 7 | Evaluations | `components/EvalsView.jsx`, `components/EvalDrawer.jsx` | `views/evaluations.rs` |
| 8 | Scanning | `components/ScanningView.jsx` | `views/scanning.rs` |
| 9 | CVEs | `components/CvesView.jsx` | `views/cves.rs` |
| 10 | Policies | `components/PoliciesView.jsx` | `views/policies.rs` |
| 11 | Compliance | `components/ComplianceView.jsx` | `views/compliance.rs` |
| 12 | Builders | `components/BuildersView.jsx` | `views/builders.rs` |
| 13 | Caches | `components/CachesView.jsx` | `views/caches.rs` |
| 14 | Admin/Server | `components/AdminView.jsx` | `views/admin.rs` |
| 15 | Shell chrome (topbar, sidebar, classification banners, setup coach) | `components/Shell.jsx`, `components/SetupCoach.jsx` | `components/layout/*` |
| 16 | Add/Edit system modals | `components/AddSystemModal.jsx`, `components/EditSystemModal.jsx` | `components/system/*` |
| 17 | Deploy gate | `components/DeployGate.jsx` | deploy surfaces |

## 3. Known LARGE gaps to file immediately (do not implement)

- **Profile view is entirely missing** (`components/ProfileView.jsx`; former TASK-335 was archived unmerged). File a fresh follow-up.
- Anything covered by open tasks — do NOT duplicate: TASK-384 (deployment banner/rollback/activity), the flakes-sync + sidebar-badges task (doc-18), TASK-353.1 (tags), TASK-353.2 (deploy gate API), TASK-357.1 (flake env span), TASK-357.2 (auto-sync persistence), TASK-348.1.1 (CVE triage persistence). Reference these instead of filing duplicates.

## 4. Verification

- After inline fixes: `cd packages/web-ui && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test` (from `nix develop`).
- `nix build .#checks.x86_64-linux.web-ui --no-link` — update existing screenshot baselines/assertions broken by intentional visual fixes; the check MUST pass.
- Extend `checks/web-ui/design-parity/manifest.json` coverage for at least the views where inline fixes were made.
- MR attaches before/after screenshots for each view with inline fixes (GitLab uploads, not committed).
- `nix flake check` only if check definitions were modified (state the tier decision in the MR).

## 5. Out of scope

- Implementing anything classified LARGE.
- Backend changes of any kind.
- Refactoring view code beyond what a presentational fix requires.
- Mobile/responsive redesign.
