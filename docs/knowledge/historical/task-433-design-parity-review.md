---
type: Historical Reference
title: "TASK-433 Design Parity Review"
description: "Records the TASK-433 comparison of the c2f5db08..ae20da81 design delta with the production Web UI: per-surface retained behavior, reviewed production differences, and which design files are product behavior versus demo-only."
tags:
  - crystal-forge
  - design-parity
  - web-ui
  - compliance
  - poam
implementation_status: historical
status: deprecated
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/task-433-design-parity-review.md at commit 3b23d36f"
    title: "TASK-433 Design Parity Review"
---
# TASK-433 Design Parity Review

> **Status:** historical. This is a point-in-time review record dated 2026-09-01 for TASK-433. It says the final SHA and the visual CI artifact URL are pending and that visual acceptance is not closed. The record is kept for its reasoning about which design behavior is product behavior and which is demo-only. It was not re-compared with the current code.

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
| POA&M detail and lifecycle | `PoamViews.jsx` | Status, risk, owner, target, progress, findings, remediation plan, milestones, activity, verification, close, reopen, and exact evidence navigation remain present. | Production adds optimistic revisions, loading and authorization states, verification history, assignment references, and explicit save actions. Metadata uses `Save metadata`; the separate remediation text uses adjacent `Save plan`. Persistence is explicit at each section, and closing without a save action does not claim that the local draft persisted. Title, risk, assignee, and target date are edited in place in the design's header and metadata band. There is no trailing metadata section. `Save metadata` appears beside the band only while a draft differs from the saved POA&M, and it sends only title, assignee, target date, and risk. `Save plan` stays beside Remediation plan and sends only the plan. A viewer sees plain text and no save action. Evidence on a policy finding in the POA&M register opens that finding's System Detail Compliance tab and then runs the same exact-evidence flow as an Evidence click inside that tab: one exact visible bundle revision opens the evidence drawer focused on the linked policy, several open the exact-context picker, and none open an explicit unavailable state. The register passes the finding's identity to System Detail through a one-shot in-memory handoff, not the URL, because the destination cannot rebuild a finding that sits on a later page of the POA&M detail response. System Detail takes the handoff once, ignores it for any other system, and clears it when the view closes. The handoff never selects a bundle revision: the destination intersects the finding's linked contexts with the bundles it loads for the current user. Verify now and Authoritative close appear only while the POA&M awaits verification, the only state in which the service accepts verification. Section order follows the design: Remediation status, Deficiency, Vulnerability scope, Remediation plan, Milestones, Activity. Baseline assignment references sit inside Deficiency, and retired exact-CVE history is a disclosure under Vulnerability scope that opens by default only when no link is active. Vulnerability scope groups active links into one card per canonical CVE and package, with compact host rows beneath. A host row shows the hostname, the observed package version, a short exact-scan reference (full UUID in its title), the resolution-state chip, and the Evidence and unlink actions. The card header shows the CVE, the package, and the host count. It omits the design fixture's severity, CVSS, fixed-version, environment, and commit fields: the POA&M detail response carries none of them, and the drawer does not infer them from the resolution state, the installed version, an environment ID, or a scan ID. Existing milestones toggle completion through a label that wraps the checkbox and title, as in the design, and have no inline title or date editor. Activity shows date, actor, and the human message only; the stored payload still builds that message but is not rendered. |
| System POA&M | `SystemDetail.jsx`, `PoamViews.jsx` | System-scoped counts, filters, rows, and finding navigation remain visible in System Compliance. | Production counts and rows come from bounded server rollups rather than filtering the in-memory POA&M array. |
| Bundle POA&M | `ComplianceView.jsx`, `PoamViews.jsx` | Open findings, On POA&M, No POA&M, Overdue, Awaiting verification, Closed, and list navigation retain the design hierarchy. | Production batches visible bundle IDs and uses committed server rollups. It does not issue one POA&M query per bundle row. |
| Dashboard | `DashboardView.jsx`, `data-dashboard.js` | POA&M Summary and Watchlist retain status, urgency, owner, due date, and detail navigation. | Production persists widget layout and loads authorization-scoped summaries. It does not use local storage or mutable fixture arrays as domain authority. |
| Notifications | `Shell.jsx` | Overdue and awaiting-verification notifications remain available from the top bar and navigate to the exact POA&M. | Production uses durable deduplicated notification events, keyboard menu semantics, and Dioxus routing instead of delayed global events. |
| Setup Coach | `SetupCoach.jsx`, `CoachTours.jsx` | The nine-step Setup track stays server-derived and independent from five role-adaptive Security Workflows modules. Walkthrough progress records viewed stops only. | Production derives Setup completion from the Administrator-only progress API, stores walkthrough presentation state in this browser, and uses typed routes plus `data-coach-target`/`data-coach-open` attributes. Bounded target polling follows rendered layout; it does not use mock global navigation or mutate security records. |
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

## Related concepts

- [Compliance view design review](../ui/compliance-view-design.md): the audit of the production `/compliance` route that cites this report.
- [Compliance UI redesign spec](../ui/compliance-ui-redesign-spec.md): the earlier compliance view redesign specification.
