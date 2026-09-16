---
id: TASK-440
title: Bring system configuration and flake exploration into full design parity
status: In Progress
assignee:
  - '@openai-agent'
created_date: '2026-08-28 03:43'
updated_date: '2026-09-16 04:32'
labels:
  - design-parity
  - web-ui
  - backend
  - systems
  - flakes
dependencies: []
references:
  - git commit eb5a18513623890e9dac1e8a74565078243288a8
  - git parent cfae4f4a33815c72059d309a547672f8c9039747
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - origin/TASK-433-policy-poam-workflows
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
  - git commit cafc678ef
  - git commit 1b9a0594193a26da99b0935151b64323acf8f913
  - git commit 4e09d60a
  - git commit f4dbfad6
  - git commit 4ddf19c6
  - git commit 046f46f14797aef5741fe7b27843db64a6133f76
  - TASK-454
  - git commit a41d41e8
  - git commit 1ccee7cf6aa59c3dc66f80ba02ed0817d2c0c9ba
  - git commit 4ad7490e881e2a457b2fb6be95758392fd7af45a
  - TASK-461
  - git commit 3a95b2b973ca9050c1d28dffd47357ae7a109bfa
  - git commit d6a122eaa0b0c700b963ebfe342a544d0c8d290c
  - git commit a15392835a72decd7592c7d59c6ea78ba251bcac
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2833655396'
  - git commit 24b9979f18e0e460d4c9efa9a964269b9c8449a2
  - git commit ae0ef8b4507caa10ff7ae4bb09e76d73b8985b70
  - git commit c07a239f510f554555c8c89195108857be6c9d56
  - git commit 24bd1d0e9950c918e698773efa936c2e2326f6c7
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2834497659'
  - git commit df58dbd6b408cfa1d718dd766690f3834201f033
  - git commit 41a4904943fd43e1c95c5cd40b8be0f7b1cd3141
  - git commit b0018e497dd4a30ddb334b77bd2d74bf5d056763
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2835583294'
  - git commit 1e986cd2351652e866f9b72bff956a312fc0827a
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2835889586'
  - git commit 88d3218d4b194f0a6c5253f760327cb5fae144fb
  - git commit 27f93478837b9b726c5cda807faf6182fb0d3229
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2840805787'
  - git commit 0aa38dba
  - git commit 7e1f0846
  - git commit 27cd67aa0705a011a3b67f8727dfc27d62dac4da
  - git commit 31819e2f
  - git commit 7f923d5535ffde3ded45649fb42ee0b75b7d453a
  - git commit 481ae958
documentation:
  - docs/design/CrystalForge/app.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/components/AddSystemModal.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/data-config.js
  - docs/design/CrystalForge/data-flake-explorer.js
  - docs/design/CrystalForge/data-flakes.js
  - docs/design/CrystalForge/styles.css
modified_files:
  - checks/evaluator-snapshot-isolation/default.nix
  - packages/default/crates/cf-protocol/src/source_artifact.rs
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/models/primary_evaluation.nix
priority: high
type: feature
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the complete Rust frontend and backend product behavior represented by design commit `eb5a18513623890e9dac1e8a74565078243288a8` (compared with parent `cfae4f4a33815c72059d309a547672f8c9039747`). The design commit is the authoritative visual and interaction reference; fixture randomness and the changed `.thumbnail` are not product requirements.

System Config scope:
- Replace the static rendered-module presentation with a revision-aware Evaluated options explorer.
- Support Generations and Commits modes, current/historical/never-deployed revision context, reload-safe deep links, and Back to current.
- Provide server-backed debounced search, All/Overridden/Changed filters with counts, bounded pagination, stale-response protection, loading/empty/error states, and a table aligned with the Modules/Evaluation/Drift cards without an unintended inner scroller.
- Show expandable option details with declared type, before/after change, complete winning and overridden definition provenance, source input/revision/path, and winner notes.
- Render scalar, package, structured list/attribute-set, submodule, opaque/function, and failed-evaluation values safely and according to declared type. Unknown or failed values must not be fabricated.
- Add real Modules and Evaluation summary cards plus the read-only module source tray. Tracked provenance can open the relevant flake/revision; untracked provenance remains visibly unavailable.
- Keep Drift behavior accurate and add the Overview store-path link into the current evaluation.

Evaluation lifecycle and comparison semantics:
- Config reads never launch Nix evaluation. Evaluation extraction occurs in the existing authorized evaluation/build job path and produces a reusable snapshot.
- When a requested never-deployed revision lacks a snapshot, Config shows an explicit unavailable state. An authorized explicit action may queue a new evaluation job or reuse an already queued/running job; the read request itself remains side-effect free. The UI distinguishes unavailable, queued, running, failed, and available states.
- In Generations mode, Changed compares the selected generation with the immediately preceding retained generation that has an evaluation snapshot. In Commits mode, Changed compares the selected commit with its Git first-parent snapshot for the same configuration. When no valid baseline snapshot exists, the UI says no comparison is available and does not report a zero-change result.
- Evaluation data required by a retained deployment generation remains queryable for as long as that generation is retained. An arbitrary commit snapshot remains queryable for as long as its flake timeline record is retained. Nix store garbage collection and branch rewrite/deletion must not silently remove retained snapshot metadata; genuinely missing or corrupt artifacts produce an explicit unavailable state.

Flake drawer scope:
- Add revision-scoped Commits, Systems, Modules, and Inputs tabs with counts and alert states.
- Make the selected commit govern every output pane and show revision identity plus host/module/input deltas against its Git first parent. Root commits and commits whose parent snapshot is unavailable show No previous revision rather than fabricated zero deltas.
- Reconcile declared configurations with managed systems, including managed, declared-but-unmanaged, and managed-but-undeclared states; include warnings for output collapse and systems pinned to older revisions.
- Let managed rows open that system's Config tab at the selected revision and unmanaged rows open system registration prefilled with configuration/hostname, flake, and branch.
- Show exported modules, declared options, consumer/blast-radius counts, and expandable declaration details.
- Show direct and resolved inputs, lock revisions, source, age, follows/transitive data, tracked/channel state, revision bumps, stale inputs, and multiple nixpkgs revision warnings.
- Derive all flake system totals consistently from the authoritative managed-system relationship across subtitles, lists, cards, rollout displays, and removal warnings.

Cross-surface and smaller parity scope:
- Make environment flake chips open the flake and restore the originating environment panel when the drawer closes.
- Preserve Config-to-flake provenance navigation and Flake-to-Config revision context.
- Deep-link pending deployment approval notifications to the exact system Deploy tab.
- Remove duplicate System Detail header Deploy/Rollback actions; route History rollback into Deploy with the exact previous generation selected; rename selectors to New commit and Previous generation.
- Add the `auto_latest` manual-deployment warning with Cancel, Continue on auto_latest, and Convert to manual and deploy outcomes, with truthful persisted/result state and idempotent retries.
- Remove only the duplicate inner Compliance bundle Edit action while retaining the authorized outer action.
- Ensure the file diff modal layers and receives input above the flake tray.
- Use full immutable commit SHA identity for persisted snapshots, API keys, caches, comparisons, navigation state, and row identity. Abbreviated SHAs are presentation only.

