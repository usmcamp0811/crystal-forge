---
id: doc-8
title: CrystalForgelatest UI Parity Matrix (TASK-328)
type: specification
created_date: '2026-05-31 16:08'
updated_date: '2026-06-10 19:36'
tags:
  - ui-parity
  - design-system
  - task-328
  - crystalforgelatest
---
# CrystalForgelatest UI Parity Matrix (TASK-328)

## What this is
This document is the authoritative implementation and QA contract for CrystalForgelatest parity work.
Every downstream UI task must use this matrix as its pass/fail source of truth.

## Design and implementation scope
Authoritative design source:
- `/home/mcamp/code/crystal-forge/CrystalForgelatest`
- Key files: `app.jsx`, `styles.css`, `components/*.jsx`

Implementation target:
- `packages/web-ui/**`
- `checks/web-ui/**`

## In-scope primary surfaces
The parity program covers every primary surface currently represented in CrystalForgelatest:
- Shell / layout frame
- Dashboard
- Systems
- System detail
- Environments
- Flakes
- Evaluations
- Builds
- Scanning
- CVEs
- Policies
- Builders
- Caches
- Admin / Server management
- Compliance
- Profile

## Global measurement standards
These values are objective and apply unless a design component explicitly overrides them:
- Base font size: **14px**
- Default line-height: **1.45**
- Radius scale: **6 / 10 / 14 / 16 / 999px**
- Button baseline: **8px vertical / 14px horizontal**, **13px text**
- Input baseline: **8px 12px**, **13px text**
- Card border: **1px**
- Focus ring: **2px visible ring**
- Theme coverage required: **dark and light**
- Screenshot naming: `<view>--<state>--<theme>.png`

## Tolerance rules
- Spacing and dimensions: **±1px**
- Typography sizes: **0px tolerance**
- Border widths: **0px tolerance**
- Radius values: **0px tolerance**
- Color values: **exact token match**
- If screenshots disagree with global CSS values, `CrystalForgelatest/styles.css` is authoritative unless the relevant component defines a local override.

## Global token and primitive contract
| Category | Reference token or primitive | Required parity rule | Owner files |
|---|---|---|---|
| Brand | `--cf-brand-purple`, `--cf-brand-purple-hover` | Exact dark/light hex parity | `packages/web-ui/assets/app.css`, `packages/web-ui/src/theme.rs` |
| Semantic text | `--cf-text-primary`, `--cf-text-secondary`, `--cf-text-muted`, `--cf-text-disabled` | No value drift across shell and view typography | `packages/web-ui/assets/app.css`, `packages/web-ui/src/theme.rs` |
| Surfaces | `--cf-page-bg`, `--cf-sidebar-bg`, `--cf-topbar-bg`, `--cf-card-bg`, `--cf-card-border` | Exact theme parity | `packages/web-ui/assets/app.css` |
| Interaction | focus ring, button variants, input states, toggles | Hover, focus, active, disabled states match design values in both themes | `packages/web-ui/assets/app.css`, shared components under `packages/web-ui/src/components/` |
| Status colors | healthy, warning, critical, offline, info chips and badges | Exact semantic fg/bg treatment and badge contrast | `packages/web-ui/assets/app.css`, badge and chip components |
| Shadows | `--shadow-card`, `--shadow-pop` | Exact shadow recipe by theme | `packages/web-ui/assets/app.css` |
| Shared primitives | `.btn`, `.input`, `.card`, `.chip`, `.env-badge` | One canonical definition each, no conflicting duplicates | `packages/web-ui/assets/app.css`, layout/shared component files |

