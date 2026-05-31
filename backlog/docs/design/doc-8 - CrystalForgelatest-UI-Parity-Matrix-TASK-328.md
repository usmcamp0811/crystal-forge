---
id: doc-8
title: CrystalForgelatest UI Parity Matrix (TASK-328)
type: specification
created_date: '2026-05-31 16:08'
updated_date: '2026-05-31 16:12'
tags:
  - ui-parity
  - design-system
  - task-328
  - crystalforgelatest
---
# CrystalForgelatest UI Parity Matrix (TASK-328)

## What this is
This is the **implementation and QA contract** for parity work, not just a progress note.
It defines objective pass/fail rules so “pixel parity” can be verified deterministically.

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
- Button baseline: **8px vertical / 14px horizontal**, **13px text**
- Input baseline: **8px 12px**, **13px text**
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

## Tolerance Rules (for pixel checks)
- **Spacing / dimensions tolerance**: ±1px
- **Typography size tolerance**: 0px (must match)
- **Color tolerance**: exact token value match
- **Border/radius tolerance**: 0px (must match)
- **If conflict exists** between screenshot and `styles.css`, `styles.css` token values are authoritative unless explicitly overridden by component-local rules in design source.

## View-by-View Parity Matrix

### 1) Shell / Layout Frame
- Source: sidebar, brand block, nav sections, top controls/tweaks
- Owner: `packages/web-ui/src/main.rs`, layout components, `assets/app.css`
- Numeric criteria:
  - Sidebar full width: match reference width token usage
  - Brand mark: **28x28**, radius **7px**
  - Brand block min-height: **60px**
  - Nav section label: uppercase, letter-spacing consistent with design token
- Required assertions:
  - Sidebar mode toggle applies expected structure/state
  - Theme toggle sets root theme attribute and preserves geometry
- Required screenshots:
  - Dark: full sidebar + rail
  - Light: full sidebar + rail

### 2) Systems View
- Source: `app.jsx` Systems section
- Owner: `src/views/systems_list.rs`, system components, adapters
- Numeric criteria:
  - Header action buttons use baseline button sizing
  - Stat-strip cards maintain radius **16px** and border **1px**
  - Filter controls maintain input/button baseline spacing
  - Table and card density match reference row/card rhythm
- Required assertions:
  - Filter/search deterministically change shown count
  - Card/table toggle preserves active filter/query state
  - Modal open/close transitions deterministic
- Required screenshots:
  - loading, empty, error, populated-card, populated-table, filtered, panel-open, each modal

### 3) Flakes View
- Owner: `src/views/flakes*.rs`, flakes components
- Numeric criteria: chip dimensions, timeline spacing, table/list row density align to baseline tokens
- Assertions: filter + selection + queue/state controls
- Screenshots: loading/empty/error/populated + modal/tab variants

### 4) Builds View
- Owner: `src/views/builds.rs`, build components
- Numeric criteria: active/history layout spacing and queue table row density align with systems/evals standards
- Assertions: row selection, action controls, status transition rendering
- Screenshots: loading/empty/error/populated + active/history + detail selected

### 5) Evaluations View
- Owner: `src/views/evaluations.rs`, eval components
- Numeric criteria: queue/table density and detail/log pane spacing match global layout rhythm
- Assertions: selection, ordering controls, live/log states
- Screenshots: loading/empty/error/populated + queue/detail/log variants

### 6) CVEs View
- Owner: `src/views/cves.rs`, cve components
- Numeric criteria: severity chip paddings/typography and filter bar spacing match chip/input baselines
- Assertions: severity filters + grouping correctness
- Screenshots: loading/empty/error/populated + filtered states

### 7) Caches View
- Owner: `src/views/caches.rs`, caches components
- Numeric criteria: controls spacing + row/card density align to baseline tokens
- Assertions: credential-test flow transitions + filter/search behavior
- Screenshots: loading/empty/error/populated + modal/dialog states

### 8) Compliance View
- Owner: `src/views/compliance.rs` (+ new if missing)
- Numeric criteria: complete screen adheres to baseline typography/spacing/radius/border system
- Assertions: key compliance interactions and transitions
- Screenshots: loading/empty/error/populated + action/dialog states

### 9) Admin View
- Owner: `src/views/admin.rs`
- Numeric criteria: table/control/dialog geometry follows baseline tokens
- Assertions: admin workflow behavior and response-state rendering
- Screenshots: loading/empty/error/populated + dialogs + success/error toasts

### 10) User Profile View
- Owner: `src/views/profile.rs` (+ related auth/profile state)
- Numeric criteria: form field/button spacing and section rhythm match baseline
- Assertions: edit/save/cancel/validation transitions
- Screenshots: loading/empty/error/populated + editing/confirmation states

## Interaction Inventory (Must Be Asserted)
1. Search filtering
2. Multi-filter combinations + reset
3. Mode/tab toggles preserve state
4. Selection updates detail panes deterministically
5. Modal/dialog open/close/cancel/confirm
6. API error states render compliant error surfaces
7. Loading→populated and loading→error transitions
8. Empty-state CTA and copy parity
9. Theme dark/light parity without layout shift

## Screenshot Contract (Mandatory)
Per in-scope view, capture:
- `loading`
- `empty`
- `error`
- `populated-default`
- `populated-filtered`
- each modal/dialog/tab variant

Naming format:
- `<view>--<state>--<theme>.png`
- e.g. `systems--populated-table--dark.png`, `admin--dialog-user-edit--light.png`

## Scoring Rubric (Execution Tracking)
Use this to track “how close we are” objectively per view:
- **Visual parity (40%)**: spacing, typography, colors, radius, borders, shadows
- **Interaction parity (30%)**: controls, toggles, filters, modal/dialog behaviors
- **Data parity (20%)**: backend-driven values, no placeholder paths in production
- **Verification parity (10%)**: assertions + full screenshot coverage landed in `checks/web-ui`

Grade per view:
- **A (95-100)** release-ready parity
- **B (85-94)** minor deltas only
- **C (70-84)** meaningful drift remains
- **D (<70)** not acceptable for parity completion

## Dependency & Execution Order
1. TASK-328 (this matrix)
2. TASK-329 (global tokens/shell)
3. TASK-332 (API contract fields)
4. TASK-330 + TASK-331 + TASK-334 + TASK-335 + TASK-336 (view implementation)
5. TASK-333 (verification harness strict enforcement)

## Exit Criteria for TASK-328
- Matrix approved as authoritative implementation contract
- Every in-scope view has objective visual criteria + assertion contract + screenshot contract
- Every in-scope area mapped to owner implementation files
- Scoring rubric adopted for downstream parity reviews