Backend, storage, and security outcome:
- Expose authorized, revision-specific, cached system evaluation data and revision-scoped flake output data required by these views.
- Evaluation data must come from the NixOS options tree rather than serializing `config`; retain declared types, safe values/errors, definition provenance, package metadata, module origins, evaluation identity/metrics, and comparison data.
- Search/filter/pagination and cross-revision diffs are computed server-side with bounded responses rather than transferring complete snapshots to the browser.
- Flake snapshots cover declared systems, exported modules and declarations/consumers, resolved lock inputs, and first-parent deltas without triggering per-host work while browsing.
- Snapshot persistence must provide content deduplication and bounded storage amplification equivalent to the design's content-addressed base-plus-delta intent. It must not store a separate complete multi-thousand-option corpus for every host and revision when that content is shared.
- Secret redaction occurs before unsafe data is persisted, indexed, diffed, logged, or serialized by an API. The boundary covers option values, nested/submodule values, package/collection elements, module defaults, evaluation errors, source metadata, and repository URLs containing credentials or tokens. Redacted data is neither returned nor searchable; masking only in Dioxus rendering is insufficient.
- Preserve environment visibility, existing mutation authorization, and deployed agent/builder compatibility. Hidden environments and unauthorized revisions use the application's existing non-disclosing authorization behavior.

Deep-link and interaction contract:
- The exact system identity, Config tab, revision mode, and full SHA survive hard reload and browser back/forward navigation. Unknown revisions and unavailable snapshots render explicit states; unauthorized or hidden targets do not disclose protected existence.
- A thorough UI/UX pass is part of this task. Every affected Rust surface matches the design example in the authoritative browser environment at 1920x1080 in light and dark themes, with narrow behavior exercised at 900x900. Keyboard assertions cover tab navigation, row expansion, revision controls, Escape behavior, modal focus trapping, and focus restoration.

The `modifiedFiles` metadata is anticipated and non-exhaustive. The implementation may modify the existing routing, modal, API, persistence, protocol, test, or migration files that actually own the required behavior, while remaining within this task's product scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The System Config tab defaults to the current deployed generation and supports generation, commit, reload-safe deep-linked revision, historical warning, never-deployed warning, unavailable local generation, and Back to current states using real revision-specific evaluation data.
- [x] #2 Config read endpoints never launch Nix evaluation. A missing never-deployed snapshot is explicitly unavailable until an authorized evaluation action queues or reuses a job, and the UI distinguishes unavailable, queued, running, failed, and available states.
- [x] #3 In Generations mode Changed compares against the immediately preceding retained generation with an evaluation snapshot; in Commits mode it compares against the selected commit's Git first-parent snapshot for the same configuration; a missing baseline displays no comparison available rather than zero changes.
- [x] #4 Evaluation artifacts required by retained deployment generations remain queryable for the generation lifetime, arbitrary commit snapshots remain queryable for the flake timeline-record lifetime, and store GC or branch rewrite/deletion does not silently erase retained snapshot metadata.
- [x] #5 Evaluated-options search is debounced and server-side across option path, safe rendered value, declared type, safe evaluation error, source path, and source input; redacted text is not searchable; filter counts remain revision-global while the result total reflects the active search and filter.
- [x] #6 Evaluated options provide All/Overridden/Changed filters, bounded pagination, correct range/button/count behavior, loading/empty/unavailable/unauthorized/error states, and protection against older asynchronous responses overwriting a newer request.
- [x] #7 Expandable option rows show declared type, the mode-defined comparison, complete winning and overridden definitions, source input/revision/path, and definition status without fabricating unknown or failed values.
- [x] #8 Scalar, package, structured collection, submodule, opaque/function, and failed-evaluation values and diffs render safely and type-appropriately, including package additions/removals and explicit not-evaluated errors.
- [x] #9 The Config Modules, Evaluation, Drift, and source-tray surfaces use real selected-revision data; tracked provenance opens the correct flake/revision, untracked provenance is non-navigable, and the Overview evaluation link opens the current revision.
- [x] #10 Secret-bearing option values, nested values, collection/package elements, module defaults, evaluation errors, source metadata, and credential-bearing repository URLs are redacted before persistence, indexing, diffing, logging, or API serialization and cannot be recovered through API responses or search.
- [x] #11 Snapshot persistence deduplicates shared evaluation content and bounds storage amplification so complete multi-thousand-option corpora are not independently duplicated for every host and revision where the content is shared.
- [x] #12 The flake drawer provides Commits, Systems, Modules, and Inputs tabs whose counts, alerts, contents, selected full revision identity, and Git first-parent deltas update consistently when the selected commit changes.
- [x] #13 A flake root commit or missing first-parent snapshot displays No previous revision and no fabricated delta; full SHAs key APIs, snapshots, caches, navigation, comparisons, and row identity, with a test proving two commits sharing a displayed SHA prefix remain distinct.
- [x] #14 The flake Systems pane correctly classifies managed, declared-but-unmanaged, and managed-but-undeclared systems; shows output-collapse and pinned-revision warnings; opens managed systems at Config for the exact selected revision; and prefills registration for unmanaged declarations.
- [x] #15 The flake Modules pane shows exported modules, descriptions, declaration counts, consumer/blast-radius counts, and expandable option path/type/default details from cached revision data.
- [x] #16 The flake Inputs pane shows direct and resolved counts, source and locked revision, update age, follows/transitive details, tracked/channel state, bumps, stale-over-90-days state, and multiple-nixpkgs-revision warnings.
- [x] #17 All flake system counts shown in page subtitles, table rows, cards, rollout denominators, drawer metrics, and removal warnings reconcile to one authoritative managed-system relationship.
- [x] #18 The exact system, Config tab, revision mode, and full SHA survive hard reload and browser back/forward; unknown revisions and unavailable snapshots render explicit states; unauthorized or hidden targets follow existing non-disclosing authorization behavior.
- [x] #19 Environment-to-flake navigation restores the originating environment panel on close; Config provenance opens the correct tracked flake/revision; Flake Systems opens Config at the selected revision; and unrelated later navigation does not retain stale context.
- [x] #20 Pending deployment approval notifications open the exact system on Deploy; System Detail has no duplicate header Deploy/Rollback controls; History rollback opens Deploy with the exact generation selected; and selector labels read New commit and Previous generation.
- [x] #21 Manual deployment from an auto_latest system offers Cancel, Continue on auto_latest, and Convert to manual and deploy; failed conversion queues no deployment; successful conversion followed by deployment failure reports the persisted manual policy and failed deployment; retries do not silently create duplicate deployments.
- [x] #22 Only the duplicate inner Compliance bundle edit action is removed, the authorized outer edit action remains, and file diff modals render and receive interaction above the flake tray.
- [x] #23 Authoritative browser coverage includes Config snapshot unavailable and API error, flake snapshot unavailable and API error, no previous flake revision, unauthorized/hidden environment, current/historical/never-deployed Config, expanded typed diffs, evaluation error, tracked/untracked provenance, all flake panes, cross-navigation, rollback, auto_latest, notification, modal-layering, and compliance states.
- [ ] #24 At the authoritative 1920x1080 viewport in light and dark themes and the 900x900 narrow viewport, affected states match commit eb5a1851 with no clipping, overlap, incorrect stacking, inaccessible controls, unintended inner scrolling, or table/card misalignment; assertions verify text, counts, revisions, warnings, actions, and navigation.
- [x] #25 Keyboard coverage verifies tab order/navigation, row expansion, revision controls, drawer and modal Escape behavior, modal focus trapping, and focus restoration after closing drawers and modals.
- [x] #26 New evaluation and flake-output APIs provide bounded server-side query/diff results, preserve supported agent/builder compatibility and environment authorization, avoid evaluation side effects on read paths and per-host evaluation during flake browsing, and return explicit unavailable/error states.
- [ ] #27 Targeted frontend and server tests, security/redaction tests, snapshot lifecycle and deduplication tests, auto_latest failure/idempotency tests, SQLx metadata/schema checks when applicable, the web-ui package build, the authoritative web-ui check, and broader Nix flake checks required by protocol, migration, packaging, or cross-package changes pass in the repository Nix development environment.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Finish-line V2 Config API slice from accepted service commit 1ccee7cf: (1) confirm exact branch/pipeline state and prove current commit-mode production handlers still use the V1 selector/readers; (2) audit generation retention and historical real/mock authority without changing generation mode or adding backfill/schema; (3) adapt the three existing public Config response DTOs to accepted V2 commit-mode query results while preserving generation-mode V1 behavior and authorization-before-selection; (4) add focused PostgreSQL-backed handler regressions for V2 availability, V1/V2 isolation, V1-only unavailability, Stage-2 unavailable truth, first-parent comparison, shared/stale tokens, corruption, non-disclosure, read-only behavior, and an injected executor writer-to-handler vertical path with scheduling suppression; (5) run disposable PostgreSQL Config Inspector/V2/API tests, SQLx checks, formatting/diff checks, and both architectural guards; (6) run the named fast TASK-440 browser workflows and only then the final server, server-regressions, integration, and web-ui Nix gates once; (7) audit MR !323, architecture, migration ordering, and scope; commit one coherent TASK-440: Serve Config Inspector V2 artifacts change, push, verify exact-head CI/local-remote equality, and leave TASK-440 In Progress. No migration, generation redesign, unsafe backfill, worker/service redesign, deployment, TASK-441 work, merge, or unrelated cleanup.