## Surface matrix
| Surface | Design source | Route or entry | Owner files | Objective visual criteria | Mandatory assertions | Required screenshots |
|---|---|---|---|---|---|---|
| Shell / layout frame | `components/Shell.jsx`, `styles.css` | Global app chrome | `packages/web-ui/src/components/layout/app_shell.rs`, `packages/web-ui/src/components/layout/sidebar.rs`, `packages/web-ui/src/components/layout/topbar.rs`, `packages/web-ui/src/main.rs`, `packages/web-ui/assets/app.css` | Sidebar full mode and rail mode match reference geometry; brand mark is **28x28** with **7px** radius; brand block minimum height is **60px**; topbar search and action controls use 13px text and baseline input or button spacing; nav section labels remain uppercase with design letter spacing | Sidebar mode toggle changes structure without layout corruption; theme toggle changes root theme state without geometry drift; topbar notifications panel opens and closes deterministically; active nav item changes with route | `shell--full--dark`, `shell--rail--dark`, `shell--full--light`, `shell--rail--light`, `topbar--notifications-open--dark`, `topbar--notifications-open--light` |
| Dashboard | `components/DashboardView.jsx` | `DashboardView` `/` | `packages/web-ui/src/views/dashboard.rs`, `packages/web-ui/src/dashboard/*.rs`, related dashboard components | Header, stat cards, cards and tables use global typography, radius and border tokens; card spacing follows 16px card rhythm; KPI and summary blocks align to shell content gutters | Navigation CTA changes route correctly; any filter, tab or summary action present in the view updates the shown state deterministically | `dashboard--loading--dark`, `dashboard--empty--dark`, `dashboard--error--dark`, `dashboard--populated-default--dark`, same set for light |
| Systems | `components/Systems.jsx`, `app.jsx` | `SystemsView` `/systems` | `packages/web-ui/src/views/systems.rs`, `packages/web-ui/src/views/systems_list.rs`, `packages/web-ui/src/views/systems_list_helpers.rs`, `packages/web-ui/src/components/system/*.rs`, `packages/web-ui/src/components/tables/systems_table.rs`, `packages/web-ui/src/components/systems_stat_strip.rs` | Header action buttons use button baseline sizing; stat-strip cards use **16px** radius and **1px** border; filter bar controls use input baseline spacing; cards and table rows match design density for comfortable and compact modes | Search changes shown count; multi-filter combinations work and can be reset; card or table toggle preserves current filters; side panel opens from selection; deploy, edit and add-system modals open and close deterministically | `systems--loading--dark`, `systems--empty--dark`, `systems--error--dark`, `systems--populated-cards--dark`, `systems--populated-table--dark`, `systems--filtered--dark`, `systems--panel-open--dark`, `systems--deploy-modal--dark`, `systems--edit-modal--dark`, `systems--add-modal--dark`, same set for light |
| System detail | `components/SystemDetail.jsx` | `SystemDetailView` `/systems/:id` | `packages/web-ui/src/views/system_detail.rs`, `packages/web-ui/src/components/system/tabs/*.rs`, system detail support components | Header hierarchy, tab strip, summary cards and section rhythm align to design spacing; status badges and evidence rows use shared chip and card tokens; table or log sections maintain global typography | Back navigation returns to systems view; tab changes preserve selected system; tag click applies system filter; deploy and edit actions open their dialogs or routes correctly | `system-detail--loading--dark`, `system-detail--error--dark`, `system-detail--populated-overview--dark`, `system-detail--logs-tab--dark`, `system-detail--related-state--dark`, same set for light |
| Environments | `components/EnvironmentsView.jsx` | `EnvironmentsView` `/environments` | `packages/web-ui/src/views/environments.rs`, `packages/web-ui/src/views/environments_list.rs`, environment adapters and related components | Cards or rows use global card tokens; environment badges use canonical env badge treatment; section spacing matches systems and dashboard gutters | Search or environment selection changes visible rows deterministically; any mode toggle preserves active state; empty and populated states render different view-state treatments | `environments--loading--dark`, `environments--empty--dark`, `environments--error--dark`, `environments--populated-default--dark`, `environments--filtered--dark`, same set for light |
| Flakes | `components/FlakesView.jsx` | `FlakesView` `/flakes` | `packages/web-ui/src/views/flakes.rs`, `packages/web-ui/src/views/flakes_list.rs`, `packages/web-ui/src/components/flake/*.rs` | Chip sizes, row density, timeline spacing and section spacing align to global tokens; cards and timeline panels use exact border and radius tokens | Filtering changes visible results; selection updates detail state; any tabs, queues or controls present in the view respond deterministically | `flakes--loading--dark`, `flakes--empty--dark`, `flakes--error--dark`, `flakes--populated-default--dark`, `flakes--filtered--dark`, `flakes--detail-or-modal--dark`, same set for light |
| Evaluations | `components/EvalsView.jsx`, `components/EvalDrawer.jsx` | `EvaluationsView` `/evaluations` | `packages/web-ui/src/views/evaluations.rs`, `packages/web-ui/src/components/eval_log_modal.rs` | Queue and history density match systems table rhythm; drawer or log pane spacing follows card and shell gutters; status chips use shared status color rules | Selection opens drawer or detail; ordering and filter controls change visible results; live or log states can be asserted | `evaluations--loading--dark`, `evaluations--empty--dark`, `evaluations--error--dark`, `evaluations--populated-default--dark`, `evaluations--detail-open--dark`, `evaluations--log-state--dark`, same set for light |
| Builds | `components/BuildsView.jsx` | `BuildsView` `/builds` | `packages/web-ui/src/views/builds.rs` | Active and history layouts share the same row density and card rhythm as evaluations; action bars and summary strips use shell spacing rules | Row selection changes detail state; filters or status controls update visible rows; any action controls render deterministic pending, success or failure states | `builds--loading--dark`, `builds--empty--dark`, `builds--error--dark`, `builds--populated-default--dark`, `builds--history-or-detail--dark`, same set for light |
| Scanning | `components/ScanningView.jsx` | `ScanningView` `/scanning` | `packages/web-ui/src/views/scanning.rs` | Scanning tables, filters and bulk-action surfaces follow chip, table and card baselines; severity or result group styling uses exact semantic colors | Search or filter changes result counts; selection enables and disables bulk actions deterministically; grouped and ungrouped states, if present, are asserted | `scanning--loading--dark`, `scanning--empty--dark`, `scanning--error--dark`, `scanning--populated-default--dark`, `scanning--filtered--dark`, `scanning--bulk-selected--dark`, same set for light |
| CVEs | `components/CvesView.jsx` | `CvesView` `/cves` | `packages/web-ui/src/views/cves.rs`, legacy cleanup tracked separately for `packages/web-ui/src/views/cves_old.rs` | Severity chip padding and typography match chip baseline; filter bar spacing matches systems and scanning; grouped sections and table cards use canonical card tokens | Severity filter changes query and result set; grouping toggle or grouped navigation updates visible structure; system navigation from a CVE row works | `cves--loading--dark`, `cves--empty--dark`, `cves--error--dark`, `cves--populated-default--dark`, `cves--filtered--dark`, `cves--grouped-or-flat--dark`, same set for light |
| Policies | `components/PoliciesView.jsx` | `PoliciesView` `/deployment-policies` | `packages/web-ui/src/views/policies.rs`, `packages/web-ui/src/views/policies_api.rs`, policy components | Table, form and modal geometry follow global card, input and button tokens; policy status indicators use shared semantic styling | Search or filters change visible policies; any create, edit, enable or disable flow changes UI state deterministically; navigation to related system works where supported | `policies--loading--dark`, `policies--empty--dark`, `policies--error--dark`, `policies--populated-default--dark`, `policies--dialog-or-editor--dark`, same set for light |
| Builders | `components/BuildersView.jsx` | `BuildersView` `/builders` | `packages/web-ui/src/views/builders.rs` | Cards, tables and utilization indicators follow token baselines; row density and filter spacing align with systems and builds | Search or filter changes builder set; any toggle between cards and table preserves state; health or status transitions render deterministically | `builders--loading--dark`, `builders--empty--dark`, `builders--error--dark`, `builders--populated-default--dark`, `builders--filtered-or-detail--dark`, same set for light |
| Caches | `components/CachesView.jsx` | `CachesView` `/caches` | `packages/web-ui/src/views/caches.rs` | Controls spacing, cards, rows and modal/dialog surfaces align with shared tokens; credential or endpoint status styling uses exact semantic colors | Search or filter changes visible caches; credential test or verification flow shows deterministic in-progress and result states; dialogs open and close correctly | `caches--loading--dark`, `caches--empty--dark`, `caches--error--dark`, `caches--populated-default--dark`, `caches--dialog-or-test-state--dark`, same set for light |
| Admin / Server management | `components/AdminView.jsx` | `AdminView` `/admin` | `packages/web-ui/src/views/admin.rs` | Table controls, dialogs and settings sections follow baseline typography, border and spacing values; classification UI, where implemented, respects shell spacing contract | Admin workflow controls change visible state deterministically; success and error responses render distinct surfaces; navigation actions target the intended route | `admin--loading--dark`, `admin--empty--dark`, `admin--error--dark`, `admin--populated-default--dark`, `admin--dialog-or-toast--dark`, same set for light |
| Compliance | `components/ComplianceView.jsx` | planned `ComplianceView` | `packages/web-ui/src/views/compliance.rs` when created, plus related components | Entire screen must follow baseline typography, spacing, border and radius system; bundle, control and evidence sections use card rhythm consistent with systems detail | Bundle selection updates visible controls; evidence or waiver interactions render deterministic state transitions; navigation to related system works | `compliance--loading--dark`, `compliance--empty--dark`, `compliance--error--dark`, `compliance--populated-default--dark`, `compliance--detail-or-dialog--dark`, same set for light |
| Profile | `components/ProfileView.jsx` | planned `ProfileView` | `packages/web-ui/src/views/profile.rs` when created, related auth or state files as needed | Form field spacing, section rhythm and button treatment match baseline inputs and cards; profile summary blocks use shell spacing | Edit, save, cancel and validation states all render deterministically; preference toggles update visible state without layout shift | `profile--loading--dark`, `profile--empty--dark`, `profile--error--dark`, `profile--populated-default--dark`, `profile--editing--dark`, `profile--confirmation--dark`, same set for light |

