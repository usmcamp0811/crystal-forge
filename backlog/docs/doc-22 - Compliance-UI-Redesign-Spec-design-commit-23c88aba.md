---
id: doc-22
title: Compliance UI Redesign Spec (design commit 23c88aba)
type: specification
created_date: '2026-08-15 17:40'
tags:
  - compliance
  - web-ui
  - design
---
# Compliance UI Redesign Spec — design commit `23c88aba`

Authoritative visual/interaction specification for rebuilding the Crystal Forge production
Compliance view (Dioxus `web-ui`) to match the refreshed design example.

**Design source of truth (read these files at this commit):**

```text
23c88aba  MC ◯ refinement of the compliance ui/ux
```

| File | What it defines |
| --- | --- |
| `docs/design/CrystalForge/components/ComplianceView.jsx` | Bundle list table, bundle detail drawer, requirement coverage view, systems drilldown |
| `docs/design/CrystalForge/components/ImportStigModal.jsx` | STIG import pause/resume draft behaviour |
| `docs/design/CrystalForge/components/PoliciesView.jsx` | `PolicyDrawer` (reused as an overlay from Compliance), back-navigation affordance |
| `docs/design/CrystalForge/data-compliance.js` | `bundleQuickStats()` — per-bundle aggregate score + system count |
| `docs/design/CrystalForge/styles.css` | New utility classes (see §6) |
| `docs/design/CrystalForge/app.jsx` | View-level wiring of `onOpenPolicy` / back-to-compliance |

Read the file at `23c88aba`, not just the diff. Line references below are for the file at that commit.

### Source precedence

When sources disagree, resolve in this order (from highest to lowest):