Public-contract clarification approved on 2026-09-08: preserve V2 missing/failure states by widening only existing Config DTO fields. Change definition and module source paths plus declared type to `Option<String>`; add optional `metadata_error`; change `overridden` to `Option<bool>`. V1 mappings wrap known values in `Some`. Mirror fields in the Web UI, render placeholders only in Dioxus, disable source-path interaction when absent, audit this adapter for lossy coercions, and add exact JSON/deserialization regressions before resuming the existing finish-line gates.

Focused Web UI VM verification exposed a test-harness environment propagation defect: the driver sets the real git fixture variables only in its own process, while the browser Node process runs inside the machine without them and falls back to example.invalid. Pass the existing real repository, commit, and configuration fixture values into the browser process, then rerun the exact seven TASK-440 workflows before broad gates. This is test-only and does not change product behavior.

Focused correction pass from exact HEAD `4ad7490e881e2a457b2fb6be95758392fd7af45a`: keep V2 selection/token/count/total authority in the existing read-only repeatable-read transaction; change the production V2 page query to select and order only narrow selected/baseline identities and digests before LIMIT/OFFSET; fetch at most the distinct selected/baseline payload digests referenced by the bounded page in a second query inside the same transaction; decode and resolve tracked provenance only for returned rows. Preserve global search through redacted `search_text`, digest-based Changed semantics including removed baseline rows, fail-closed missing/malformed page payloads, and unit-separator/C-collation ordering. Add adversarial ordering and production-scale ~15,000-option PostgreSQL regressions with All/Overridden/Changed/search/token/side-effect assertions, hydration bounds, practical measurements, and EXPLAIN evidence. No migration, V1 fallback, generation-mode change, UI workaround, timeout increase, integration change, rebase, merge, or deployment. Run focused DB checks, offline cargo lib/all-target checks, format/diff checks, architecture guards, 12l alone, 12l/12m/12n together, server-regressions, then one broad Web UI check. Commit only if all TASK-440-attributable requirements pass.

Focused 12l fixture sequencing correction approved on 2026-09-09: before starting the serial Config Inspector worker, remove queued fixture jobs that do not match both the live fixture commit and `configurationName`. Preserve production claim ordering, worker serialization, and the 300-second readiness bound. Verify JavaScript syntax and rerun only `12l-task440-config-lifecycle` through the repository Web UI test runner. Do not change product behavior, worker concurrency, timeout policy, or unrelated broad-suite isolation.

Production remediation assigned on 2026-09-09: (1) add direct runtime parity to `configInspectorWorkerScript` so a configured cache encryption key file is existence-checked and read into `CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY` immediately before exec; add a NixOS integration fixture that creates an unknown random test key at runtime before Crystal Forge starts and proves the real running worker has a non-empty variable without printing its value, while retaining ExecStart and slice/cgroup assertions; verify and commit/push as `TASK-440: Propagate Config Inspector encryption key`; (2) change only System Config lifecycle labels/details and adjacent Web UI enum documentation so queued/running describe source-agnostic configuration evidence preparation; update unit/browser expectations, verify focused lifecycle coverage, and commit/push as `TASK-440: Make Config lifecycle status truthful`. No migration, backend fallback change, worker ownership/concurrency change, retry of historical jobs, merge, or deployment.

Deployed-review remediation from exact branch SHA `24bd1d0e9950c918e698773efa936c2e2326f6c7`: (A) add an Admin-authorized and non-disclosing system Config-inspection POST route that atomically resolves the exact existing NixOS derivation and reuses the established Config Inspector enqueue state machine; return a typed 409 prerequisite without touching the primary evaluation queue; update the commit-mode Web UI action, lifecycle error handling, documentation, and focused DB/API/browser regressions; verify and commit as `TASK-440: Queue targeted Config inspections`. (B) deserialize `require_cve_check` writer input through the authoritative server `CveCheckConfig` defaults, preserve sparse JSON and explicit malformed-value rejection, add focused writer plus real publication/rollback regressions and narrow default documentation, verify and commit as `TASK-440: Preserve CVE policy XCCDF defaults`. Audit from the starting SHA, recheck the unchanged remote branch, push both commits, and leave TASK-440 In Progress with AC #24/#27 unchanged. No migration, evaluator redesign, worker concurrency change, deployment change, generation redesign, broad XCCDF refactor, TASK-441 work, rebase, merge, or deployment.

