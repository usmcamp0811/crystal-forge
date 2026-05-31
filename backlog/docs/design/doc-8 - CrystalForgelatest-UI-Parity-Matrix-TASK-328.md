---
id: doc-8
title: CrystalForgelatest UI Parity Matrix (TASK-328)
type: specification
created_date: '2026-05-31 16:08'
tags:
  - ui-parity
  - design-system
  - task-328
  - crystalforgelatest
---
# CrystalForgelatest UI Parity Matrix (TASK-328)

## Scope
Authoritative design source:
- `/home/mcamp/code/crystal-forge/CrystalForgelatest`
- key files: `app.jsx`, `styles.css`, `data-*.js`, `components/*`

Implementation target:
- `packages/web-ui/**`
- `checks/web-ui/**`

## Global Measurement Standards (Objective)
- Base font size: **14px**
- Default line-height: **1.45**
- Radius scale: **6 / 10 / 14 / 16 / 999px**
- Button sizing baseline: **8px vertical / 14px horizontal padding**, **13px text**
- Input sizing baseline: **8px 12px**, **13px text**
- Card border: **1px**
- Focus ring: **2px equivalent visible ring**
- Theme modes required: **dark + light**

## Global Token Parity Checklist
| Category | Reference Token(s) | Required Value/Behavior | Owner Files |
|---|---|---|---|
| Brand | `--cf-brand-purple`, `--cf-brand-purple-hover` | Exact hex parity per theme | `packages/web-ui/assets/app.css`, `packages/web-ui/src/theme.rs` |
| Semantic text | `--cf-text-primary/secondary/muted/disabled` | No value drift; all major typography mapped | `assets/app.css`, `theme.rs` |
| Surfaces | `--cf-page-bg`, `--cf-sidebar-bg`, `--cf-card-bg`, `--cf-card-border` | Exact dark/light parity | `assets/app.css` |
| Interaction | `--cf-focus-ring`, `--cf-input-*`, button variants | Focus/hover/active parity in both themes | `assets/app.css`, shared components |
| Status colors | healthy/warning/critical/offline/info chips | Exact semantic color + bg alpha parity | `assets/app.css`, chip components |
| Shadow model | `--shadow-card`, `--shadow-pop` | Exact values for each theme | `assets/app.css` |

## View-by-View Parity Matrix

### 1) Shell / Layout Frame
- Source: sidebar, brand block, nav sections, top controls/tweaks
- Owner: `packages/web-ui/src/main.rs`, layout components, `assets/app.css`
- Criteria:
  - Sidebar width and paddings match reference spacing rhythm
  - Brand mark size/radius/gradient matches visual spec
  - Section label typography/case/letterspacing match
  - Rail/full mode spacing and icon alignment match
- Required web-ui assertions:
  - Sidebar mode toggle applies expected structural class/state
  - Theme toggle updates root theme attribute and preserves layout geometry
- Required screenshots:
  - Dark: full sidebar + rail sidebar
  - Light: full sidebar + rail sidebar

### 2) Systems View
- Source: `app.jsx` Systems view section
- Owner: `src/views/systems_list.rs`, system components, adapters
- Criteria:
  - Header/title/subtitle text and action button geometry match
  - Stat strip card count, spacing, accents, and typography match
  - Filter bar controls, segmented view toggle, result count placement match
  - Card/table modes match row/card density and column rhythm
  - Panel/modal visual geometry and behavior match
- Required web-ui assertions:
  - Filter/search affect shown count deterministically
  - Card/table toggle retains selected filters
  - Modal open/close state transitions deterministic
- Required screenshots:
  - loading, empty, error, populated(card), populated(table), filtered, panel open, each modal

### 3) Flakes View
- Owner: `src/views/flakes*.rs`, flakes components
- Criteria: timeline/list density, chips/statuses, filters/tabs, detail pane alignment
- Assertions: filter + selection + queue/state controls
- Screenshots: loading/empty/error/populated + modal/tab variants

### 4) Builds View
- Owner: `src/views/builds.rs`, build components
- Criteria: active/history segmentation, queue table density, detail panel hierarchy
- Assertions: row select, action controls, status transitions rendering
- Screenshots: loading/empty/error/populated + active/history + detail selected

### 5) Evaluations View
- Owner: `src/views/evaluations.rs`, eval components
- Criteria: queue presentation parity, logs/detail layout parity, control density parity
- Assertions: selection, ordering controls, live/log state behavior
- Screenshots: loading/empty/error/populated + queue/detail/log variants

### 6) CVEs View
- Owner: `src/views/cves.rs`, cve components
- Criteria: severity chip semantics, grouping rows, filter/search alignment
- Assertions: severity filters + grouping correctness in UI state
- Screenshots: loading/empty/error/populated + filtered states

### 7) Caches View
- Owner: `src/views/caches.rs`, caches components
- Criteria: card/table spacing, controls alignment, status chip parity
- Assertions: test-credential flow UI state transitions, filter/search behavior
- Screenshots: loading/empty/error/populated + modal/dialog states

### 8) Compliance View
- Owner: `src/views/compliance.rs` (+ new if missing)
- Criteria: complete screen composition parity, controls/states parity
- Assertions: critical compliance interactions and state transitions
- Screenshots: loading/empty/error/populated + action/dialog states

### 9) Admin View
- Owner: `src/views/admin.rs`
- Criteria: exact table/control/dialog parity and hierarchy
- Assertions: admin workflows and response-state rendering
- Screenshots: loading/empty/error/populated + dialogs + success/error toasts

### 10) User Profile View
- Owner: `src/views/profile.rs` (+ related auth/profile state)
- Criteria: profile sections, forms, feedback/status styling parity
- Assertions: edit/save/cancel/validation state transitions
- Screenshots: loading/empty/error/populated + editing/confirmation states

## Interaction Inventory (Must Be Asserted)
1. Search input filtering (systems, flakes, builds, evals, cves, caches)
2. Multi-filter combinations and reset behavior
3. Card/table or tab mode toggles preserve filter/query state
4. Row/card selection updates detail pane deterministically
5. Modal open/close, cancel, and confirm states
6. API error states render design-compliant error surfaces
7. Loading → populated transition and loading → error transition
8. Empty-state messaging and CTA visual parity
9. Theme switch dark/light visual parity and no layout shift

## Screenshot Contract (Mandatory Coverage)
Per in-scope view, capture:
- `loading`
- `empty`
- `error`
- `populated-default`
- `populated-filtered`
- each modal/dialog/tab variant relevant to that view

Naming format:
- `<view>--<state>--<theme>.png`
- examples: `systems--populated-table--dark.png`, `admin--dialog-user-edit--light.png`

## Dependency & Execution Order
1. TASK-328 (this spec)
2. TASK-329 (global tokens/shell)
3. TASK-332 (API contract fields)
4. TASK-330 + TASK-331 + TASK-334 + TASK-335 + TASK-336 (view implementation)
5. TASK-333 (verification harness completion and strict enforcement)

## Exit Criteria for TASK-328
- Matrix completed and approved as authoritative implementation contract
- Every in-scope view has objective visual criteria, assertion contract, and screenshot contract
- Every in-scope area mapped to owner implementation files
