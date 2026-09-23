# SC1: System Detail CVEs target and scan selection

**Status:** Domain decisions settled. UI implementation requires the existing Claude design; uncovered states are design blockers.
**Handoff revision:** 2, UI design authority correction.  
**Decision date:** 2026-09-23.  
**Inspected branch head:** `327d03b6d58055eb688fe657f12e223b8419f446`.  
**Source branch:** `TASK-326.2-scanning-cve-triage-parity`.  
**Parent design:** `systems-view-design-v0.1.md`, internal version 0.3, Sections 1.0, 10, 11.4, and 22.  
**Slice ID:** SC1. This is not a pre-existing Backlog task ID.

## 1. Outcome

An operator can open System Detail → CVEs and determine which running or explicitly selected configuration a scan describes. The page must show matching results, an honest no-scan state, or an honest unmapped/unavailable state. It must never substitute flake-head evidence for an unmapped running system.

The page may show the schema-1 scan of a uniquely mapped running derivation without complete deployment proof. That result remains read-only. This slice does not create new remediation authority.

Stop after this workflow is implemented, verified, and available for manual review. Do not implement the rest of the Systems audit.

## 2. Controlling decisions

The owner changed the earlier head-fallback requirement. The controlling default is now **Current**, with no automatic head substitution when running output is unmapped.

A local activation is a description of how a switch happened. It is not itself a reason to reject a known equivalent configuration. A known, fully proved target has the normal Current behavior. A uniquely mapped target with missing proof can show read-only scan feedback. Unmapped output has no Current scan in SC1.

The future local-agent Vulnix feature is noted but excluded. Captured-start POA&M edits and cross-revision continuity are also excluded from this implementation. Preserve their future requirement, but do not remove current writer protections to approximate them.

Only Section 22's agreed statements and this slice are implementation instructions. The original AS-BUILT sections and old tests are evidence of code, not permission to retain a superseded requirement. Other PROPOSED architecture sections are not a mandate for a general rewrite.

### 2.1 UI authority and the design gate

The owner's Claude design implementation is authoritative for the UI. It is not a style suggestion. Start with `docs/design/CrystalForge/components/SystemDetail.jsx`. Follow the shared triage components it uses, including `CvesView.jsx`, and the applicable styles. Use the current design source at the implementation base; do not assume that the production UI already matches it.

This contract defines backend facts and behavior. It does not approve new UI elements, placement, static copy, or interactions. Do not add a banner, badge, panel, card, field, column, button, tooltip, modal, tab, refresh indicator, or confirmation workflow merely to expose a new state or DTO field. Preserve the existing design's hierarchy, spacing, typography, colors, icons, controls, and interactions. Do not redesign the reference itself.

Before browser-visible edits, map each affected condition to the exact design component and state. The mapping must identify the source file/symbol, the corresponding data slot or existing interaction, and its acceptance case. An existing designed state can receive real data. A generic style/component in another location is not permission to create a new arrangement or behavior here.

If a required condition has no matching design state, report the file/component, condition, required fact or interaction, and affected acceptance case. Mark that case **Blocked: design gap** for the owner's Claude design workflow. Do not implement a temporary design or remove a required truth or safety condition. Continue independent backend work and already-designed cases. Never report the whole slice complete with a design-blocked case.

Terms below such as read-only, unmapped, unavailable, and no scan are semantic states, not instructions to create named badges or new notices. Presentation and copy must come from the Claude design. Inspecting existing production banners does not make them approved design elements. Exact IDs needed for verification can be inspected through API responses and test records when the design has no visible ID field.

## 3. Scope

### Included

Current selection, server-side target mapping for the selected read, scan-source selection, the read-only mapped-running tier, truthful empty/error semantics, source metadata, exact revision navigation, refresh behavior, page/source validation, and targeted regressions. Browser presentation is limited to the corresponding existing Claude design states under Section 2.1.

Implement the Claude design's package-first tab, revision bar, package summaries, CVE/Severity/CVSS/Fix/Triage/action columns, advisory links, and shared triage dialog. Match its structure and interactions; do not preserve a production deviation solely because it already exists. Do not add the fleet filter bar or a new dashboard. Do not expand this slice into unrelated visual repairs.

Small shared-helper changes are permitted when necessary. Prove the affected Hardening selector behavior and preserve its separate evidence/action rules. Do not redesign Hardening inventory or scan admission.

### Excluded

