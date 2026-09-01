# TASK-433 Design Parity Review

This report records the authoritative design comparison inventory for
TASK-433. The authoritative source is the design delta
`c2f5db08..ae20da816edb1cb14275db9cd646010e69d88cd8` under
`docs/design/CrystalForge/`. The production implementation remains
server-authoritative where the design example uses fixture state.

Review date: 2026-09-01. The source and contract review uses the descendant of
candidate `da7ae7de480c485ee79db676f8d99893bf07c572` that contains this report.
The final SHA and visual CI artifact URL are pending. The table below does not
close visual acceptance until the exact-head CI comparison pairs are generated
and inspected.

## Evidence

The repository contains reviewed strict workflow captures under
`checks/web-ui/baselines/`. Across the six canonical TASK-433 workflows, the
baseline set includes desktop, narrow desktop, mobile, dark, and light states.
Not every workflow uses every viewport profile.

The Web UI check also generates these non-blocking authoritative comparison
artifacts:

- `design-targets/`: rendered design-example targets;
- `design-parity/`: matching Dioxus targets;
- `design-drift-report.json`: per-target comparison results;
- `design-drift-summary.md`: the comparison summary;
- `montages/`: side-by-side comparison images.

`checks/web-ui/design-parity/manifest.json` identifies each target and its
authoritative design file. The manifest includes direct targets for the common
policy editor and the POA&M detail tray. CI must publish successful pairs for
both targets before TASK-433 AC40 and TASK-433.9 AC11 can close.

## Source and Contract Review

| Production surface | Authoritative source | Retained hierarchy and behavior | Reviewed production difference |
| --- | --- | --- | --- |
| Policy catalog | `PoliciesView.jsx` | Domain grouping, collapse and expansion, cards and table views, search, selection, export, and partial deletion remain visible. | Production uses server pagination, stable identity, authorization, and partial-delete results instead of the synthetic `POLICY_STIG_BULK` array. |
| Common policy editor | `PolicyEditor.jsx` | Basics, Enforcement, Compliance, Evidence, category guidance, Unmapped state, read-only imported mappings, and immutable provenance remain distinct. | The design renders imported Provenance as a read-only rail block. Production renders Provenance as a fifth read-only section tab so all sections remain reachable in the same responsive tab model. This changes placement, not mutability or provenance authority. |
| Compliance and evidence | `ComplianceView.jsx` | Failed controls remain FAIL. Finding-origin Create POA&M and Link existing actions retain exact system, bundle, policy, requirement, and evidence context. | Production separates remediation from waiver actions and resolves all finding compatibility on the server. The design fixture computes relationships in memory. |
| POA&M detail and lifecycle | `PoamViews.jsx` | Status, risk, owner, target, progress, findings, remediation plan, milestones, activity, verification, close, reopen, and exact evidence navigation remain present. | Production adds optimistic revisions, loading and authorization states, verification history, assignment references, and explicit save actions. Metadata uses `Save metadata`; the separate remediation text uses adjacent `Save plan`. Persistence is explicit at each section, and closing without a save action does not claim that the local draft persisted. |
| System POA&M | `SystemDetail.jsx`, `PoamViews.jsx` | System-scoped counts, filters, rows, and finding navigation remain visible in System Compliance. | Production counts and rows come from bounded server rollups rather than filtering the in-memory POA&M array. |
| Bundle POA&M | `ComplianceView.jsx`, `PoamViews.jsx` | Open findings, On POA&M, No POA&M, Overdue, Awaiting verification, Closed, and list navigation retain the design hierarchy. | Production batches visible bundle IDs and uses committed server rollups. It does not issue one POA&M query per bundle row. |
| Dashboard | `DashboardView.jsx`, `data-dashboard.js` | POA&M Summary and Watchlist retain status, urgency, owner, due date, and detail navigation. | Production persists widget layout and loads authorization-scoped summaries. It does not use local storage or mutable fixture arrays as domain authority. |
| Notifications | `Shell.jsx` | Overdue and awaiting-verification notifications remain available from the top bar and navigate to the exact POA&M. | Production uses durable deduplicated notification events, keyboard menu semantics, and Dioxus routing instead of delayed global events. |
| Setup Coach | `SetupCoach.jsx` | The nine-step hierarchy includes Track a POA&M and preserves current, complete, pending, and locked states. | Production derives completion from server state. It does not use `window.__cfCoach` or timeout sequencing. |
| Responsive shell and themes | `Shell.jsx`, `styles.css` | Desktop, narrow desktop, mobile, dark, and light layouts retain usable navigation, dialogs, action hierarchy, and semantic status colors. | Production uses the application theme and mobile drawer contracts. The strict baselines accept the narrow editor's explicit scroll cue and timestamp-only screenshot noise as P3 differences. |

## Design Delta Classification

The following changed files specify product behavior and are implemented by
TASK-433 criteria:

- `app.jsx`;
- `components/ComplianceView.jsx`;
- `components/DashboardView.jsx`;
- `components/PoamViews.jsx`;
- `components/PoliciesView.jsx`;
- `components/PolicyEditor.jsx`;
- `components/SetupCoach.jsx`;
- `components/Shell.jsx`;
- `components/SystemDetail.jsx`;
- `data-dashboard.js`;
- `data-enforcement.js`;
- `data-mappings.js`;
- `data-poam.js`;
- applicable control-family behavior in `data-policies.js`;
- `styles.css`.

The following files or mechanisms are demo-only and are not production domain
authority:

- `.thumbnail` and `crystal-forge.html`;
- `fixtures/crystal-forge.fixtures.js` and
  `fixtures/crystal-forge.fixtures.json`;
- fixture identity changes in `data.js`;
- `POAM_FINDING_STATUS_OVERRIDE`;
- synthetic `POLICY_STIG_BULK` and `POLICY_EDITOR_DEMO` data;
- mutable in-memory POA&M arrays and local-storage POA&M state;
- `CustomEvent` POA&M synchronization;
- `window.__cfCoach`;
- timeout-based navigation sequencing.

Production replaces each demo mechanism with persisted state, authenticated
APIs, Dioxus state and routing, or deterministic browser fixtures. The
authoritative design files are not modified by TASK-433.