Commit B from exact parent 997d7d4826db9ca7c6723254c700a122c01a469c: deserialize only the XCCDF writer's require_cve_check implementation view through the authoritative server CveCheckConfig serde defaults; continue writing the original sparse JSON as config-json and do not add a strict XML representation. Add focused unit matrices for complete/defaulted, per-field omission, explicit values, and malformed bool/threshold/when_no_scan input. Add enrolled isolated-PostgreSQL publication regressions proving an accepted/trusted sparse seed-shaped CVE policy publishes and exports after commit, while malformed CVE writer input rolls back bundle publication. Update only the stale require_cve_check default contract. Run focused writer tests, the two isolated database regressions, offline cf-server lib check, rustfmt, diff check, and the XCCDF schema check if its focused command is available. Audit scope, then create exactly one local commit titled TASK-440: Preserve CVE policy XCCDF defaults; do not push.

Commit A final-audit correction from parent 24bd1d0e: acquire the shared snapshot-writer transaction lock before all automatic and targeted Config enqueue resolution/readiness work; reject active queued/running rows whose derivation or carrier differs from the newly resolved target with a typed retryable conflict and no mutation; enforce CSRF on the retained whole-commit prerequisite mutation after authentication/role checks; strengthen focused PostgreSQL/API and live 12l coverage; correct lifecycle and prerequisite documentation without changing primary evaluator semantics; run focused DB/API, syntax, formatting, offline server, Web UI, and 12l checks; audit exactly the 11 Commit A files; create one local commit named TASK-440: Queue targeted Config inspections; do not push or alter the stash.

Final Commit A P1 correction: update `missing_config_snapshot_lifecycle_v2` to resolve the same unique completed NixOS derivation/carrier used by targeted inspection and select only jobs matching both `derivation_id` and `carrier_drv_path`. Preserve the existing primary-evaluation fallback when no exact job matches and retain the read-only system-to-flake non-disclosure join. Add a focused isolated-PostgreSQL regression for obsolete queued/failed identities and exact matching lifecycle, run the requested focused lifecycle tests and structural gates, then audit and commit only A files without popping the corrected B stash or pushing.

Final deployed-review policy-draft remediation from exact SHA `b0018e497dd4a30ddb334b77bd2d74bf5d056763`: add matching Web UI `CreatePolicyDraftRequest`/response DTOs, send `Some(&CreatePolicyDraftRequest { new_version: None })` from the accepted-policy Create draft action so the shared client emits JSON and Content-Type while preserving CSRF, and extend the existing focused real policy catalog browser workflow to assert the exact request plus persisted draft lineage/current pointers/source immutability and refreshed UI. Run focused Web UI/client, browser, server lifecycle, WASM/package, formatting, syntax/manifest, and diff checks. Audit the narrow scope, commit as `TASK-440: Send policy draft JSON`, recheck the unchanged remote starting SHA, push normally, and leave TASK-440 In Progress with AC #24/#27 unchanged. No server production change, migration, Config Inspector, XCCDF, bundle publication, evaluator, generation, deployment, TASK-441, rebase, merge, or deployment.

Final bounded policy-draft JSON remediation from exact clean HEAD `b0018e497dd4a30ddb334b77bd2d74bf5d056763`: mirror the server `CreatePolicyDraftRequest` and `CreatePolicyDraftResponse` in the Web UI; make `create_policy_draft(policy_id, request)` send `Some(request)` through the existing CSRF JSON sender; pass `{ new_version: None }` only from the accepted-policy production button; add focused DTO serialization and client/view source guards; extend `20af-policy-catalog-selection-delete-regressions` after its strict capture to prove the real production request, CSRF, response, persisted lineage/pointers/source immutability, and refreshed editable state. Do not change server production code or any excluded subsystem. Verify targeted Web UI tests, changed-file rustfmt, Node/manifest/static checks, wasm/offline/package build, the authoritative focused Web UI workflow, and the existing isolated-PostgreSQL server policy draft lifecycle regression; stop if valid JSON exposes an independent lifecycle failure; audit and create exactly one local commit titled `TASK-440: Send policy draft JSON` without pushing.

Diagnostic-only policy-draft follow-up: instrument `20af-policy-catalog-selection-delete-regressions` to snapshot every `deployment_policy_versions` column for the exact accepted source UUID immediately before and after the production Create draft POST, emit complete rows and a per-column `IS DISTINCT` equivalent comparison, and rerun only 20af. If any column changes, stop without server changes. If no column changes, replace the hash assertion with explicit immutable semantic-field and identity/state assertions, then finish only the already-declared client/browser verification.

2026-09-10 final closure research gate: Before code changes, resolve two material contract gaps. XCCDF: choose whether typed custom-check export follows CF-XCCDF v0.1 (`binding=cfg`, expressions projected to `cfg.config.*`) or a revised/versioned `config` binding contract; also decide how the valid no-enforcement shape `{mode: all, rules: []}` is represented because the current XSD requires at least one rule. Config Inspector: guarded traversal can preserve healthy options, but schema V2 cannot truthfully mark partial option enumeration or root/subtree diagnostics. Any implementation that preserves partial results requires an artifact/persistence/API completeness extension and likely a migration; otherwise the inspector must remain globally unavailable. Do not implement until these decisions are approved.

2026-09-10 approved two-commit closure plan from exact clean HEAD `1e986cd2351652e866f9b72bff956a312fc0827a`. Commit A (`TASK-440: Align custom-check XCCDF context`): derive current export metadata as context `nixos-configuration-v2` plus binding `config`; preserve canonical `config.*` expression text; retain V1 `cfg`/`cfg.config.*` import compatibility by normalizing to current canonical JSON; reconcile typed custom-check projection with lossless `cf:config-json`; widen XSD to allow zero rules only for explicit All no-enforcement semantics; prove current single/multi/empty isolated export-import canonical equality, digest behavior, normal evaluator/gating parity, production-shaped publication and post-commit export, and malformed atomic rollback; update maintained profile. Commit B (`TASK-440: Represent partial Config inventories`): add additive migration 0254 and typed bounded inventory-completeness diagnostics; port guarded attrNames/child-WHNF/_type/depth traversal to targeted V2 inspector only; preserve healthy exact options and record unknown children as unreadable prefixes; make partial artifacts available/selectable but never comparison-ready; skip fabricated Stage-2 identities while retaining healthy work; expose minimal API and UI truth; prove old V2 complete backfill, V1 isolation, poison-before/after, root unavailable, certification/selector/count/All/Search/Changed semantics, and retained shared-index complexity. Run only focused XCCDF, real-Nix Config, Rust, isolated PostgreSQL, offline lib, formatting, and diff checks. Run no broad Web UI or flake checks; if browser behavior changes, run only the narrow partial Config scenario. Preserve policy Create-draft production behavior and leave TASK-440 In Progress with AC #24/#27 unchanged.