Local agent scanning; new scanner commands or protocol; scanner toggles; new scan scheduling; proof-repair migrations or invented deployment records; POA&M continuity/reconciliation; automatic readiness or closure; changing mutation eligibility; fleet CVE redesign; broad Compliance fixes; header/count-source consolidation; Scanning's false-clean fix; unrestricted polling or a new event bus; unrelated design or CI repair.

The existing header and Scanning summary inconsistencies remain tracked for SC2. Do not report cross-screen consistency as complete after SC1.

## 4. Target intent, source, and capability

The implementation must distinguish these facts. The field names below are logical responsibilities, not mandatory new API names.

| Fact | Required meaning |
|---|---|
| Target intent | Current, exact retained generation, or exact derivation. Presentation mode does not replace this intent. |
| Latest observation | The latest report selected with a deterministic timestamp/ID order, before validity predicates. |
| Running mapping | Known unique target, unmapped, ambiguous, no usable report, or invalid/conflicting data. |
| Resolved target | Server-owned derivation and its registered flake/configuration; full commit and actual generation only when established. |
| Selected source | One completed eligible scan, its UUID, completion time, scanner name/version, and evidence representation. |
| Proof | Existing strict Current remediation proof, or its missing/invalid prerequisite. |
| Read capability | Whether this exact source can be displayed to this actor. |
| Mutation capability | Existing strict domain and role checks. The new read tier never grants it. |

A source-less result must not use zero as the only indication of missing evidence. A report with no generation must not invent generation 0. A generation/store mismatch must not claim that the reported generation belongs to the selected output.

### 4.1 Current with full proof

Preserve the normal Current inventory path and existing mutation behavior. Use the newest completed schema-1 scan for the resolved exact target. Order eligible scans by completion time and scan ID. Do not choose the newest scan across the system's other derivations.

A known local activation that has the required proof follows the same path. Do not add an origin-based mutation restriction.

### 4.2 Current with a uniquely mapped target but missing proof

Resolve the reported running output from authoritative server records. Scope the mapping to the system's registered flake and effective configuration. Prove that one eligible target identity exists; do not pick the first result. A candidate menu truncated to 1,000 entries cannot establish this uniqueness.

Use the exact target's newest completed schema-1 scan. Reuse its immutable observations and existing inventory pagination/metadata calculations. Return read-only state and the proof reason. Do not call the result fully authoritative Current, historical solely because it is read-only, or remediated.

Do not create remediation observation references for the new tier. Keep writes unavailable. Use the design's existing non-actionable state; do not choose a new hide/disable pattern, explanation control, or notice. If the design lacks that state, report a design gap. Direct calls to existing mutation endpoints must still fail when their proof requirements fail.

No schema-0 Current fallback is added by this slice. Existing explicit schema-0 browsing remains unchanged. Do not turn an unsupported newer result into an eligible completed source by relabelling its format.

Do not hide contradictory identity records by treating all failures as merely missing proof. A conflict that prevents a unique output-to-target mapping must show unavailable/conflict, not an arbitrary read-only scan.

### 4.3 Unmapped, no report, ambiguous, and no scan

These are different states:

| Condition | Required result |
|---|---|
| Reported output has no known scoped mapping | Unmapped running configuration; no Current inventory. No head fallback or automatic agent scan. |
| No usable latest report | No usable running state reported; no Current source. |
| Multiple eligible mappings cannot be reconciled to one target | Ambiguous mapping; no arbitrary source. |
| Identity/source read fails | Read failure remains distinct from unmapped or clean. Use the designed error/retry state; a missing interaction is a design gap. |
| Known target has no completed schema-1 scan | No eligible completed scan for this target. Keep target identity visible. |
| Known target has a failed/queued newer attempt and earlier completed evidence | Preserve the earlier eligible same-target evidence. Do not borrow another target's scan. |
| Selected completed scan has zero eligible findings | No findings in this selected scan, with source and completion time. This is not a guarantee of no vulnerabilities. |

All existing design elements that describe the same state must agree. This does not require a banner plus an empty panel, or authorize either element when absent from the design. Do not tell the operator that running another scan creates missing retained-generation proof.

### 4.4 Explicit revision browsing

Existing commit and generation controls remain available. An explicit target is authorized again on the server. Requests must not trust a browser-supplied store path or commit hash as proof.

An evaluated but undeployed commit is inspectable only as its own target. Selecting it does not change the running header or permit Current-only mutations. If its derivation or scan is absent, explain that fact. Do not substitute an older successful target.

