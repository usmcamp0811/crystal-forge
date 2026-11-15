---
id: TASK-521
title: CVEs view multi-select filters and design export formats
status: In Progress
assignee:
  - '@automated-agent'
created_date: '2026-11-11T22:40:00Z'
updated_date: '2026-11-11T22:40:00Z'
labels:
  - cve
  - web-ui
  - api
dependencies: []
references:
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/styles.css
  - packages/web-ui/src/views/cves.rs
  - packages/default/crates/cf-server/src/handlers/api/cves.rs
  - packages/default/crates/cf-server/src/queries/cves.rs
  - https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/342
priority: high
type: feature
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: design commit bc30ef57 ("update cves design view") changes the CVEs
filter bar. Severity, fix status, and triage status become multi-select
toggle groups (`MultiSeg`). Environment becomes a multi-select `MultiFilter`.
The implementation accepts one value per filter in the Web UI and in the
server API. The export menu offers only CSV, and CSV is disabled when an
environment filter is active. The design export menu offers OSCAL JSON,
Excel, CSV, and OSCAL XML for the current filtered list and environment scope.

Goal: implement the bc30ef57 filter design and a correct, scope-aware export.

Owner decisions (2026-11-11): the API accepts multiple values as
comma-separated lists on the existing parameters. Export covers all four
design formats.

Owner decision (2026-11-12, MR !342 review): OSCAL exports are standalone and schema-valid. They are valid against the official OSCAL 1.1.2 JSON schema and XSD; `import-ap` resolves to a back-matter resource that states no assessment plan exists. NIST oscal-cli import-chain validation is knowingly not satisfied.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Severity, fix status, and triage status filters are multi-select toggle groups with an "All" reset, matching bc30ef57 (check icon and per-value color on selected options).
- [x] #2 Environment filter is a multi-select MultiFilter; findings, groups, and members are restricted to the union of the selected environments, intersected with the caller's authorized scope.
- [x] #3 CVE list, pair, group, member, and export endpoints accept comma-separated `severity`, `fix_status`, `triage_status`, and `environment_id` values; a single value keeps its existing meaning; values in one filter are OR-combined and filters are AND-combined.
- [x] #4 Filter selections round-trip through the page URL and browser history.
- [x] #5 The export menu offers OSCAL JSON, Excel, CSV, and OSCAL XML; each export contains exactly the server-filtered rows of the current view, including environment scope, and the button count equals the exported row count.
- [x] #6 Export is no longer disabled when environments are selected; the menu title shows the selected environment scope.
- [x] #7 The existing export row limit still rejects oversized exports with 422 for every format.
<!-- AC:END -->

## Non-goals

- No change to triage, POA&M, or scan behavior.
- No schema migration.
- No change to fleet KPI cards (they remain fleet-wide).

## Constraints

- The server owns filtering, scope intersection, and export generation.
- An unauthorized environment ID yields an empty scope, never all scope.

## Verification plan

- cf-server unit tests for filter parsing and export rendering.
- cf-server database tests for multi-value filters and scoped export (isolated test DB).
- web-ui unit tests and `cargo check --target wasm32-unknown-unknown`.
- Live preview check of filters and each export format.

## Impact and risk

- Impact: CVE API query parameters (backward compatible), CVEs view.
- Risk: medium (shared SQL used by several CVE endpoints).

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: automated-agent on vm-agent in /home/mcamp/code/crystal-forge/TASK-521-cve-multiselect-export. Branch TASK-521-cve-multiselect-export from dev bc30ef57. Task created and started on explicit owner instruction (2026-11-11). dev worktree has an untracked session-ses_f927.md and untracked backlog locks; untouched.

2026-11-11 implementation (uncommitted). Server: comma-list `severity`/`fix_status`/`triage_status`/`environment_id` on list, grouped, pairs, groups, members, export; environment list intersected with scope inside the inventory CTE (`scoped_list_cte`). Export `format=csv|excel|oscal-json|oscal-xml` via new `services/cve_export.rs` (existing rust_xlsxwriter, quick-xml; no new dependency). Web UI: `MultiSeg` toggles, environment `MultiFilter`, URL round-trip, 4-format export menu with scope title; export now fetches and saves a blob so a 422 shows a toast instead of navigating away (previous `set_href` behavior).

Verified locally (exit 0): cf-server `cargo check --tests` (SQLX_OFFLINE); cf-server lib tests `cve_export projection_page_bounds handlers::api::cves` 25 passed; web-ui `cargo check --target wasm32-unknown-unknown`; web-ui `cargo test views::cves` 37 passed; against a disposable task-owned PostgreSQL 17 (127.0.0.1:55491, `.devenv/task521-pg`): `--test cve_inventory_projection -- --ignored` 2 passed, `--test poam_workflows -- cve_ --include-ignored` 41 passed, then `cve_export` 2 passed after adding format/422/400 assertions. `nix build .#okf-knowledge` and integration owners not yet run.