2026-09-10 maintainer authorized rewriting only the two unpushed temporary implementation commits. Correct Commit A for runtime precedence when `expression` coexists with empty/non-empty rules, V1 source-digest verification before deterministic canonical normalization, shared API/import custom-check validation, and direct original/imported evaluator pass/fail/multi parity. Correct Commit B by replacing repeated list membership with one indexed key set and by deterministically bounding diagnostic detail with certified truncation state while traversal continues beyond 128 poison prefixes. Re-run focused checks and independent review, then require exactly two corrected commits above remote `1e986cd2` before a normal fast-forward push.

2026-09-14 deployed CVE inventory blocker remediation from exact clean head `d2ca1c180d0a9e7c9b1a72439709fc98469ede5f`: stop browser-CI iteration and separate rolling-upgrade CVE inventory authority from strict exact-CVE remediation authority. Preserve all exact retained-generation, verified-lineage, certified-artifact, schema-1 scan, immutable-observation predicates for POA&M, scheduling, verification, closure, link/reopen, and existing exact justification mutation semantics. Add a backward-compatible inventory response path that selects exact evidence when available, otherwise reads the prior bounded current-system inventory source and marks it `legacy`, otherwise reports `no_scan`; include scan timestamp and a typed first failed exact-authority reason. Never union exact and legacy rows, backfill schema-0 scans, fabricate immutable occurrences, or fabricate retained lineage. Update system and fleet read UI so legacy findings remain visible and counts do not collapse, while exact remediation controls are disabled with truthful guidance; distinguish exact clean, legacy clean, no scan, legacy findings, and exact findings. Add production-shaped upgraded PostgreSQL regressions for legacy vulnerable/clean/no-scan, exact current, generation/store mismatch, lineage false, and exact precedence/no duplicates, plus focused API/UI/browser coverage and contract documentation. Verify focused PostgreSQL CVE and exact-POA&M tests, cve-processing-test, server-regressions, focused Web UI CVE tests, WASM check, formatting, and `git diff --check`; then commit narrowly, push, redeploy to the authorized real non-production environment, and verify existing CVEs reappear without enabling legacy remediation before resuming broad Web UI CI. Do not merge MR !323.

Verification constraint added by maintainer on 2026-09-14: do not run costly Web UI Nix/browser checks during this blocker remediation. Use focused PostgreSQL/server tests, exact-CVE regressions, cve-processing-test/server-regressions as requested, lightweight Web UI unit/static checks, WASM check, formatting, and diff checks. Defer broad/costly Web UI gates until the maintainer explicitly resumes them after deployed smoke.

2026-09-14 next recovery sequence after CVE commit: Phase 1B adds one shared observational current-revision resolver for Config. Resolution priority is retained exact identity, then unambiguous durable deployment identity with exact observed store/system/flake/configuration, then unique legacy exact-store derivation mapping constrained to the same flake and effective configuration; mismatch or ambiguity remains unmapped and never grants rollback authority. Harden generation fallback, populate `/commits.current_commit`, expose per-commit Config inspectability, and make Commit mode select the newest inspectable completed revision rather than `commits[0]`. Verify focused DB/API/UI/WASM checks, commit as `TASK-440: Restore Config upgrade inspection`, push, deploy, and smoke before distributed scanning. Phase 2 then designs a server-authoritative DB-free remote scan lease protocol for scanner-capable API builders with build priority, post-build affinity, conservative capacity, old-builder compatibility, bounded validated result submission, stale-session fencing, and server-local fallback. Immediate/manual/periodic/post-build triggers converge on one scan work lifecycle; split protocol/server/builder/Nix changes into reviewable commits. Phase 3 reproduces generation retention in both deployment-first and observation-first order through current production paths; change retention only if focused reproduction fails. Do not rebase, force-push, merge !323, mutate legacy evidence, or run broad Web UI checks as an inner loop.

Distributed scanning behavior approved on 2026-09-15: for a fresh API build, the producing builder obtains a server-issued exact scan lease after Nix build success, then runs cache publication and vulnix concurrently while output and recorded `.drv` are local. The builder does not claim another build until both tasks settle, but reports build/cache and scan as separate authoritative outcomes; scan failure MUST NOT falsify Nix build success. Server retains scheduler/database/result-sealing authority. Crash/session loss revokes and requeues scan work for another scanner or server-local fallback. Historical/manual/periodic work uses the same lease lifecycle. Approved defaults: scanner capability enabled by default in the NixOS builder module with explicit opt-out; old wire clients default incapable; one concurrent scan per builder; no background scan claim ahead of queued/active build work; structured evidence with exact `.drv`-to-output mappings; 8 MiB body, 50,000 entries, 250,000 observations; deterministic digest for idempotent completion; invalid payload returns 422 while lease remains active; narrowly authorized cache materialization data and server-issued bounded scan policy. Post-build direct lease replaces the normal affinity delay for the producing builder; fallback preference/grace applies only after failure/unavailability and to non-post-build work. The maintainer has independently begun deploying `7e1f0846` for Phase 1 smoke, so this agent will not run an undocumented deployment command.

2026-09-15 deployed P0 resequencing from deployed `7e1f0846`: stop distributed-scanner implementation. First reproduce an exact failed verified-source build. The failure affects both remote builder `webb` and worker `reckless`, so diagnose the shared evaluator/source contract rather than assuming a remote-only defect. Record job/derivation/source/strategy/fingerprint identities, executing Nix versions, exact server and builder evaluation flags, archive digest, extracted worktree HEAD and cleanliness, controlled pure/impure and evaluator-option variants, and normalized derivation differences. Preserve the mandatory pre-build exact drvPath comparison, do not silently change strategy, and do not build a mismatch. Correct the evaluator fingerprint and only align evaluation semantics after the differing dimension is proven. Add matched-source/settings success, genuine-plan mismatch refusal, evaluator-mismatch diagnostics, truthful fingerprint, archive/worktree identity, and no-fallback regressions; then deploy and prove one real build before declaring builder health.

2026-09-15 deployed System CVE P0: replace the `MAX_SYSTEM_CVE_INVENTORY_ROWS = 1_000` whole-result rejection with a bounded deterministic paged inventory contract. Apply authorization and environment scoping before pagination; expose authoritative total/count and exact/legacy/no-scan/clean metadata independently of the page; preserve stable CVE/package identity and no duplicates; and ensure search/filter semantics apply to the full scoped inventory rather than only a loaded page. Add a production-shaped 1,315-finding PostgreSQL/API/UI regression proving first/subsequent pages, complete reachability, totals, authority metadata, non-disclosure, and clean/no-scan distinctions. Redeploy and smoke the `webb` System CVE tab.

Updated recovery order: (1) exact all-builder verified-source mismatch reproduction and security-preserving fix; (2) bounded System CVE paging fix; (3) deploy and prove one real build plus `webb` CVE tab; (4) resume server-authoritative DB-free distributed scanner work; (5) prove producing-builder post-build scan affinity on the now-working builder; (6) reproduce generation retention. Protocol-only distributed-scanner commit `a3e4fac7` was pushed immediately before these blocker instructions arrived, but it must not be deployed and no further scanner changes may be committed or pushed until both P0 blockers are resolved. Uncommitted scanner-server work must remain preserved and inactive. Do not merge MR !323.