Label source relationship from known evidence. Do not call every non-current target historical; it can be undeployed. Do not derive relationship from array position or timestamp alone.

## 5. Refresh, navigation, and drafts

Refresh must re-resolve Current and reload relevant candidate metadata. It must show a newly reported deployment and newly evaluated choices when those reads succeed. A newer evaluated commit alone does not replace the running target.

An explicit target must survive tab switching, reload, and back/forward navigation. Serialize stable selectors through the existing navigation pattern. Preserve unrelated Config/POA&M query parameters. Do not make a display-mode toggle silently freeze Current as a commit or silently reset an explicit target.

A target, source, authority tier, or inventory-revision change invalidates incompatible pages and revision-local state. Validate the response body as well as the request epoch. A request tagged as Current does not prove that the body belongs to the intended system and newly resolved output.

Do not require a successful historical menu fetch before displaying an independently successful authoritative Current read. Show menu errors locally. Do not infer source authority from a candidate response merely because the source request failed.

The required refresh behavior uses browser refresh, page navigation, and refresh interactions already supplied by the design. Do not add a Refresh button, new-head notification, auto-follow toggle, or independent refresh UI. Reuse existing polling only where it already exists. Do not add a page-wide timer framework.

While a triage/POA&M draft is mounted, do not silently retarget or discard it on an in-app refresh. Preserve the original baseline/context and defer replacement of action-bound component state where necessary. Use existing designed conflict behavior. Do not add a refresh-confirmation dialog or a new editing workflow. If the existing design cannot represent the required conflict, report a design gap. SC1 must retain current server conflict checks; cross-generation save support comes later. A hard browser reload follows normal existing draft persistence behavior; this slice does not add an offline draft store.

## 6. Security and compatibility invariants

Authorize the system before exposing source identity or cursor diagnostics. Keep hidden and absent resource behavior non-enumerating. Recheck exact target membership under the established scoped read transaction.

A non-null Current candidate derivation ID is not proof: the current query uses separate left joins for derivation and scoped commit. A failed flake-scoped commit join can leave the derivation ID populated. New mapping logic must not inherit this defect.

Do not emit the existing fully authoritative Exact/ExactCurrentScan semantics for a weaker result. Use explicit additive read metadata or a safely versioned state. Test the supported old/new client behavior. Older clients may show a conservative unavailable/read-only state, but must not acquire write controls or report the fallback as fully proved Current.

Preserve the strict writer predicates. GET must not run Nix, enqueue scans, persist a deployment, insert a retained generation, create a POA&M, or change a disposition. Update SQLx metadata only with a verified task-owned database if checked query shapes change.

## 7. Starting source map

Read these modules and relevant nearby tests. Use symbol search and small contiguous ranges, not whole-repository dumps.

| Path / symbols | Reason |
|---|---|
| `packages/web-ui/src/views/system_detail.rs`: `revision_scope_default`, `RevisionScopeSelectionState`, `RevisionScopeBar`, inventory resource and refresh callbacks | Defaults, menu mapping, navigation, source installation. |
| `packages/web-ui/src/components/cve/mod.rs`: `CvesTab`, `CveInventoryPaginationState`, `inventory_allows_exact_remediation` | Read-only presentation, empty states, pagination, actions. |
| `packages/default/crates/cf-server/src/queries/cves.rs`: `fetch_system_cve_inventory_tx`, `fetch_historical_system_cve_inventory_tx`, `fetch_inventory_page_for_source`, candidate query | Actual target and source authority. |
| `packages/default/crates/cf-server/src/queries/systems.rs`: observational current-revision resolver | Existing scoped mapping concepts. A commit-only result is not sufficient to select an exact scan. |
| `packages/default/crates/cf-server/src/handlers/api/systems.rs`, CVE inventory handlers near lines 1750–1880 | Read-only results omit remediation hydration; response assembly. |
| Server and Web UI API model modules; `packages/web-ui/src/api/client.rs` | Compatible target/proof/source DTOs and query serialization. |
| `docs/design/CrystalForge/components/SystemDetail.jsx`: revision bar and CVE tab, plus referenced shared components/styles | Authoritative UI structure and interactions. Replace mock data with server facts; do not invent additional UI. |
| `checks/web-ui/tests/integration-test.js`: `12ha-system-detail-cve-inventory-fallbacks`, `12h-system-detail-cves-grouped-justification` | Existing browser fixtures and assertion targets. |