## Interaction inventory by surface
Each relevant surface must assert at least one real interaction from this inventory, and complex surfaces should cover multiple items:
- Shell: theme toggle, sidebar rail/full toggle, notifications panel open/close, active navigation state
- Dashboard: route-changing CTA or summary drill-down
- Systems: search, multi-filter combinations, view toggle, side panel, deploy or edit modal, add-system modal
- System detail: back navigation, tab switching, tag filtering, deploy or edit action
- Environments: search or filter, selection, mode toggle if present
- Flakes: search or filter, selection, tab or detail change
- Evaluations: selection, filter or order change, drawer or log open
- Builds: selection, filter or status control, detail or history switch
- Scanning: filter, grouped state switch, multi-select or bulk action enablement
- CVEs: severity filter, grouped or flat switch, navigation to impacted system
- Policies: filter, edit or enablement action, related-system navigation
- Builders: search or filter, mode toggle, status change rendering
- Caches: filter, credential test flow, dialog open or close
- Admin: action button, dialog, success state, error state
- Compliance: bundle selection, evidence or waiver interaction, navigation to system
- Profile: edit, save, cancel, validation, preferences change

## Screenshot contract
For every in-scope surface, capture both dark and light theme screenshots for:
- `loading`
- `empty`
- `error`
- `populated-default`
- `populated-filtered` when the surface supports filtering or searching
- Each modal, drawer, side panel, tab set, dialog, detail-open state, or alternate mode that materially changes layout