2026-09-15 authoritative-source P0 diagnostic plan from exact HEAD `8bc2e0c4`: preserve the scanner stash and generated Tailwind file. First obtain service-shaped evidence without retrying multiple commits: verify the deployed unit's effective executable paths and source-root access, determine campground credential type without reading secret material, and reproduce the hardened Git configuration locally. The historical journal cannot prove the first failed stage because journald suppressed 51,994 service messages at 21:21:24 and the current error path persists only the outer `failed to materialize authoritative immutable source` context. Then implement the narrow repair demonstrated by evidence. Independently preserve a bounded redacted materialization stage/cause and the typed retry class through `EvaluationFailure`, logs, and attempt persistence. Add focused cold/warm exact-source, authenticated HTTPS when applicable, invalid-source fail-closed, redaction, service-user filesystem/Nix/Git, evaluator-launch, incremental-dispatch, and derivation-mismatch regressions. Run targeted Rust/Nix checks, audit documentation and scope, commit as `TASK-440: Repair authoritative source materialization`, push normally, deploy through the established procedure, and use the supported retry action for exactly one affected campground commit. Do not merge MR !323 or resume distributed scanning until the live retry proves evaluator launch and exact derivation identity.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-15 exact-head CI correction committed and pushed as `31819e2f` (`TASK-440: Correct CVE inventory regressions`). Pipeline 2852445060 exposed four narrow issues: the Web UI runner expected list omitted new workflow 12ha; a stale static assertion still expected direct `allow_mutations` gates instead of stricter authority-aware gates; fleet affected-system rows were not deduplicated by system before the 1,001-system overflow probe; and one exact-CVE relationship test claimed Admin without persisting that role. The correction updates the two test contracts, deterministically selects one CVE/package occurrence per system before applying the system bound, and persists the fixture role without weakening production authorization. Verification passed: both originally failing PostgreSQL regressions in a disposable isolated PostgreSQL cluster; Web UI static contracts; `nix build .#checks.x86_64-linux.web-ui-test-runner --no-link -L`; `nix build .#checks.x86_64-linux.server-regressions --no-link -L` (full check); package rustfmt; and `git diff --check`. Read-only P0/P1 audit found no blocker. The task preview was rebuilt at `31819e2f`; authenticated HTTPS LAN access remains at https://10.8.0.177:8090. The new exact-head CI pipeline must still validate the costly full Web UI VM check.

2026-09-15 read-only P0 research at `8bc2e0c4`: local, upstream, and `ls-remote` all equal `8bc2e0c4f51028914a652d326104251b52c6a848`; only untracked `packages/web-ui/assets/tailwind.css`; scanner work remains in `stash@{0}`. The deployed service runs as `crystal-forge:crystal-forge`, works in `/var/lib/crystal-forge`, allows `/var/lib/crystal-forge/source-archives`, has no drop-ins, and uses `ProtectSystem=no`/`ProtectHome=no`. Source-root, lock, mirror, and staging paths are owned by `crystal-forge:crystal-forge`, mode 0755, and no source lock was visible, so the leading permission/lock-contention hypotheses are weakened. The exact historical journal window reports `Suppressed 51994 messages from crystal-forge-server.service` at 21:21:24 and contains no materialization error, so that diagnostic is no longer recoverable from journald. Code review confirmed materialization stages 7-37 are collapsed by `.context("failed to materialize authoritative immutable source")` and later `to_string()`, losing the inner cause and typed `MaterializationFailureClass`. A read-only local probe also showed the current `GIT_CONFIG=/dev/null` hardening suppresses the `GIT_CONFIG_COUNT` credential helper used for PAT/username-password HTTPS fetches; campground's credential type still requires privileged DB/API confirmation. Non-privileged `psql` failed because role `mcamp` does not exist. No source files changed and no hotfix was created.

2026-09-15 follow-up service evidence: campground uses unauthenticated HTTPS (`auth_type=none`), so the PAT helper conflict is not this incident's cause. The live service PATH contains Nix 2.35.2, Git 2.55.0, OpenSSH, and `nix-eval-jobs` 2.35.2; `NIX_REMOTE=daemon` and flakes are enabled. The expected bare mirror `repo-d587fee8cb868adf02a2f32d.git` exists, is owned by `crystal-forge`, is a valid bare repository, and already contains both failing commits. Hardened unauthenticated `git ls-remote` succeeds. Read-only `git archive` succeeds for both commits in 7-9 ms, each artifact is 1,628,160 bytes, both contain readable `flake.lock`, and the only symlink is safe (`AGENTS.md -> CLAUDE.md`). Neither commit has a published artifact or identity. The remaining likely boundary is therefore extraction, `nix store add-path`, NAR hashing, or publication; the 16 ms duration most strongly favors an immediate extractor or Nix invocation/daemon failure. The next diagnostic must run the exact materialization sequence as `crystal-forge`; current SSH cannot become that user without the maintainer's sudo password.

2026-09-15 service-user reproduction result supplied by maintainer: `nix store ping` succeeds as `crystal-forge` with `NIX_REMOTE=daemon` and reports daemon 2.34.5 (the service CLI/evaluator PATH is 2.35.2). The exact campground commit archives, extracts with GNU tar, and ingests through `nix store add-path --name crystal-forge-source-v1-169fa07f128d235bef0aeae239783c4a02abb013`, producing `/nix/store/8957pjsnf0mk4kfw4sj6yzb50p5cz39b-crystal-forge-source-v1-169fa07f128d235bef0aeae239783c4a02abb013`. This does not mutate application state; the store object is GC-eligible. External Git, filesystem, flake.lock, Nix daemon, and add-path stages are therefore operational. Remaining code-specific suspects are the bounded Rust extractor and artifact/identity publication; the CLI/daemon version difference should be recorded but does not explain add-path failure by itself. No further broad host probing is needed before a focused stage-preserving diagnostic/fixture change.

2026-09-15 authoritative-source hotfix committed and pushed as `7f923d55` (`TASK-440: Accept Git archive metadata`). Root cause: `git archive --format=tar` emits a global PAX metadata member before tracked entries, while the contract-v1 Rust extractor rejected every non-file/directory/symlink member. The fix ignores only tar global PAX extension metadata before path extraction; hard links, devices, FIFOs, unsafe paths, duplicate paths, and limits remain fail-closed. A regression reproduces Git's `pax_global_header`, verifies `flake.lock` extraction, and verifies metadata is not materialized. Verification passed: cf-protocol source-artifact tests (8 passed), cf-server verified-source tests with `SQLX_OFFLINE=true --offline` (5 passed), cf-protocol rustfmt check, and scoped `git diff --check`. An initial cf-server test invocation without SQLX_OFFLINE failed at compile time because no local PostgreSQL was listening; the corrected offline invocation passed. Local HEAD, upstream, and remote all equal `7f923d5535ffde3ded45649fb42ee0b75b7d453a`. The generated untracked Tailwind file and scanner stash remain untouched. Awaiting maintainer redeploy and one controlled campground retry; do not merge MR !323.