1. **Production data/behaviour semantics declared in this spec** (and TASK-418's model) override
   the mock. The mock's data shapes are illustrative, not contractual.
2. **Existing TASK-418 functionality must be preserved** — the normalized
   requirement/mapping model, the reconciliation STIG state machine, and their request/commit
   behaviour are owned by TASK-418; this task may not regress them (see §8.3.1).
3. **`23c88aba` is authoritative for visual geometry and interactions** where this spec does not
   declare a production-specific difference (typography, spacing, colours, layout, motion,
   click behaviour).

If two claims in this document conflict, the more specific section wins; if still ambiguous, ask.

---

## 1. Prerequisite — MR !315 must be merged first

<https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/315> (TASK-418) introduces the
normalized framework/requirement/mapping model that this UI depends on. Do not start until it is
merged into `dev`. After the merge, the following already exist and **must be consumed, not
re-implemented**:

Server routes (`packages/default/crates/cf-server/src/bin/server.rs`):

```text
GET    /api/v1/compliance/frameworks
GET    /api/v1/compliance/frameworks/:id/versions
GET    /api/v1/compliance/frameworks/:id/mapped-policy-versions
GET    /api/v1/compliance/framework-versions/:fv_id/requirements
GET    /api/v1/compliance/requirement-versions/:rv_id/children
GET    /api/v1/compliance/bundle-versions/:bv_id/requirement-coverage
GET    /api/v1/compliance/bundle-versions/:bv_id/requirements
GET    /api/v1/policy-versions/:pv_id/requirement-mappings
POST   /api/v1/policy-versions/:pv_id/requirement-mappings
PUT    /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id
DELETE /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id
```

web-ui client functions (`packages/web-ui/src/api/client.rs`): `fetch_compliance_frameworks`,
`fetch_compliance_framework_versions`, `fetch_framework_mapped_policy_versions`,
`fetch_requirement_children`, `fetch_bundle_requirement_coverage`,
`fetch_bundle_version_requirement_membership`, `fetch_policy_requirement_mappings`,
`create_policy_mapping`, `update_policy_mapping`, `delete_policy_mapping`.

Coverage response model (`packages/web-ui/src/api/models.rs`): `BundleCoverageReport`
(`bundle_version_id`, `total_requirements`, `full`, `partial`, `unmapped`, `rows`),
`BundleCoverageRow` (`requirement_version_id`, `external_id`, `title`, `kind`,
`parent_requirement_version_id`, `coverage`, `mapped_policy_version_ids`, `mappings`),
`BundleCoverageMapping` (`policy_version_id`, `policy_name`, `relationship`, `coverage`,
`provenance`, `rationale`), `RequirementCoverage` = `full | partial | unmapped`.

---

## 2. Design mock → Crystal Forge domain mapping

The design example is a mock with its own data shapes. Translate as follows. Never invent a
lineage/revision concept that the server does not have.

| Design mock concept | Crystal Forge production equivalent |
| --- | --- |
| `COMPLIANCE_BUNDLES` entry | one `ComplianceBundleSummary` from `GET /api/v1/compliance/bundles` |
| `groupBundlesByLineage()` "lineage" | a single CF bundle (`ComplianceBundleSummary`) |
| lineage "revision" | an entry in `ComplianceBundleSummary.versions` (`ComplianceBundleVersionSummary`) |
| `bundle.publicationState` (`current`/`accepted`/`draft`/`deprecated`) | `ComplianceBundleVersionSummary.publication_state` / `trust_state`; the "current" chip maps to `is_current_published` |
| `bundle.policyIds.length` (`N controls`) | **do not render as "controls".** TASK-418 split the legacy count into `ComplianceBundleSummary.policy_count` (implementation policies) and `.requirement_count` (normalized requirements). `control_count` still exists but is a **deprecated compatibility alias for the legacy policy count** — never display it. Render `{requirement_count} requirements · {policy_count} policies` (see §4.3). |
| `bundle.framework` / `bundle.version` | `ComplianceBundleSummary.framework` / `.version` |
| `bundleQuickStats(bundle).score` / `.systemCount` | see §7(a) — new server-computed aggregate on the bundle list |
| `stats` (`overallScore`, `pass/warn/fail/waiver`, `compliantHosts`, `totalHosts`) | `ComplianceRollupTotals` from `GET /api/v1/compliance/bundles/{id}/systems[?version_id=…]` |
| `bundleStatusForSystem()` per-host row | `ComplianceSystemRollup` from the same response |
| `bundleRequirementCoverage(bundle)` | `GET /api/v1/compliance/bundle-versions/{bv_id}/requirement-coverage` → `BundleCoverageReport` |
| `reqBreadcrumb(...)` top-level grouping | `BundleCoverageRow.parent_requirement_version_id` chain, resolved within `rows` |
| `POLICIES.find(p => p.id === m.policyId)` | `BundleCoverageMapping` → §7(b) adds `policy_id` |

**Never do N+1 fetching.** The bundle table renders every bundle; per-row data must come from a
single list request.

---

## 3. Page layout (`ComplianceView.jsx:61-166`)

Root is a vertical flex column, `gap: 16`.

1. `div.page-head` — unchanged from production: `h1.page-title "Compliance"`, `p.page-subtitle`
   "Walk through compliance bundles, review per-control evidence, export for auditors.", then the
   `IOMenu` ("Import / Export") and the admin-only `New bundle` primary button.
   - The first `IOMenu` item label is **`Import STIG (.xml)`** normally and **`Resume STIG import…`**
     when a paused import draft exists (`ComplianceView.jsx:72`).
2. Paused-import callout (§8.2), rendered only when a draft exists and the import modal is closed.
3. `BundleListTable` — full width (§4).
4. `BundleDetailDrawer` — rendered when a bundle is selected, the drawer is open, and no policy
   drawer is open (§5).
5. Evidence drawer — unchanged production `EvidenceDrawer`, opened from a host row inside the
   bundle drawer.
6. Policy drawer overlay (§5.4).
7. Existing modals (export, new bundle, edit bundle, import STIG).

**Removed:** the 320px left `BundleCatalog` rail and the `display:grid;grid-template-columns:320px 1fr`
wrapper in `packages/web-ui/src/views/compliance.rs`. Bundle detail no longer renders inline in a
right-hand column; it renders in the drawer.

---

## 4. Bundle list table (`ComplianceView.jsx:184-283`)

Outer element: `div.card` with `overflow:hidden`.

### 4.1 Filter header

`padding:"10px 16px"`, `borderBottom:"1px solid var(--cf-card-border)"`, flex column, `gap:10`.

- **Framework chip row** — `display:flex; gap:6; flex-wrap:nowrap; overflow-x:auto`.
  - First chip: `All <span>{total bundle count}</span>`.
  - Then one `button.cf-fw-chip` per distinct `framework` value, ordered by **bundle count
    descending, then framework name ascending** (deterministic tie-break so equal-count fixtures
    produce stable, comparable screenshots), label `{framework} <span>{count}</span>`.
  - Active chip carries `.active`. Counts are computed from the loaded bundle list.
- **Search** — `div.q-search` (`marginLeft:0; width:100%; box-sizing:border-box`) containing the
  `search` icon (13px), `input.q-search-input` with placeholder `Search bundles…`, and, only while
  the query is non-empty, `span.q-search-count` reading `{matching} of {total}` plus a
  `button.btn-icon.xs` (title `Clear search`) with an `x` icon (13px).
- Search matches case-insensitively against bundle name, framework and version.
  Framework chip and search filters compose (AND).

### 4.2 Empty result state

When filters match nothing: `div.q-empty` containing a `search` icon (20px) and the text
`No bundles match “{query}”.` (curly quotes). The table is not rendered.

### 4.3 Table

`table.sys-table.sys-table-fixed` with `<colgroup>` widths `38% | 16% | 18% | 18% | 10%`.

Header cells: `Bundle`, `Framework`, `Version`, `Score`, and a right-aligned blank header.

Row (`<tr>`, clickable, opens the drawer in `overview` view; carries class `selected` when it is the
selected bundle):

| Column | Content |
| --- | --- |
| Bundle | flex row `gap:8`: a `7×7` circle (`border-radius:50%`, `flex-shrink:0`) coloured by score (§4.4), then the bundle name at `font-weight:600; font-size:13`, single-line with ellipsis. Second line `font-size:11; color:var(--cf-text-muted); margin-top:2`: `{requirement_count} requirements` plus ` · {policy_count} policies` and, only when `versions.len() > 1`, ` · {n} revisions`. **Never display `control_count` or the word "controls"** — the design mock's "N controls" meant count of implementation policies, and TASK-418 split that into `requirement_count`/`policy_count` with `control_count` retained only as a deprecated alias. |
| Framework | `span.chip.chip-info` with the framework string |
| Version | `div.mono` `font-size:12` with the version string; below it (`margin-top:3`) a publication-state chip (§4.5) |
| Score | `span.mono` `font-size:13; font-weight:600`, colour by score, text `{score}%` or `—` when unknown. Second line `font-size:11; color:var(--cf-text-muted); margin-top:2`: `{n} system` / `{n} systems` |
| actions | right-aligned `div.row-actions` (`opacity:1; justify-content:flex-end`) containing a `button.btn-icon` (title `View bundle`) with an `arrow-right` icon (14px). Clicking must not double-fire the row handler. |

### 4.4 Score colour function (`ComplianceView.jsx:177-182`)

```text
score == null  → var(--cf-text-muted)
score >= 90    → #34d399
score >= 70    → #fbbf24
otherwise      → #f87171
```

### 4.5 Publication-state chip (`ComplianceView.jsx:171-175`)

`span.chip`, `font-size:9`, `padding:"1px 6px"`, `color: C`,
`background: color-mix(in oklab, C 16%, transparent)` where

```text
current → #34d399   accepted → #60a5fa   deprecated → #6b7280   draft → #fbbf24   fallback → #6b7280
```

---

## 5. Bundle detail drawer (`ComplianceView.jsx:285-394`)

`div.fl-tray-backdrop` (click closes) + `aside.fl-tray` with `width: min(900px, 96vw)`.
Both classes already exist in `packages/web-ui/assets/app.css`.

The drawer has two views held in one state value: `overview` and `coverage`.

### 5.1 Header (`header.fl-tray-head`)

- **overview**: `shield` icon 18px in `var(--cf-brand-purple)`, then the label
  `Compliance bundle` at `font-size:11; color:var(--cf-text-muted)`. Right side: admin-only
  `button.btn.btn-ghost.xs` with an `edit` icon (12px) and label `Edit bundle`, then a
  `button.btn-icon` close button with an `x` icon (16px).
- **coverage**: a `button.btn-icon` with `arrow-left` (16px) returning to `overview`, then the title
  `Requirement coverage` (`font-weight:700; font-size:15`) with the bundle name underneath
  (`font-size:11; color:var(--cf-text-muted); margin-top:2`). Right side: close button only.

### 5.2 Overview body (scrollable, `overflow:auto; flex:1`)

In order:

1. `padding:"14px 18px"` wrapper containing `BundleHeader` — the existing production component, but
   **without** the outer `.card` (it becomes a plain flex column, `gap:10`). Bundle name `h2`
   `font-size:18; font-weight:700`, framework/version/layer chips, owner, last review, required
   environment pills, description.
2. `div.stat-strip.stat-strip-flush` with `border-top:1px solid var(--cf-divider)` — 5 equal
   columns, no accent bars, no `.stat-meta`: `Overall score` (`{overall_score}%`, coloured by the
   §4.4 thresholds) then `Pass` `#34d399`, `Warn` `#fbbf24`, `Fail` `#f87171`, `Waiver` `#a78bfa`.
3. Revisions section — only when the bundle has more than one version. `border-top`,
   `padding:"12px 18px"`. A collapsed-by-default disclosure button showing
   `chevron-right`/`chevron-down` (13px), the label `Revisions` (`font-size:13; font-weight:600`)
   and `{n} total` (`font-size:11; muted`). Expanded content is a wrapped flex row (`gap:8`,
   `margin-top:12`) of revision buttons: `padding:"7px 10px"; border-radius:8`, background
   `var(--cf-subtle-bg)` (selected: `color-mix(in oklab,var(--cf-brand-purple) 12%, transparent)`
   with a `1px solid var(--cf-brand-purple)` border). Each shows
   `Rev {revision} · {version}` (mono, `font-size:11.5; font-weight:600`), a `Current` chip
   (`#34d399`, `font-size:8.5`) on the current published version, and a second line with the
   publication-state chip and the published date (`font-size:10; muted`). Selecting a revision
   re-scopes the drawer (stats, coverage, systems) to that version.
4. Requirement-coverage summary row (§5.3), inside a `border-top` wrapper.
5. Systems drilldown (§5.5), inside a `border-top` wrapper.

Existing production functionality that has no design counterpart — the XCCDF version `<select>`,
`BundleVersionActions` (trust / publish / create draft) and the `Assign bundle` panel — moves into
the overview body (below the revisions section) with behaviour and admin gating unchanged. Do not
delete it.

### 5.2a Single source of truth for the selected bundle version

There is exactly **one** selected bundle-version signal for the whole Compliance view. Production
already has `selected_export_version_id` (`packages/web-ui/src/views/compliance.rs:68`), which
drives systems, evidence and version actions. **Rename it to `selected_bundle_version_id`
throughout the view rather than introducing a second signal** (do not create
`selected_revision_id`, `coverage_version_id`, etc.).

```text
On bundle selection:
  current_published_version_id, else current_draft_version_id, else first available version

Revision buttons in the drawer:      write selected_bundle_version_id
Existing XCCDF revision <select>:    writes the same selected_bundle_version_id
Systems request:                     uses selected_bundle_version_id
Requirement-coverage request:        uses selected_bundle_version_id
Evidence request:                    uses selected_bundle_version_id
Trust / publish / create-draft:      operate on selected_bundle_version_id
```

If a bundle has no version at all, the systems/coverage sections render their empty states and the
version actions are disabled. Every request that carries a version must read this single signal, so
stats, coverage, and the Systems table can never disagree with each other.

### 5.3 Requirement-coverage summary row (`ComplianceView.jsx:396-423`)

`padding:16`. When the coverage report has `total_requirements == 0`, render a static block:
bold `Requirement coverage` (`font-size:13`) and, below it, `No requirement catalog modeled for
{framework name} yet.` (`font-size:12; muted`).

Otherwise render a full-width button:

- Left: `Requirement coverage` (`font-size:13; font-weight:600`), then
  `{framework name} · {total} requirements · derived from mapped policies, not policy tags`
  (`font-size:11; muted`).
- Right: three chips at `font-size:9.5` — `{full} full` (`#34d399`), `{partial} partial`
  (`#fbbf24`), `{unmapped} unmapped` (`chip.chip-unknown`) — followed by a muted `chevron-right`
  icon (13px).
- Clicking switches the drawer to the `coverage` view.

### 5.4 Coverage view body (`ComplianceView.jsx:425-506`)

Filter bar: `padding:"12px 18px"`, `border-bottom`, flex, `gap:10`, wrap.

- `div.seg` with four buttons: `All {total}`, `Full {full}`, `Partial {partial}`,
  `Unmapped {unmapped}`; the active one has `.active`.
- `div.q-search` pushed right (`margin-left:auto`) with placeholder `Filter requirements…` and a
  clear button while non-empty. Matches case-insensitively on requirement external ID and title.

Body: `overflow:auto; flex:1; padding:"14px 18px"`, flex column, `gap:16`.

- Rows are grouped by their **top-level** requirement ancestor (walk
  `parent_requirement_version_id` up within the report; rows with no parent are their own root).
  A group heading `{externalId} — {title}` (`font-size:11.5; font-weight:700; margin-bottom:6`) is
  rendered **unless** the group contains exactly one row and that row *is* the root.
- Each requirement row: `display:flex; justify-content:space-between; gap:10;
  padding:"6px 9px"; background:var(--cf-subtle-bg); border-radius:7`.
  - Left: mono external ID (`font-size:11.5; font-weight:600; white-space:nowrap`), then the title
    (`font-size:11; color:var(--cf-text-secondary); margin-left:6`).
  - When the row has mappings, a second line (`margin-top:4`, flex, `gap:6`, wrap): the label
    `ENFORCED BY` (`font-size:9.5; font-weight:600; muted; text-transform:uppercase;
    letter-spacing:.03em`) followed by one `button.cf-policy-link` per mapped policy showing a
    `file` icon (10px), the policy name, and an `arrow-right` icon (10px).
  - Right: a status chip at `font-size:9` with `color`/`background` from
    `full → #34d399`, `partial → #fbbf24`, `unmapped → #6b7280`, and text
    `Fully covered` / `Partially covered` / `Unmapped`.
- When filters exclude everything: centred muted text `No requirements match.`
  (`font-size:12; padding:"24px 0"`).

Clicking a `cf-policy-link` opens the **policy drawer as an overlay on top of the Compliance view**
(`ComplianceView.jsx:113,127-135`) — it does **not** navigate to `/deployment-policies`. While the
policy drawer is open the bundle drawer is not rendered; closing the policy drawer returns to the
bundle drawer still in `coverage` view with filters intact.

### 5.4a Policy drawer drill-in — data-loading path

Extract the existing private `PolicyDrawer` (`packages/web-ui/src/views/policies.rs:762`) into a
reusable component (e.g. `packages/web-ui/src/components/policy/`) **without behavioural change**.
Do not write a second, partial drawer.

The gap the implementer must close is: a coverage row only carries `BundleCoverageMapping.policy_id`,
but `PolicyDrawer` consumes the richer `PolicyDefinition` model. Require:

```text
1. Move PolicyDrawer (and its sub-components) verbatim into a shared component; keep PoliciesView
   using it with identical behaviour. Existing policies web-ui checks must stay green.

2. Wire the coverage drill-in through the same policy-loading path PoliciesView already uses:
   policies_api::load_policies() (packages/web-ui/src/views/policies.rs:47). Do NOT build a
   separate partial PolicyDefinition construction.

3. Look up the policy by BundleCoverageMapping.policy_id within the loaded library; PolicyDrawer
   receives the resolved PolicyDefinition.

4. Load the policy library lazily on first Enforced-by click, or once with the page — but never
   one request per coverage row and never one request per click on the same session.

5. While policy data is loading show the standard loading state used by PoliciesView.

6. If policy_id cannot be resolved in the library, surface an explicit error state (with a close
   affordance) rather than silently rendering an empty drawer. The bundle drawer must still be
   restored intact when the policy drawer closes.
```

### 5.5 Systems drilldown (`ComplianceView.jsx:534-620`)

No `.card` wrapper (the drawer supplies the surface).

Header: `padding:"12px 16px"`, `border-bottom`, `h3 Systems` (`font-size:13; font-weight:600`),
the existing 4-way `div.seg` filter (`All | Clean | Warning | Failing`), the `{n} hosts` count, and
the existing pinned-revision info callout — all unchanged from production behaviour.

Table: `table.sys-table.compact.sys-table-dense` with `<colgroup>`
`22% | 90px | 120px | 110px | 60px | 70px | 60px | 76px | 52px`.

Columns: `Host`, `Env`, `Assignment`, `Score`, then right-aligned `Pass`, `Warn`, `Fail`,
`Waiver`, and a right-aligned blank action column.

- Score cell: a 40px × 5px track (`background:var(--cf-subtle-bg); border-radius:99;
  overflow:hidden; flex-shrink:0`) filled to `{score}%` with the §4.4 colour, then mono `{score}%`
  (`font-size:12; font-weight:600`) in the same colour.
- Numeric cells are right-aligned mono: `pass` `#34d399` bold; `warn` `#fbbf24` bold when `> 0`
  else muted; `fail` `#f87171` weight 700 when `> 0` else muted; `waiver` `#a78bfa` when `> 0` else
  muted.
- The action cell is an icon-only `button.btn-icon` (title `View evidence`) with `arrow-right`
  (14px) — the old `View evidence →` text button is gone. Clicking it must not double-fire the row
  click.

---

## 6. CSS additions

Add to `packages/web-ui/assets/app.css` (values copied verbatim from
`docs/design/CrystalForge/styles.css` at `23c88aba`). `.q-search`, `.seg`, `.stat-strip`,
`.sys-table`, `.chip-info`, `.fl-tray*`, `.sd-callout`, `.sd-callout-info` and
`.sd-callout-danger` already exist; `.sd-callout-warn` does not.

```css
a { color: var(--cf-brand-purple); }
a:hover { color: var(--cf-brand-purple-hover); }

.cf-policy-link { all: unset; cursor: pointer; display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; color: var(--cf-brand-purple); font-family: var(--font-mono); font-size: 10.5px; font-weight: 600; padding: 3px 8px; border-radius: 6px; background: color-mix(in oklab, var(--cf-brand-purple) 10%, transparent); border: 1px solid color-mix(in oklab, var(--cf-brand-purple) 22%, transparent); }
.cf-policy-link svg { flex-shrink: 0; }
.cf-policy-link:hover { color: var(--cf-brand-purple-hover); background: color-mix(in oklab, var(--cf-brand-purple) 16%, transparent); border-color: color-mix(in oklab, var(--cf-brand-purple) 32%, transparent); }
.cf-policy-link svg:last-child { opacity: .6; }

.cf-fw-chip {
  all: unset; cursor: pointer; font-size: 11px; font-weight: 600; padding: 3px 9px; border-radius: 999px;
  color: var(--cf-text-secondary); background: var(--cf-subtle-bg); border: 1px solid var(--cf-divider);
  display: inline-flex; align-items: center; gap: 4px; white-space: nowrap;
}
.cf-fw-chip span { color: var(--cf-text-muted); font-weight: 400; }
.cf-fw-chip.active { color: var(--cf-brand-purple); background: color-mix(in oklab, var(--cf-brand-purple) 14%, transparent); border-color: color-mix(in oklab, var(--cf-brand-purple) 40%, transparent); }
.cf-fw-chip.active span { color: inherit; opacity: 0.75; }

.stat-strip.stat-strip-flush { margin-bottom: 0; gap: 8px; padding: 10px 18px; grid-template-columns: repeat(5, minmax(0, 1fr)); }
.stat-strip.stat-strip-flush .stat-label, .stat-strip.stat-strip-flush .stat-value { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stat-strip.stat-strip-flush .stat-value { font-size: 17px; }
.stat-strip.stat-strip-flush .stat-label { font-size: 9.5px; }
.stat-strip.stat-strip-flush .stat-meta { display: none; }
.stat-strip.stat-strip-flush .stat { border: none; border-radius: 0; background: transparent; padding: 0; }
.stat-strip.stat-strip-flush .stat:first-child { padding-left: 0; }
.stat-strip.stat-strip-flush .stat-accent { display: none; }

.sys-table-dense tbody td, .sys-table-dense thead th { padding-top: 8px; padding-bottom: 8px; }
.sys-table-dense tbody td { font-variant-numeric: tabular-nums; }
.sys-table-fixed { table-layout: fixed; }
.sys-table-dense { table-layout: fixed; }
.sys-table-dense td, .sys-table-dense th { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.sd-callout-warn {
  background: rgba(251,191,36,0.08);
  border-color: rgba(251,191,36,0.28);
  color: var(--cf-text-primary);
}
.sd-callout-warn svg { color: #fbbf24; flex-shrink: 0; margin-top: 1px; }
```

Verify each new rule renders correctly in both `dark` and `light` themes.

---

## 7. Required backend changes

Only two, both additive and backward compatible. Everything else the view needs already exists.

### (a) Per-bundle aggregate score and system count on the bundle list

`GET /api/v1/compliance/bundles` currently returns no score and no applicable-system count, so the
bundle table cannot render the §4.3 `Score` column without one request per bundle.

Extend `ComplianceBundleSummary`
(`packages/default/crates/cf-server/src/api/models.rs`, backed by
`queries::compliance::list_bundles`) with:

```rust
#[serde(default)]
pub applicable_system_count: i64,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub aggregate_score: Option<i64>,
```

Semantics must match the existing rollup so the drawer and the table never disagree:

- Scope: the bundle's current published version, falling back to the current draft version — the
  same pointer `on_select_bundle` already resolves.
- `applicable_system_count` = number of systems the bundle applies to (`ComplianceSystemRollup.applies`).
- `aggregate_score` = the same value `ComplianceRollupTotals.overall_score` produces for that
  version; `None` when no system applies or no control is evaluated (rendered as `—`).
- Computed set-based in the existing list query. It must not become one query per bundle.

**Do not independently reimplement the compliance scoring formula in a new SQL expression if the
existing implementation can be reused.** Fetch the bundle/system/policy inputs set-wise, group them
in Rust, and reuse the existing rollup/totals functions where possible. "Set-based" means *no
per-bundle database query*; it does **not** require duplicating the evaluation semantics in SQL.

Required tests (server/API level):

```text
no applicable systems           → applicable_system_count 0, aggregate_score None
applicable but none evaluated   → count N, aggregate_score None
all pass                        → aggregate_score 100
mixed pass/warn/fail/waiver     → equals systems-endpoint totals for the same version
published + draft versions      → published is selected
draft only                      → draft version is selected
```

Mirror the fields in `packages/web-ui/src/api/models.rs`.

### (b) `policy_id` on coverage mappings

`BundleCoverageMapping` exposes `policy_version_id` and `policy_name` but no policy identity, so the
"Enforced by" chip cannot open the policy drawer. Add `pub policy_id: Uuid` to the server and
web-ui `BundleCoverageMapping` structs and populate it from the existing join in
`queries::framework_requirements::compute_bundle_requirement_coverage`.

Both changes require regenerated SQLx offline metadata if query shapes change
(see `docs/agent/database-safety.md`; only run preparation against the isolated local dev DB).

---

## 8. STIG import pause / resume (`ImportStigModal.jsx:195-260`, `ComplianceView.jsx:84-95`)

### 8.1 Modal behaviour

- Draft storage key: **`cf-stig-import-draft`**.
- The draft captures wizard progress: current step, file name, bundle name, selected environments,
  refine cursor position, and which controls/rules are selected or already refined.
- The draft is written whenever that state changes; it is **removed** when the wizard is back at the
  `upload` step with nothing parsed, and when the import completes.
- Clicking the backdrop no longer closes the modal.
- On the `review`, `reconcile` and `refine` steps the header gains, on the right:
  a `Discard draft` text button (`font-size:11; muted; underline`) that clears the draft and resets
  the wizard to `upload`, and a `button.btn-icon` close button (`x`, 16px) whose title is
  **`Pause — your progress is saved`**.
- Reopening the modal while a draft exists resumes at the stored step with the stored state.

### 8.2 Paused-import callout on the Compliance page

Rendered between the page head and the bundle table when a draft exists and the modal is closed:
`div.sd-callout.sd-callout-warn` with `justify-content:space-between`.

- Left: `shield` icon (14px) then, at `font-size:12.5`:
  `Paused STIG import — <strong>{benchmark title or bundle name or "unnamed benchmark"}</strong>, {n} of {m} controls selected.`
  When per-control counts are unavailable, the trailing clause is `in progress`.
- Right: `button.btn.btn-ghost.xs` `Discard` (clears the draft) and `button.btn.btn-primary.xs`
  `Resume` (reopens the modal).

### 8.3 Crystal Forge constraints (differences from the mock)

The mock stores the whole parsed benchmark in `localStorage`. In production the parsed benchmark
comes from `POST /api/v1/compliance/xccdf/preview` and can be large, and `localStorage` is limited
(~5 MB) and browser-local.

#### 8.3.1 TASK-418 preservation invariant (read before implementing)

**Do not replace the production STIG/XCCDF state machine with the simpler design-example state
machine.** The design commit is authoritative for *appearance and pause/resume interaction only*.
All reconciliation states, decisions, request models, rule actions, mapping semantics, and commit
behaviour delivered by TASK-418 (MR !315) must remain intact. Pause/resume must **wrap the
post-TASK-418 production workflow**, not re-implement it.

The post-!315 production `ImportStigModal` state machine is:

```text
upload → native-review → review → reconcile → refine → final-review → committing → done
```

- `native-review` is the CF-native benchmark review (post-XCCDF parse).
- `reconcile` contains the normalized requirement/reuse workflow: native controls vs. reusable
  policies, shared-implementation reconciliation, reviewed-related decisions.
- `final-review` is the summary before commit.

A design-copy could "look right" while deleting `native-review`/`reconcile`/`final-review`
semantics. Verify the resumed flow still exercises every one of these states and preserves the
decisions/request shapes TASK-418 introduced. See also `packages/web-ui/src/views/compliance.rs`
on the TASK-418 branch (step values `"upload" | "native-review" | "review" | "reconcile" |
"refine" | "final-review" | "committing" | "done"`).

#### 8.3.2 Draft persistence — schema, size guard, and the raw-file problem

Required production behaviour:

- Use a browser-compatible storage API (`web_sys` local storage — see existing usage in
  `packages/web-ui/src/views/systems_list.rs` and `components/widget_grid.rs`). Do not assume any
  native/`std` filesystem behaviour.
- **Never persist raw uploaded benchmark bytes in `localStorage`.** After a page reload the
  original file bytes are gone (they live only in the in-memory `file_bytes` signal, and the
  commit path `import_xccdf(&bytes, &filename, &plan)` needs them).
- Persist a **versioned, size-guarded** object under `cf-stig-import-draft` with at least:

  ```text
  version: 1                                  ← schema version; bump on wizard shape changes
  step                                      ← current state-machine step
  original_filename
  expected_sha256                           ← verified against the parsed preview bytes
  bundle_name, environments
  refine cursor position, selected/refined control identity
  parsed preview payload                    ← only when it fits the guard
  ```

  Size guard (deterministic, do not invent a different one):

  ```text
  MAX_STIG_IMPORT_DRAFT_BYTES = 2 * 1024 * 1024   (2 MiB)
  ```

  If the serialized draft exceeds the guard, persist wizard **metadata only** (step, file name,
  bundle name, environments, cursor, selected/refined control identity) and omit the parsed
  payload.

- **Source-file re-attachment before commit (the raw-file problem).** Because the original bytes
  are never persisted, a resumed workflow that reaches a commit-capable state must:

  ```text
  1. Require the operator to re-select the source benchmark file before commit is allowed.
  2. Run the persisted expected SHA-256 against the re-selected file's parsed preview bytes.
  3. Reject a different artifact (mismatching SHA/summary) with a clear error; never commit it.
  4. Then restore the saved workflow state (decisions, refinement, selections) on top of the
     re-attached artifact.
  ```

  The UI **may** display the restored review/reconciliation state from the saved preview *before*
  re-selection, but the final import action (the button that would call `import_xccdf`/commit)
  must remain **disabled** until the matching source artifact has been reattached.

- If the parsed payload is not restorable on resume (oversize/omitted/corrupt), reopen at the
  `upload` step with the previously entered bundle name and environments preserved and an explicit
  prompt to re-select the benchmark file; if commit would be needed, also require SHA re-verification
  as above. Never silently drop the operator's work and never resume into a step whose data is
  missing.
- Never persist credentials, session tokens, signed URLs or any authorization header material in the
  draft.
- Corrupt or unparsable draft data (wrong `version`, missing fields, bad JSON) must be treated as
  "no draft" rather than causing a panic; a later wizard change must not break deserialization of
  stale browser state (hence the `version` field).

---

## 9. Reviewer verification endpoints

Start both sides and compare each state below in `dark` **and** `light`.

### 9.1 Run the design example (reference)

```bash
bash docs/design/CrystalForge/serve.sh 8081
```

Use port `8081` — `run-ui-dev` already serves the real UI on `8080`.

### 9.2 Run Crystal Forge locally with the same golden fixture

```bash
nix develop
run-ui-dev
```

This starts PostgreSQL, seeds
`docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json` (the same fixture the design example
reads), starts the API on `http://127.0.0.1:3445` and the UI on `http://localhost:8080`.
Sign in with `admin` / `password` (`AUTH_MODE=local`); admin is required for
`New bundle`, `Edit bundle`, STIG import and bundle version actions.

### 9.3 Captured design-example reference screenshots

Pre-captured screenshots of the design example (compliance redesign states,
dark theme) live in
`docs/design/CrystalForge/screens/compliance-redesign/cb-1.png` … `cb-7.png`.
They are a quick visual reference for the implementer and reviewer; the
state-by-state walkthrough below remains the authoritative comparison method.

| File | Notes |
| --- | --- |
| `screens/compliance-redesign/cb-1.png` | Bundle list (dark) |
| `screens/compliance-redesign/cb-2.png` | Bundle list (light) |
| `screens/compliance-redesign/cb-3.png` | Bundle drawer — overview |
| `screens/compliance-redesign/cb-4.png` | Revisions expanded / version selection |
| `screens/compliance-redesign/cb-5.png` | Requirement coverage view |
| `screens/compliance-redesign/cb-6.png` | Coverage filters / policy drill-in |
| `screens/compliance-redesign/cb-7.png` | Systems drilldown / evidence |

If a capture does not match its row here, treat the live design example
(`9.1`) as the source of truth and update this table.

### 9.4 State-by-state comparison table

| # | State | Design example | Crystal Forge |
| --- | --- | --- | --- |
| 1 | Bundle list, dark | `http://localhost:8081/crystal-forge.html?view=compliance&theme=dark` | `http://localhost:8080/compliance` (theme: dark) |
| 2 | Bundle list, light | `…?view=compliance&theme=light` | `http://localhost:8080/compliance` (theme: light) |
| 3 | Framework chip filter + search | state 1, click a framework chip, type in `Search bundles…` | same interaction on `/compliance` |
| 4 | Empty filter result | state 1, search `zzzz` | same |
| 5 | Bundle drawer — overview | state 1, click any bundle row | same |
| 6 | Revisions expanded | state 5, click `Revisions` | same (bundle must have >1 version) |
| 7 | Requirement coverage view | state 5, click the `Requirement coverage` row | same |
| 8 | Coverage filters | state 7, use the `All/Full/Partial/Unmapped` segment and `Filter requirements…` | same |
| 9 | Policy drawer from coverage | state 7, click an `Enforced by` policy chip | same |
| 10 | Systems drilldown | state 5, scroll to `Systems`, use the seg filter | same |
| 11 | Evidence drawer | state 10, click a host row's `arrow-right` action | same |
| 12 | STIG import — paused draft | Import / Export → `Import STIG (.xml)`, advance to `review`, close with the pause button | same |
| 13 | Paused-import callout + menu label | state 12, observe the warn callout and the `Resume STIG import…` menu label | same |

### 9.5 Backing API endpoints (Crystal Forge, `http://127.0.0.1:3445`)

Use these to confirm the UI is rendering server data rather than derived guesses:

```text
GET /api/v1/compliance/bundles
GET /api/v1/compliance/bundles/{bundle_id}/systems?version_id={bundle_version_id}
GET /api/v1/compliance/bundles/{bundle_id}/systems/{system_id}/evidence
GET /api/v1/compliance/bundle-versions/{bundle_version_id}/requirement-coverage
GET /api/v1/compliance/bundle-versions/{bundle_version_id}/requirements
GET /api/v1/compliance/bundle-versions/{bundle_version_id}/policies
GET /api/v1/compliance/frameworks
GET /api/v1/policy-versions/{policy_version_id}/requirement-mappings
```

Checks a reviewer should make:

- Every `Score` / `{n} systems` value in the bundle table appears in
  `GET /api/v1/compliance/bundles` (fields `aggregate_score`, `applicable_system_count`) — not
  assembled by one request per row (confirm in the browser network panel).
- `full` / `partial` / `unmapped` chip counts equal the `full` / `partial` / `unmapped` fields of
  the coverage response for the selected bundle version.
- Each `Enforced by` chip corresponds to an entry in that requirement's `mappings[]`.
- Drawer stat-strip numbers equal `totals` from the bundle systems response for the selected
  version.

---

## 10. Required implementation sequence

Implement in this order; each phase ends at a STOP point with a commit and its own verification
before starting the next phase. Do not reorder phases to "get the design visible early" — the
server contract and the reusable drawer must exist first.

**Phase 0 — prerequisite/baseline (no code changes)**
- Verify MR !315 is merged into `dev`.
- Branch from updated `dev`.
- Run the existing compliance browser steps (29…29e) to confirm the pre-change baseline.
- Refresh/reference the captured design screenshots `cb-1.png` … `cb-7.png` (§9.3).
- Do not change code yet.

**Phase 1 — additive server contract**
- `aggregate_score` + `applicable_system_count` on the bundle list (§7a), reusing the existing
  rollup functions with the required tests.
- `BundleCoverageMapping.policy_id` (§7b).
- Regenerate SQLx offline metadata if query shapes change.
- Server/API tests green; `nix build .#server` passes.

> STOP: commit + verify.

**Phase 2 — reusable PolicyDrawer + single version signal (no visual redesign)**
- Extract `PolicyDrawer` into a shared component without behavioural change (§5.4a); existing
  Policies web-ui checks must remain green.
- Rename `selected_export_version_id` → `selected_bundle_version_id` and establish it as the single
  source of truth (§5.2a).
- Do not touch the bundle table/drawer layout yet.

> STOP: commit + verify (touches existing surfaces; confirm no visual regressions).

**Phase 3 — bundle table + drawer overview**
- Full-width table, framework chips (deterministic order), search, empty state (§4).
- Drawer shell/header (§5.1), overview body (§5.2): BundleHeader (card-less), stat-strip-flush,
  revisions disclosure, relocated XCCDF selector/version actions/assignment panel.
- Systems drilldown dense layout (§5.5).
- Browser steps + dark/light screenshots.

> STOP: commit + screenshots + browser checks.

**Phase 4 — requirement coverage**
- Exact-version coverage fetch with a generation guard (§5.3, §5.4).
- Segmented filter, search, ancestor grouping.
- Enforced-by policy drawer overlay + coverage-state restoration (§5.4a).

> STOP: commit + dark/light screenshots.

**Phase 5 — STIG pause/resume**
- Versioned draft schema (`version: 1`), `MAX_STIG_IMPORT_DRAFT_BYTES = 2 MiB` size guard,
  no raw bytes persisted (§8.3.2).
- Preserve the post-TASK-418 state machine (§8.3.1); pause/resume wraps it.
- Source-file re-attachment + SHA-256 verification before commit; disable the commit action until
  the matching artifact is reattached.
- Paused-import callout + `Resume STIG import…` menu label (§8.2).
- Corrupt/oversize/missing-payload paths.

> STOP: commit + focused browser proof.

**Phase 6 — final visual/acceptance pass**
- CSS parity pass (§6) in both themes.
- Update steps 29…29e and add focused NixOS check (`checks/web-ui-reconciliation` pattern).
- 13-state dark/light walkthrough (§9.4) with screenshots; network assertion that the Score column
  is served by a single bundle-list request.
- Full builds and checks (§11 "Verification" below).

> STOP: open MR, record reviewer results.

---

## 11. Verification

- `nix build .#web-ui -L`
- `nix build .#server --no-link` (only if server code changed)
- `nix develop --command cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check`
- Web-ui browser proof: the authoritative `nix build .#checks.x86_64-linux.web-ui -L` run, plus a
  focused NixOS check for the new states. Follow the existing focused-check pattern in
  `checks/web-ui-reconciliation/default.nix`, which drives `integration-test.js` with
  `CF_UI_TEST_PROFILE=ci_fast CF_UI_TEST_STEPS=<step-name>` and asserts `results.json` plus dark and
  light screenshots.
- Existing compliance steps in `checks/web-ui/tests/integration-test.js`
  (`29-compliance-empty`, `29a-compliance-populated`, `29b-compliance-evidence-drawer`,
  `29c-compliance-export-modal`, `29d-compliance-new-bundle-modal`, `29e-compliance-api-error`)
  assert the old layout and text and must be updated to the new layout in the same change.
  Several of them fail today; update the assertions to the new design rather than preserving old
  expectations.
- `checks/web-ui/coverage-manifest.json` must be updated for any added or renamed step.

---

## 12. Out of scope

- Changing the normalized framework/requirement/mapping model or its mapping CRUD semantics
  (owned by TASK-418 / MR !315).
- The Policies view's own layout, cards, editor modal and Mappings tab.
- Waiver workflow, evidence taxonomy and export format changes.
- Any new compliance evaluation logic; scores must keep coming from the existing rollup query.