Additional shell-specific screenshots are mandatory:
- Full sidebar
- Rail sidebar
- Notifications panel open

## Assertion contract
A surface is not parity-complete if it only has screenshots.
Each surface must also assert:
1. At least one interaction that changes the rendered state.
2. At least one state transition from loading to either populated or error.
3. Correct route, panel, modal, drawer, or tab activation where that surface supports navigation.
4. Dark and light theme rendering without layout breakage.

## Route and owner map
| Surface | Route enum or route | Primary owner files |
|---|---|---|
| Dashboard | `DashboardView` `/` | `packages/web-ui/src/views/dashboard.rs` |
| Systems | `SystemsView` `/systems` | `packages/web-ui/src/views/systems.rs`, `packages/web-ui/src/views/systems_list.rs` |
| System detail | `SystemDetailView` `/systems/:id` | `packages/web-ui/src/views/system_detail.rs` |
| Environments | `EnvironmentsView` `/environments` | `packages/web-ui/src/views/environments.rs` |
| Flakes | `FlakesView` `/flakes` | `packages/web-ui/src/views/flakes.rs` |
| Evaluations | `EvaluationsView` `/evaluations` | `packages/web-ui/src/views/evaluations.rs` |
| Builds | `BuildsView` `/builds` | `packages/web-ui/src/views/builds.rs` |
| Scanning | `ScanningView` `/scanning` | `packages/web-ui/src/views/scanning.rs` |
| CVEs | `CvesView` `/cves` | `packages/web-ui/src/views/cves.rs` |
| Policies | `PoliciesView` `/deployment-policies` | `packages/web-ui/src/views/policies.rs` |
| Builders | `BuildersView` `/builders` | `packages/web-ui/src/views/builders.rs` |
| Caches | `CachesView` `/caches` | `packages/web-ui/src/views/caches.rs` |
| Admin | `AdminView` `/admin` | `packages/web-ui/src/views/admin.rs` |
| Compliance | not yet routed in repo | `packages/web-ui/src/views/compliance.rs` to be created |
| Profile | not yet routed in repo | `packages/web-ui/src/views/profile.rs` to be created |
| Shell | global | `packages/web-ui/src/components/layout/*.rs`, `packages/web-ui/src/main.rs` |

## Scoring rubric
Use this rubric when rescoring parity after each merge:
- Visual parity: **40%**
- Interaction parity: **30%**
- Data parity: **20%**
- Verification parity: **10%**

Grades:
- **A (95-100)** release-ready parity
- **B (85-94)** minor deltas only
- **C (70-84)** meaningful drift remains
- **D (<70)** not acceptable for parity completion

## Exit criteria for TASK-328
TASK-328 is complete only when all are true:
- Every primary CrystalForgelatest surface listed above exists in this document.
- Every surface row names design source, owner files, objective criteria, assertions, and screenshot targets.
- The interaction inventory covers filter, search, toggle, modal, table, card, and detail flows where relevant.
- The screenshot contract explicitly requires loading, empty, error, and populated states.
- The matrix remains the single downstream contract for parity work such as TASK-329 and later surface tasks.