Preview: devenv `devenv up` fails at shell evaluation on vm-agent (only a Nix "restricted setting 'system'" warning is printed); fell back to `run-ui-dev` in tmux session `cf-task521`.

Preview check found and fixed a defect: client-side `matches_pair_filters` compared the comma list as one value, so Package/None groupings rendered empty under multi-select. Now OR-combines per filter; regression test added (web-ui `views::cves` 38 passed). `nix build .#okf-knowledge` exit 0.

Preview verification (run-ui-dev, fixture + 6 seeded `preview-cve-*` hosts with exact scans in the worktree-owned DB on :3042): API `severity=critical` 7, `high` 8, `critical,high` 15; env dev 20, prod 20, dev+prod 30. Export with `severity=critical,high&environment_id=dev,prod`: csv/excel/oscal-json/oscal-xml all 200 with 15 rows/findings = button count "Export 15"; OSCAL title "— dev, prod"; `format=pdf` and malformed UUID 400. Headless Playwright: deep link restores toggles and env filter; menu "Export CVEs · dev, prod" lists 4 formats; all 4 downloads saved; toggling Medium updates URL and count (27); Package, None, Environment groupings render rows; no page errors.

Not run: integration owners (`integration-assurance`), `nix flake check`, MR screenshots. `cf-server-core` package build on the remote builder failed once with 15 unrelated tests timing out on "Nix parser did not finish within 2 seconds" (deployment_policies, compliance_interchange); a rebuild then succeeded. Not committed.

2026-11-11: committed 797bd5c1 and pushed branch TASK-521-cve-multiselect-export (vm-agent cannot reach gitlab.com:22; pushed via altssh.gitlab.com:443). Draft MR !342 (https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/342) opened with GitLab push options targeting dev. Task stays In Progress until owning integration results (integration-assurance) and MR screenshots exist.

2026-11-12 MR !342 review (changes requested at 360cdc4d) response:
1. P1 invalid environment IDs: UI now rejects a mixed or all-invalid `environment_id` list as a whole, keeps the raw value in the URL, shows an alert, hides findings, and disables export; regression test `malformed_environment_ids_never_widen_to_all_environments`.
2. P1 OSCAL import-ap: resolves to a back-matter resource stating no assessment plan exists (owner decision above). `reviewed-controls` added (required); selects NIST SP 800-53 `ra-5` only, because the XSD requires include-all or explicit controls.
3. P2 blob download: anchor attached to the document for the click; object URL revoked after 60 s.
4. P2 OSCAL semantics/validation: target status is always `not-satisfied` (accepted risk no longer maps to `satisfied`); titles/props single-line; JSON and XML render one document. Empty/single/multi samples (`CF_OSCAL_SAMPLE_DIR`) validate with check-jsonschema 0.37.2 (official JSON schema) and xmllint 2.15.4 (official XSD) from the NIST v1.1.2 release. The first XSD run caught the missing include-control, now fixed. NIST oscal-cli 3.2.0 rejects all six: "backmatter resource ... does not have an rlink entry" (import chain; accepted per owner decision).
5. P2 verification: poam_workflows Viewer/Operator/Admin matrix over pairs, groups, members, and all export formats with environment lists that mix visible and hidden environments, hidden-only lists, combined filter groups, and no disclosure of hidden environment names. The test router now mounts the three inventory routes. Integration-assurance `cve.cjs` was extended with multi-select, four browser downloads checked against the button count, and the invalid-link alert.

Verified locally (exit 0): cf-server lib 29 passed; cve_inventory_projection 2 passed; poam_workflows `cve_` 41 passed (disposable PG 127.0.0.1:55491); web-ui `views::cves` 39 passed; wasm32 check with no errors; okf-knowledge. Preview browser runs in Playwright Chromium and Firefox: five consecutive downloads (oscal-json, excel, csv, oscal-xml, csv) with no failure, 15 rows/findings = API total; invalid links show an alert, 0 rows, and disabled export; no page errors. Dark/light screenshots of the implementation and design captured locally.

Not verified locally: integration-assurance. Farm builds failed twice on infrastructure faults (segfault/abort, stale file handle, unrealisable paths). A local KVM run was killed by agent-session interruptions. Exact-head MR CI is the evidence source.

2026-11-12 design refinement 18404419 (owner: "mostly styling; export may need backend tweaks"):
- Filter bar: Severity/Fix/Triage/Environment are MultiFilter dropdowns (option color dots; checkbox `value` = API value); Clear button; Group/Sort are selects; package text filter removed (package stays a URL filter and is listed in the export description); MultiSeg and its CSS removed. Selection strip restyled with "N CVEs · select".
- Export: button has no count; menu asks "What to export": Selected (pairs selected in Package/None view), Current view (named All CVEs when unfiltered), All CVEs (offered only while filtering), each with its count; formats disabled at 0 rows.
- Backend: `POST /cves/export?format=` exports exact pairs (Viewer+, CSRF, scope-limited, duplicates once, empty 400, >1,000 pairs 422). Unused `fetch_cve_package_names` client call removed.
- Tests: poam_workflows selected-export cases (visible-only, CSRF 403, empty 400, oversized 422, OSCAL count); web-ui scope/description tests; assurance cve.cjs and web-ui-test 16-cves updated for the new controls. 16b-cves-severity-filter was already stale (looks for a "Flat" button) and is unchanged.