2026-09-15 local-only evaluation performance diagnosis and correction: no SSH or remote commands were used. The saved deployed journal shows commit 3252 starting nix-eval-jobs at 22:47:09 for 39 declared configurations with two workers; the first successful result arrived at 22:58:42. The flake uses build_scope=cf_systems_only and has policy assignments for 10 managed configurations, but the primary evaluator still forced all 39 declarations and treated unmanaged declarations as expected results. This exhausted both workers on configurations that could never create build jobs. The correction adds an optional configurationNames boundary to primary_evaluation.nix, passes active registered configuration names for cf_systems_only, and applies the identical scope to missing-system detection so excluded declarations cannot trigger fallback evaluation. all_configs and unscoped callers retain full-flake behavior; the complete declared-system inventory remains in commit_artifacts_cache for Flake Explorer reconciliation. Verification passed: focused Rust expression test; focused expected-system boundary test including scoped, empty, and unscoped cases; evaluator-snapshot-isolation Nix check with an excluded aborting configuration; verified-source-evaluator-parity Nix check; cargo fmt check; git diff check. A broad offline cf-server lib run reached 1504 passing tests but failed eight existing CVE database tests because no DATABASE_URL service was available (PoolTimedOut); the failures do not execute the changed evaluator code. Local reproduction against the preserved older campground checkout was not comparable because its locked dotfiles input now reports a NAR hash mismatch. Runtime before/after must be confirmed after maintainer-controlled deployment; based on the observed workload the primary evaluator input falls from 39 configurations to the 10 active managed configurations (29 excluded, about 74% less configuration work).

2026-09-15 evaluation-scope performance correction committed and pushed as `481ae958` (`TASK-440: Bound scoped flake evaluation`). The generated Tailwind asset remains untracked and untouched. No SSH, deployment, database mutation, merge, or scanner work was performed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: openai-agent
created: 2026-08-28 16:42
---
Parallel-work coordination (2026-08-28): TASK-440 may be implemented in parallel with TASK-433, but TASK-440 must not merge until TASK-433 is merged. Before TASK-440 adds any database migration or refreshes SQLx metadata, inspect TASK-433's latest branch state and migration numbers, then rebase/update from post-TASK-433 `dev` and allocate new additive migration numbers. Known overlap includes System Detail/Compliance UI, server models/queries/handlers, migrations, SQLx metadata, and browser checks. Preserve TASK-433's POA&M behavior when resolving overlap.
---

author: @openai-agent
created: 2026-09-03 08:23
---
Second immutable-artifact audit remediation verified on 2026-09-03. Fixed exact deployment-artifact retention in both finalization directions, protected deployment-bound artifacts from GC, made pre-0248 retained lineage queryable but rollback-ineligible through `lineage_verified`, preserved upgrade compatibility when derivation store metadata is null, and made stale snapshot tokens authoritative over failed, unavailable, or absent replacement state for Config options, summary, and module-source reads. Expanded the populated pre-0248 rehearsal and migration-backed immutable retention/GC/rollback regression. Updated `docs/evaluation-flake-snapshots.md` and `docs/specs/02-backend-api.md`. Verification passed: `nix develop ../.. -c env SQLX_OFFLINE=true cargo check --offline --package cf-server`; `nix develop ../.. -c cargo fmt --package cf-server -- --check`; `nix build .#checks.x86_64-linux.server-regressions -L` (7m42s); scoped `git diff --check`. Existing repository warnings remain; no new warning from this remediation. TASK-440 remains In Progress because AC #24 and final MR workflow are outside this backend remediation pass.
---

author: @openai-agent
created: 2026-09-03 09:26
---
Final backend audit verification completed on 2026-09-03. The final audit regressions now cover fail-closed migrated legacy generation reads and reciprocal binding, Available-only deployment binding, unavailable/oversized finalization without generation retention, and source-reset preservation of deployment-bound snapshot/derivation lineage. Verification passed: `nix build .#checks.x86_64-linux.server-regressions -L`; `nix develop ../.. -c env SQLX_OFFLINE=true cargo check --offline --package cf-server`; `nix develop ../.. -c cargo fmt --package cf-server -- --check`; and scoped `git diff --check -- packages/default/crates/cf-server checks/server-regressions/default.nix docs/evaluation-flake-snapshots.md docs/specs/02-backend-api.md`. Cargo check completed with existing warnings. Final scope review found no unresolved backend issue. No frontend files were edited during this audit, and no files were staged, committed, or pushed.
---

author: @openai-agent
created: 2026-09-03 09:45
---
Final deployment/snapshot race P1 resolved on 2026-09-03. Added one shared `SNAPSHOT_WRITER_LOCK_KEY` transaction-lock helper and made both deployment-creation transactions acquire it before the system-row lock and before artifact selection/binding. Snapshot publication, deployment creation, generation retention, and reclamation now use the documented lock order: snapshot-writer advisory lock first, row locks second. Added and enrolled migrated PostgreSQL regression `deployment_creation_and_snapshot_finalization_serialize_exact_binding_and_retention`; it holds the advisory lock until deployment creation and snapshot finalization are both waiting, releases them to race in either order, and proves exact deployment binding plus retained snapshot/derivation/commit lineage. Passed: `nix develop ../.. -c env SQLX_OFFLINE=true cargo check --offline --package cf-server`; `nix develop ../.. -c cargo fmt --package cf-server -- --check`; `nix build .#checks.x86_64-linux.server-regressions -L`; scoped `git diff --check -- packages/default/crates/cf-server checks/server-regressions/default.nix docs/evaluation-flake-snapshots.md docs/specs/02-backend-api.md`. Existing compiler warnings remain. No frontend files were edited and nothing was staged, committed, or pushed.
---

author: opencode
created: 2026-09-05 16:25
---
Implemented and pushed commit 5b5be966 to origin/TASK-440-system-config-flake-parity. Existing MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323

Verification completed: cargo fmt --check; SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml -p cf-server --lib; nix build .#checks.x86_64-linux.config-inspector --no-link --print-build-logs; nix build .#checks.x86_64-linux.evaluator-snapshot-isolation --no-link --print-build-logs; git diff --cached --check. Cargo emitted pre-existing warnings; checks exited successfully.
---

author: openai-agent
created: 2026-09-05 19:17
---
Integrity remediation started from exact local/remote HEAD 950c31fb055c251243c003d89df7b700094729dd. Worktree was clean before changes. Scope is limited to Stage-1/Stage-2 identity and wire-integrity binding; no semantic-model, persistence, HTTP, UI, deployment, or Flake Explorer work.
---

author: openai-agent
created: 2026-09-05 19:30
---
Integrity remediation completed and pushed as commit c98fba26 on TASK-440-system-config-flake-parity. Stage-1/Stage-2 inspection target binding, carrier derivation binding, duplicate index identity rejection, explicit supported-field enforcement, and adapter reason sanitization are implemented. Task remains In Progress per instruction.
---