Source facts were checked at `327d03b6`. Re-read changed ranges when the branch advances. The full document's S32–S38 entries give pinned references. Existing line numbers are navigation hints, not permission to skip the implementation.

## 8. Acceptance matrix

Every case needs exact identity checks through API/test evidence and visible-state assertions against the Claude design. A backend field need not become a new visible field. Test source existence is not a passing result. A missing design state must be reported as blocked rather than filled with invented UI.

| ID | Scenario | Required proof |
|---|---|---|
| SC1-01 | Running A, newer evaluated/scanned head B | Current selects A and A's scan. No default head substitution. |
| SC1-02 | Known local activation A | Current identifies A. Full proof uses existing behavior; missing proof uses the read-only path. |
| SC1-03 | Unique running derivation A, completed schema-1 scan, no retained binding | Rows/source shown read-only; proof reason visible; direct mutation calls still rejected. |
| SC1-04 | Unmapped running output, fully scanned head B exists | Unmapped with no Current scan. No B inventory, auto-scan, or invented proof. Explicit B browsing still works. |
| SC1-05 | No report, ambiguous mapping, mapping outside registered scope, latest unusable report | Distinct honest failures. No previous-report, foreign-flake, or arbitrary-target substitution. |
| SC1-06 | A has no eligible scan; older revision/head has one | A remains selected with no scan. Historical/explicit targets remain independently browsable. |
| SC1-07 | A has completed clean evidence; then failed/queued rescan | Clean belongs to the completed source. Later attempt does not erase it or become the evidence. Test nonempty source too. |
| SC1-08 | Missing proof with >1 page of A findings | Pages remain same source/read tier; no triage hydration; continuation failure preserves loaded rows; source change restarts. |
| SC1-09 | Refresh after B activation versus B evaluation only | Activation changes Current; evaluation only updates choices. In-flight A data cannot overwrite B. |
| SC1-10 | Select an exact old generation/commit; reload, back/forward, switch modes/tabs | Exact selection persists. Labels and source remain matched. Returning to Current follows running state. |
| SC1-11 | Stagger reads; select another system/target before completion | No wrong-system/target data, no false empty/unmapped conclusion, and no overwritten explicit selection. Reject mismatched bodies. |
| SC1-12 | Read-only/missing-proof source returns 0 findings; Unknown-only source has findings | Source-less is not clean. Valid empty is source-labelled. Unknown-only is not clean. |
| SC1-13 | Viewer/Operator/Admin; hidden system and foreign target; older supported client | Read scope is preserved. New tier grants no write authority. Hidden access and cursor errors do not leak source identity. |
| SC1-14 | Existing refresh behavior while a draft is open | No silent target/baseline replacement or draft loss. Existing server stale-write checks remain. No invented confirmation dialog. Record that cross-revision save is deferred. |
| SC1-15 | All affected visible states, including read-only, unmapped, unavailable, and error | Each maps to a named Claude design component/state. No invented UI or reference edits. Missing design states are reported and block their UI acceptance. |

Browser coverage must include actual API-produced data for SC1-03 and SC1-04, not only unrelated hard-coded responses. Use an isolated fixture database. Network stubs are useful for races and failures, but are not proof that SQL selected the correct source.

Compare the implementation with the authoritative Claude design at matching wide/narrow widths and light/dark themes, using equivalent fixture data. Record missing sections, reordered sections, merged concepts, missing metadata, changed interactions, and hierarchy differences. Added UI must also be reported. Do not edit the design, its fixtures, or visual baselines to accept an invented presentation. Metadata absent from the design is an input to the design-gap workflow, not permission to add a field.

## 9. Verification and handoff

Use the repository's Nix environment, task lifecycle, and live-preview instructions. Reuse only a verified task-owned preview database. Do not touch the user's persistent database or another worktree's processes.

Run focused unit and database tests, WASM compilation, the affected browser workflows, and documentation checks for the changed public contract. If a shared helper affects Hardening, include its focused regression. Preserve exit statuses and test counts. A filter that executes zero tests is not verification.

A shared design-target prerequisite blocked older browser attempts. Check the current condition. If it blocks the selected workflows, report them as not executed; do not treat an old waiver as a new waiver or repair unrelated Config Explorer work in SC1.

The handoff must include the verified preview URL and data mode, exact commit, task and worktree, changed files, acceptance matrix with evidence, test commands/results, manual scenario locations, documentation changes, and unresolved blockers. Stop for owner validation. Do not merge or force-push.