Verification (owner instruction 2026-11-12: remote builders only, no local builds): poam_workflows `cve_` 41 + projection 2 passed locally against disposable PG before that instruction. Farm builds: `.#web-ui` exit 0, 856 web-ui tests passed (41 CVE view); `.#server` ran cf-server lib tests: 1568 passed, 1 failed `handlers::api::builders::tests::niks3_basic_probe_applies_guard_to_path_info_only` (`Configuration` error; file unchanged on this branch), all CVE export/handler tests passed. okf-knowledge exit 0. Not executed: web-ui-test 16-cves and integration-assurance (need a local preview/VM); preview not refreshed for this revision.

Design note: the design's triage MultiSeg omits `inventory_only`; the implementation keeps it because the domain status exists.

2026-11-13 production HTTP 502 on "Unable to load CVE groups" (owner report, reckless):
- Owner ran EXPLAIN ANALYZE of the logged groups statement (LAN, host grouping): custom plan 272 s, generic plan 9.3 s. In the custom plan `visible_members` was estimated at 1 row, so `view_system_deployment_status` was re-run for each of 21,044 membership rows (420,880 inner lookups, 83.6 M commit probes). The generic plan rejected 40 M join rows twice (pairs x memberships nested loops).
- Owner-run fix test: materialized deployment-status CTE gave custom 1.34 s; generic stayed 9.8 s (COALESCE-hashable join did not change the generic plan, so it was dropped).
- Implemented: `deployment_status_once AS MATERIALIZED` in the projection CTE; `visible_members` carries pair severity/exploited/fix status, and `grouped` no longer re-joins `filtered_pairs`; projection transactions set `plan_cache_mode = force_custom_plan` (`begin_projection_read`); groups and members return total and page from one statement (one-row total LEFT JOIN LATERAL page, keeps the total past the end); Web UI requests the unfiltered "All CVEs" export total only after the export menu first opens. The pairs request is kept in Environment/Host views: it supplies recently-observed highlights, the export count, and nav-badge acknowledgment. Projection DB test extended with past-end group/member pages (not executed: no local DB runs per owner instruction).
- Verification: farm `.#server` exit 0 (cf-server lib 1,569 passed; embedded web-ui 856 passed). An earlier attempt failed 17 unrelated lib tests on builder timeouts (Nix parser 2 s limit, Nix runtime probe) and passed on rerun. Not executed: DB integration tests, preview, integration-assurance, a production re-measure of the members query.
- Owner scope addition: System Detail CVE tab SELECT chips (High/Patchable/Outstanding), row selection, and batch triage per design; follow-up commit on this MR.

2026-11-13 System Detail batch triage (owner decisions: batch applies to the specific system only; local preview/tests allowed):
- Server: `POST /api/v1/systems/:id/cves/batch-triage` (Operator+, CSRF) writes only `cve_system_dispositions` for the path host, atomically. Pairs must be current exact findings on the host (else 409); host-scheduled or POA&M-linked pairs (incl. inherited environment schedules) are skipped; accepted skipped unless `skip_existing=false`. Locks all CVE keys sorted, then system sentinel, policy keys, finding keys, system row; actor reload and visibility recheck; hidden system 404. The fleet batch POA&M grouping block was extracted into `create_batch_schedule_poams_tx` and is shared.
- `cve-inventory-page` exact mutable rows now carry `triage_state` (host override, else coherent environment default) so the Outstanding chip needs no per-row hydration.
- Web UI: SELECT strip (Critical/High/Patchable/Outstanding, non-empty only), Ctrl/Cmd-click rows and package headers, "N selected" package chip, bulk bar, `SystemCveBatchTriageDialog` (accept risk or schedule patch, One/PerPackage grouping). The design's "All of <env>" scope toggle is omitted per the host-only decision.
- Verification: local disposable PG (127.0.0.1:55491): `system_batch_*` 2 passed (host-only writes, skips, 409, Viewer 403, missing CSRF 403, hidden 404, inventory `triage_state`); all 23 fleet `batch_*` tests passed after the refactor; projection 2 passed (incl. past-end pages). Farm `.#server` exit 0 (lib 1,569 passed; an earlier preview build of the core package failed 17 unrelated tests on farm Nix-parser timeouts); farm `.#web-ui` exit 0 (857 passed); okf-knowledge exit 0. Preview (local cargo-built server, fixture DB) Playwright Chromium: chips, Ctrl-click selection, dialog, host accept of 2 pairs, reload shows Accepted and Outstanding 20→18, no page errors. Assurance `triage.cjs` extended (non-mutating selection + `triage_state`), syntax-checked only; integration-assurance not run.
<!-- SECTION:NOTES:END -->