author: Codex
created: 2026-09-05 21:14
---
Implemented and pushed final integrity remediation in commit `5f331e30` on `TASK-440-system-config-flake-parity`. Stage 1 and Stage 2 now bind to the resolved flake `outPath`, caller-owned target key, and shared carrier derivation. Stage-2 index failures return explicit sanitized `stage2_index_failed` results. Added focused mismatch and redaction tests. Verification passed: `nix build .#checks.x86_64-linux.config-inspector --no-link`; `nix develop --command cargo test --manifest-path packages/default/Cargo.toml -p cf-server models::config_inspector --lib` (30 passed); `cargo fmt --manifest-path packages/default/crates/cf-server/Cargo.toml --check`; `git diff --check`.
---

author: Codex
created: 2026-09-05 21:14
---
The pushed commit covers the bounded config-inspector integrity remediation only; the broader TASK-440 acceptance criteria remain incomplete. Restored task status to In Progress. The MR remains open for this remediation review.
---

author: Codex
created: 2026-09-06 00:14
---
Final Stage-1 single-resolution remediation completed and pushed as `5b8a8514` on TASK-440-system-config-flake-parity. `config_inspector.nix` now accepts `{ flake, configuration, targetKey, encodeValue }` and contains no target resolution. Structural tests prove one `builtins.getFlake` in generated Stage 1, one in generated Stage 2, and none in the inspector source. Verification passed: 30 targeted config-inspector Rust tests; `nix build .#checks.x86_64-linux.config-inspector --no-link --print-build-logs`; `nix build .#checks.x86_64-linux.evaluator-snapshot-isolation --no-link --print-build-logs`; Nix-dev `cargo fmt --check`; `SQLX_OFFLINE=true cargo check -p cf-server --lib`; and `git diff --check`. Task remains In Progress; worktree is clean and local/remote HEAD is `5b8a85148417d7944a9a3860743ce718a21a4b49`.
---

author: Codex
created: 2026-09-06 01:43
---
Bounded semantic assembly slice completed and pushed as `e55eb87a` on TASK-440-system-config-flake-parity. Added pure `assemble_config_inspection` joining validated Stage-1 and Stage-2 results without Nix, JSONL, DB, API, or DTO changes. Preserves explicit metadata/value/provenance/enrichment states, raw definition metadata including nullable source paths, multiple survivors, ordinal identity, and integrity failures. Producer inspection confirmed `definitionsByOption` omits zero-definition options; assembly treats omission as known zero definitions. Verification passed: 37 targeted Config Inspector tests; Config Inspector Nix check; evaluator snapshot isolation check; Nix-dev fmt check; SQLX_OFFLINE cf-server lib check; and diff check. Task remains In Progress; worktree and local/remote heads are clean and equal.
---

created: 2026-09-06 17:07
---
Starting bounded V2 DB-only reader core from accepted selector-isolation commit baaac5de6eff905c1506e5c55fcfbfdb75905a22. Initial inspection found existing V1 commit reader and V1 page code intentionally use evaluation_snapshot_selections and option_path; the new reader will remain separate and use config_snapshot_selections plus V2 identity columns.
---

author: openai-agent
created: 2026-09-07 16:19
---
Bounded durable Config Inspector scheduling slice completed and pushed as commit 49520a25 on TASK-440-system-config-flake-parity. Added migration 0252_config_inspection_jobs.sql, exact finalized-target validation, real-mode-only enqueue wiring, V2 same-carrier suppression, active idempotency, terminal retry, mismatch/atomicity/concurrency/lifecycle tests, and non-fatal enqueue failure handling. Verified in the repository Nix environment: 8 focused config-inspection PostgreSQL tests passed; 23 finalization tests passed sequentially; 4 V2 artifact tests passed; 10 V2 remediation tests passed; 1393 cf-server library tests passed with 483 ignored; cargo fmt/check passed; config-inspector and evaluator-snapshot-isolation Nix checks passed; schema/index/trigger audit passed; scoped diff check passed. Full nix flake check was attempted but remains blocked by existing test-flake MAIN_HEAD evaluation errors and unavailable remote cache workers. MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323
---

author: openai-agent
created: 2026-09-07 18:54
---
Scheduling remediation follow-up committed and pushed as 868d11db. Finalization group: 23/23 passed. Config Inspector scheduling DB tests: 8/8 passed. Four focused V2 selector/persistence tests passed. The two non-metric lifecycle failures were reproduced identically on corrected and pristine 49520a25 worktrees: failed_and_corrupt_snapshots_requeue_with_active_lifecycle (Available vs Unavailable at evaluation_snapshots.rs:5677) and retained_generation_survives_store_metadata_loss_and_blocks_commit_deletion (snapshot finalization should retain the observed generation at evaluation_snapshots.rs:9254). Three metric tests remain unverified because pg_stat_statements is unavailable in the disposable database. Both architectural Nix guards, cargo checks, formatting, and diff checks passed. Task remains In Progress pending maintainer review.
---

author: openai-agent
created: 2026-09-08 02:14
---
2026-09-07: Added and pushed the bounded durable Config Inspector worker as commit a41d41e8. The dedicated serial worker uses 5-second skipped intervals, startup/periodic stale recovery with a 10-minute threshold, one claim per cycle, direct awaited execution, and mock-mode gating before database initialization. Added worker unit coverage and the config-inspector-worker binary/package target. Verification passed: 13 focused worker tests, Nix-dev cargo fmt --check, SQLX_OFFLINE cargo check --offline -p cf-server --all-targets, evaluator-snapshot-isolation Nix guard, Config Inspector Nix guard, disposable PostgreSQL 17 Config Inspector tests (17/17), and nix build .#server with config-inspector-worker present. A direct non-Nix cargo test attempt was blocked by missing host OpenSSL tooling; the Nix-dev test passed. TASK-440 remains In Progress because broader acceptance criterion #27 and final MR workflow are incomplete. MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323
---

author: openai-agent
created: 2026-09-08 03:07
---
The bounded Config Inspector NixOS service-isolation slice is ready for maintainer review in commit `1ccee7cf6aa59c3dc66f80ba02ed0817d2c0c9ba` on MR https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323. No deployment was performed.
---

author: @openai-agent
created: 2026-09-14 20:21
---
Takeover on 2026-09-14: read the complete prior session and verified the dedicated worktree is clean at `d2ca1c180d0a9e7c9b1a72439709fc98469ede5f`, equal to `origin/TASK-440-system-config-flake-parity` and MR !323. Continuing the recorded plan: isolate the three critical Web UI failures with the impure targeted CI job before another full authoritative run. GitLab API credentials are currently unavailable locally; public API access remains read-only.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Hardened V2 config snapshot reads with fail-closed corruption handling, comparison-unavailable state, literal LIKE search escaping, structured path identity coverage, provenance/global-unavailability coverage, query bounds, and side-effect regression checks. Verified targeted Rust tests, Nix config-inspector and evaluator-snapshot-isolation checks, formatting, diff checks, and cargo check. Committed and pushed as deb37a5d6f9653fda55ec12a36abfa75425d4986.
<!-- SECTION:FINAL_SUMMARY:END -->
