---
id: TASK-440
title: Bring system configuration and flake exploration into full design parity
status: In Progress
assignee:
  - '@openai-agent'
created_date: '2026-08-28 03:43'
updated_date: '2026-09-17 19:01'
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
  - git commit 3a5f207c249c4c272f4b2ab32e11bacefcb9ba45
  - git commit 7f49d8bb41ae0b86c5fa1ac8e6d375e7f134dfd7
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2857537294'
  - git commit 9562fe09e94de3c790cd512ce7c77cc819bc0779
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2859062860'
  - git commit bf27c03d03f15355a967960c5cc666b880b01064
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
2026-09-16 Scanning design-parity slice in the existing dedicated TASK-440 worktree: (1) align `packages/web-ui/src/views/scanning.rs` with the authoritative ScanningView design using the truthful existing stats, deployed, queue, systems, per-system scans, environment, and schedule APIs; (2) use exact Deployed, All scans, and By system tabs with count badges, one shared filter/sort/table presentation, robust loading/empty/error states, and at most one expanded system; (3) remove the non-design activity side panel and preserve schedule editing; (4) keep fleet/per-row rescan, build-and-scan, scan-log, and cancellation controls visibly disabled because no supported frontend mutation/log contract exists, and omit scanner version/database age because APIs do not provide them; (5) add focused pure Rust tests for status/freshness normalization plus filtering and sorting; (6) add scoped responsive/accessibility CSS in `packages/web-ui/assets/app.css`; (7) run web-ui formatting, targeted unit tests/check, static contracts if applicable, and diff checks, but do not run the authoritative web-ui Nix check.

2026-09-16 distributed CVE scanner completion after applying preserved stash: (1) retain server-owned signed/session-fenced lease APIs and renumber the additive migration to 0263 after the committed retry migration; (2) add builder HTTP client methods and a bounded local vulnix executor that advertises capability only when enabled, executes the exact claimed output without database access, heartbeats during execution, preserves affected and whitelisted evidence, canonicalizes the schema-1 payload, and reports completion/failure independently from build outcome; (3) prefer an immediate claim by the builder that completed the build and leave delayed fallback to the server-local worker unless a truthful cache materialization source exists; (4) harden claim/write paths for enabled/current builders, environment authorization, stale-recovery separation, durable post-build enqueue, trigger provenance, and closure-bound package evidence; (5) add focused protocol, builder, handler, migration-backed lease, concurrency, and recovery tests; (6) package vulnix in the production/development builder runtime; (7) run targeted Nix-environment checks and isolated PostgreSQL regressions, while leaving the authoritative Web UI and broad flake checks to CI per maintainer instruction.

2026-09-16 server-owned distributed CVE hardening pass in `/home/mcamp/code/crystal-forge/TASK-440-system-config-flake-parity` on `TASK-440-system-config-flake-parity` from existing task branch HEAD `891bb8e8`: (1) make successful build completion and policy-enabled post-build scan enqueue one durable transaction, with idempotent recovery on completion retries and preservation of an existing manual/fleet active trigger; (2) require enabled, registered, active, current-session builders for claim/heartbeat/complete/fail, and enforce the existing wildcard-or-assigned environment rule during non-disclosing candidate selection; (3) separate typed remote lease recovery from legacy metadata recovery and run typed recovery even when server-local vulnix is unavailable; (4) persist additive scanner identity/version capability and affected/whitelisted evidence losslessly while preserving old builder JSON; (5) validate submitted package outputs against immutable closure evidence available from the server's Nix store and document that server-unavailable closure membership remains a trusted scanner boundary rather than invented evidence; (6) allow non-producing remote fallback only when existing cache/substituter provenance proves materialization, otherwise preserve delayed local fallback; (7) tighten additive migration constraints without rejecting coherent upgraded legacy rows; (8) add focused query/handler/database tests and enroll isolated DB coverage; (9) run scoped rustfmt, protocol/server checks and unit tests in `nix develop`, then the repository disposable PostgreSQL helper if it establishes an isolated database. No Web UI, generated Tailwind, broad flake check, or authoritative web-ui check will be changed or run.

2026-09-16 bounded Scanning UI parity follow-up: preserve the desktop 250px toolbar search width; remove only Scanning's 900px stat/count wrapping overrides; make the <=640px search consume a full flex row; visibly mute disabled scan controls; add table-header `aria-sort` and quieter inactive chevrons; make unfiltered CVE navigation labels truthful; simplify filtered-empty states while retaining explicit no-data states; label zero system aggregates `0 critical/high`; and mark exactly capped 500-item client results as `500+`/loaded without changing sub-cap counts or APIs. Verify with scoped web-ui rustfmt, scanning unit tests, package check, and diff check only.

2026-09-16 final distributed scanner audit remediation: (1) fix `BuilderCapabilities` ownership by cloning at task/message boundaries and retain spawned build/scan tasks in a shutdown-aborting `JoinSet`; (2) permit remote scan claims only for schema-1 post-build work whose successful producing build belongs to the same builder and exact session, remove cache-push-as-materialization proof, and retain the delayed server-local fallback for post-build plus all manual/fleet scans; (3) place every distributed-scanner `nix`, `nix-store`, and `vulnix` child in a dedicated Unix process group, synchronously kill the group and reap the direct child on timeout, revocation, cancellation/future drop, and shutdown, with descendant regressions; (4) centralize canonical direct-child `/nix/store/<basename>` syntax and canonical result bytes/digest in `cf-protocol`, enforce paths on builder and server, verify each locally available package output's deriver against its submitted package `.drv`, and persist `server_local_verified` versus `unverified_remote` closure provenance; (5) pass the locally probed scanner identity into execution and reject any exact claim name/version mismatch; (6) update focused backend docs/tests without touching Web UI; (7) run scoped rustfmt, `cf-protocol`/`cf-builder`/`cf-server` compile and tests, the isolated CVE database helper, and a bounded path-flake builder package build. Do not run broad flake or Web UI checks.

2026-09-16 three remaining distributed-scanner P1 findings: (1) preserve stale-heartbeat recovery by allowing capability persistence for only authenticated current enabled/registered sessions in active or offline state, while retaining disabled, unregistered, and stale-session fences, with a migrated database regression; (2) when expired, superseded, or retryable typed remote work returns to pending, move remote execution identity into audit metadata and clear typed execution, lease, sealed claim-input, outcome, and failure ownership fields before a server-local claim, with a remote-recovery/local-claim/remote-recovery race regression; (3) separate locally provable package-output deriver validation from top-level target closure membership, so an unavailable target yields unverified_remote only after every locally present submitted package output is checked against its submitted drv, with matching/mismatch/unavailable tests; (4) run scoped rustfmt, cf-server check and focused unit tests in the Nix environment, then the isolated migrated CVE database suite. Do not edit or run Web UI or broad flake checks.

2026-09-16 migration immutability correction: restore migration 0263 byte-for-byte to the version already applied by the isolated preview database, move later scanner identity/evidence constraint additions into additive migration 0264, rerun migrations/CVE regressions as applicable, and restart the preview without resetting or directly mutating preview data.

2026-09-17 maintainer-authorized workflow completion in the existing TASK-440 worktree/branch: (1) keep the verified isolated preview on ports 8080/3445/3042 current; (2) wire fleet rescan first, then exact derivation and distinct current/history actions through the canonical pending scan lifecycle with idempotent scan IDs and admin/CSRF checks; (3) reproduce scoped Config Root early and classify the exact request/worker/carrier/rendering failure before changing its source contract; (4) add only the next free additive migration for bounded immutable per-execution diagnostics if required, preserving migrations 263/264 and optional compatibility for deployed builders; (5) expose bounded redacted scan detail/attempt data and implement the real drawer; (6) correct scoped observations to use the exact published immutable source when proven, retain carrier verification and legacy compatibility, and preserve shallow/lazy behavior; (7) use isolated PostgreSQL and focused host/browser/real-Nix verification, then combined focused VM workflows at stable candidate; (8) review and push independently reviewable scanning and Config commits without merging or deploying.

2026-09-17 rescan-actions slice in the existing TASK-440 worktree: (1) extend scanning read DTOs with the durable derivation ID and persisted source_trigger; derive system-current from the latest reported store path and constrain system history to the exact system flake/configuration; (2) add one admin+CSRF exact-derivation POST contract that atomically inserts a canonical pending/manual cve_scans row or returns the active scan identity without executing vulnix; (3) expose fleet eligible/enqueued/reused counts while retaining its canonical pending/fleet lifecycle; (4) wire Scanning page fleet, row, exact system-current, and exact system-history actions with pending state, queued/reused success feedback, returned identities, actionable errors, and read refreshes; (5) add focused database/route/UI tests for auth, CSRF, idempotency, exact scope, response shape, source trigger, and UI pending/success/error behavior; (6) run scoped rustfmt, checks and targeted tests through nix develop plus git diff --check, with no full VM, migration, Config observation, scan log/diagnostic, stash, staging, commit, or push work.

2026-09-17 independent rescan P1 correction in the existing dedicated worktree: (1) constrain deployed row and count queries to the active system's exact flake while preserving content-addressed store-path matching; (2) add an explicit `rescan_eligible` DTO field derived from non-empty store-path availability, disable unbuilt row actions, and exclude unbuilt rows from history bulk requests; (3) make exact enqueue eligibility, active-row reuse, and pending insertion one transaction under the repository's established POA&M derivation lock order so claim/completion cannot create a false conflict response; (4) restore nullable commit joins for standalone NixOS rows in All scans; (5) add focused migrated database, Rust/UI, and browser-contract coverage for cross-flake deployed identity, unbuilt eligibility/actions, concurrent terminal transition serialization, and standalone rows; (6) run scoped rustfmt, targeted cf-server tests including isolated PostgreSQL coverage, web-ui unit/check or static browser contracts, and `git diff --check`. Do not touch generated Tailwind, stash, Config/log work, staging, commits, or remotes.

2026-09-17 bounded actual CVE scan diagnostics/log-details slice from exact HEAD dde14639 in the existing dedicated worktree: (1) add only migration 0265 with a separate append-only per-execution diagnostic event relation keyed by scan and execution attempt; do not modify released migrations 0263/0264 or authoritative vulnerability evidence tables; (2) add optional defaulted protocol diagnostics to remote complete/fail payloads so older builders remain accepted, and preserve existing claim priority, affinity, session fencing, evidence digest, and independent build/scan outcomes; (3) capture bounded local and remote vulnix lifecycle, successful stderr, process failure, and timeout context, then apply the existing snapshot_redaction policy before persistence and API output; (4) make diagnostic append/finalization ownership checks occur in the same scan terminal transactions where applicable, with regressions proving stale local/remote execution or session tokens cannot append or finalize; (5) add an admin-only fixed-limit scan detail endpoint with attempt identity, source, level, timestamps, and truncation metadata; (6) add a Dioxus Scanning log drawer with loading, empty, loaded, error, retry, refresh, and close states from real API data, plus focused Rust and 16c Playwright coverage; (7) update maintained protocol/API documentation and SQLx metadata if required; verify migration/database queries, protocol, builder, server worker/redaction/routes, Web UI tests/check/build, focused 16c workflow if feasible, rustfmt/rustdoc, and git diff --check through Nix. Preserve untracked tailwind.css and perform no stash, stage, commit, or push operations.

2026-09-17 CVE diagnostics P1 review corrections in the existing locked worktree at HEAD dde14639: (1) replace timeout-time stderr task abortion in both remote-builder and server-local vulnix runners with a bounded shared capture that can snapshot bytes already read after process-group termination without waiting for pipe EOF; include local successful-exit JSON parse failures with retained stderr and preserve redaction before persistence; add focused bounded/redacted actual-output tests; (2) add a monotonically increasing scan-detail request generation token so overlapping requests for the same scan and close/reopen cycles cannot publish stale state, with focused pure state-token tests and browser coverage where practical; (3) extend only migration 0265 with repository-consistent trigger semantics rejecting direct UPDATE and DELETE of diagnostic events while leaving normal INSERT and parent scan lifecycle behavior explicitly tested; (4) update multi-builder and backend API documentation for optional/defaulted diagnostics, caps/redaction, evidence-digest independence, old-builder compatibility, admin authorization, fixed-limit response shape, truncation, and lack of pagination; (5) run scoped rustfmt, targeted protocol/builder/server/web-ui tests and checks in nix develop, applicable rustdoc, focused web-ui workflow if feasible, and git diff --check. Do not modify migrations 0263/0264, generated tailwind.css, stash, staging, commits, or remotes.

2026-09-17 final diagnostics P1 remediation: (1) move the established cf-builder credential redaction policy from the builder binary into a shared builder module and apply it to CVE diagnostic construction before complete/fail request serialization while retaining server redaction; add serialized request assertions; (2) represent server-local vulnix execution failures with typed bounded stderr so timeout output survives the public runner boundary and is persisted as a separately redacted diagnostic, with worker/database coverage; (3) remove terminal local failure attempt increments so claims alone advance attempts, verify legacy tokenless callers and local/remote attempt parity; (4) expose immutable diagnostic event IDs through server and Web UI DTOs and key Dioxus rows by ID, with same-time multiline identity coverage; (5) run scoped rustfmt, protocol/builder/server/web-ui focused tests and checks in the Nix environment plus git diff --check, while leaving migrations 0263/0264, generated tailwind.css, stash, flake_timeline.rs, coach_panel.rs, staging, commits, and remotes untouched.

2026-09-17 measured shallow Config interaction pass authorized at starting HEAD `7f49d8bb41ae0b86c5fa1ac8e6d375e7f134dfd7`: (1) reproduce root/prefix/option measurements with the packaged evaluator and service-shaped immutable-source environment, recording executable/version differences; (2) reuse the task-owned isolated preview and keep it current; (3) extract one shared bounded direct `nix eval --json` shallow implementation for root, prefix, exact option, and selected definition locations, with exact immutable-source identity, dedicated cross-process capacity, deadlines, bounded output, process-group cleanup, execution fencing, and newest-first catch-up; (4) attach optional bounded root data to each production-style primary configuration result or launch a root-only per-configuration fallback immediately, persist it independently, and make observational reads depend on per-configuration readiness rather than whole-commit completion while leaving policy/build/deployment authority unchanged; (5) preserve lazy full inventory, configured index, provenance, incremental build dispatch, cache coalescing, authorization, redaction, and compatibility; (6) make the Dioxus Explorer render cached roots immediately, poll once before delay, retain loaded rows during localized requests, and fence revision changes; (7) add focused real-Nix, isolated PostgreSQL streaming/cache/security/capacity/ordering tests plus browser coverage and timings; (8) update Config architecture documentation, perform independent review, commit reviewable slices, and push normally without merge, deploy, live DB mutation, rebase, or force-push.

2026-09-17 Config Explorer presentation-parity pass from exact local/remote HEAD `9562fe09e94de3c790cd512ce7c77cc819bc0779`: (1) preserve the shallow observation request/cache contract and make branch and option lifecycle state render in the affected row and selected inspector without new observation kinds or eager requests; (2) use structured `path_components` for tree leaf labels, scoped flat ancestry/leaf presentation, cache identity, and selection, while retaining certified full-path identity where the API supplies only a qualified string; (3) add persistent selected-row styling, fixed tree/inspector scrolling, reference-aligned geometry, type-derived value presentation, and a truthful Option/Provenance inspector hierarchy; (4) extend only existing isolated browser fixtures and focused Config workflows for delayed branch/option states, local failure/retry, empty and failed values, long paths, request counts, keyboard selection, and wide/light/dark/narrow screenshots; (5) keep generated CSS current, verify the task-owned isolated preview and intended assets, run scoped rustfmt/unit tests, WebAssembly/web-ui package checks, browser syntax and focused authoritative workflows, then review, commit, and push normally to MR !323. No evaluator, policy, scheduler, source-identity, database, baseline, deployment, or stash changes.

Maintainer verification adjustment: do not run the authoritative `checks.x86_64-linux.web-ui` VM locally and do not monitor the post-push pipeline. Run focused frontend tests, formatting, browser fixture syntax, and the WebAssembly/web-ui package check only; commit and push the reviewed slice, then leave broad browser/CI verification to the maintainer and CI.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-17 shallow Config root slice committed and pushed as `9562fe09` (`TASK-440: Publish shallow Config roots`) to `origin/TASK-440-system-config-flake-parity`; local and remote heads match exactly. MR !323 remains open. Exact-head pipeline 2859062860 is running and owns the maintainer-requested broad flake gate. TASK-440 remains In Progress until CI and review complete. The only worktree residue is the preserved generated untracked `packages/web-ui/assets/tailwind.css`; the preserved scanner stash remains unchanged.

Presentation-parity preflight recorded exact local HEAD and `origin/TASK-440-system-config-flake-parity` at `9562fe09e94de3c790cd512ce7c77cc819bc0779`. Dedicated worktree is `/home/mcamp/code/crystal-forge/TASK-440-system-config-flake-parity`; recognized generated `packages/web-ui/assets/tailwind.css` and scanner stash remain untouched. The `dev` worktree has unrelated untracked `session-ses_f927.md`; no `main` worktree is registered. Existing task preview session `task440-preview` is alive from this task worktree. This pass is restricted to Config Explorer frontend presentation, directly related CSS, fixtures, and tests.

Maintainer explicitly requested no local full Web UI check and no CI monitoring for this presentation pass. The maintainer will watch CI and deploy.

2026-09-17 Config Explorer presentation-parity slice committed and pushed as `bf27c03d` (`TASK-440: Align Config tree and inspector presentation`) on MR !323; local and remote heads match. Branch loading now renders in the affected branch row's value cell, selected-option loading renders in that option's value cell and the persistent inspector, and no generic status block is inserted during expansion. Collapsing a loading branch no longer duplicates its request, and option detail state became path-local so a second selection cannot discard the first result. Browse rows show the final structured path component with the full qualified path in the title, while Configured, observed-search, and certified rows keep the qualified path with muted ancestry and an emphasized leaf. Selection identity is passed into every row list, with persistent accent styling for scoped and certified rows. The tree and inspector use the reference's bounded 58vh scrolling, fixed row height, depth-independent grid columns, and 1000px stacking breakpoint. Values are classed from the typed payload rather than string heuristics, so known false, zero, null, empty string, and empty list stay distinct from not-inspected, unavailable, and failed states. The inspector uses the reference Type/Value/metadata/Provenance/footer hierarchy while preserving is_defined and highest_prio as secondary metadata, and it reports definition totals only from the current response. Accessibility correction from real browser evidence: status text nested in a row button is dropped from the accessibility tree, so row indicators are now visual and one shared polite live region announces the current scoped operation.

Verification against the task-owned isolated preview at http://127.0.0.1:8080 (API 127.0.0.1:3445, PostgreSQL 127.0.0.1:3042): `12m-task440-config-explorer-keyboard-wide` 1/1, `12n-task440-config-narrow-keyboard` 1/1, `12la-task440-partial-config-inventory` 1/1, `13j-task440-flake-states-panes-navigation` 1/1, `14d-task440-cross-surface-auth-navigation` 1/1, with 10 dark/light state captures including branch-loading-in-row, selected-option-loading, selected-option-populated-tree, branch-continuation-failure-retry, and narrow-selected-option. Also passed: `cargo test --manifest-path packages/web-ui/Cargo.toml` (431 passed, 1 ignored), scoped rustfmt on the changed component, `node --check` on the browser suite, and `git diff --check`. Request-count assertions prove rendering fetches no option value or provenance that was not explicitly selected. Per maintainer instruction the authoritative Web UI VM check was not run locally and CI was not monitored; pre-existing rustfmt differences in `flake_timeline.rs` and `coach_panel.rs` and the untracked generated `tailwind.css` remain untouched.
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

author: @openai-agent
created: 2026-09-16 20:45
---
2026-09-16 distributed CVE server hardening follow-up: audited remote failure, requeue, completion, and legacy recovery transitions against migration 0263. Remote requeue and failure clear typed lease ownership; completion retains immutable provenance. Tightened nullable PostgreSQL CHECK branches so scanner identity/version and completed result evidence cannot pass through SQL NULL semantics. Disabled or inactive builders now receive `410 Gone` on claim, and capability persistence during session establishment fails closed if the session is replaced or the builder becomes inactive. Added disabled-claim regression coverage and corrected helper/API contract documentation. Verification passed in the Nix environment: SQLX_OFFLINE offline cargo check for cf-protocol/cf-builder/cf-server libraries; cf-protocol builder tests (14); cf-builder CVE scanner tests (5); cf-server builder handler tests (55); CVE lease module tests compiled and its two non-DB tests passed; cargo fmt check; git diff check. The DB-backed remote lease test returned early because `CRYSTAL_FORGE_TEST_DATABASE_URL` is unset, so migration and live transition behavior remain unverified. Existing repository warnings and ignored Nix eval-cache busy warnings remain. Task stays In Progress; no commit, push, MR update, broad flake check, or authoritative Web UI check was performed.
---

author: @openai-agent
created: 2026-09-16 20:59
---
Correction to comment #18: live migration and transition behavior is now verified. I initialized a fresh disposable PostgreSQL cluster at `/tmp/opencode/task440-cve-pg-35440`, applied the repository migrations through `nix run .#devScripts.runCveProcessingTest`, and ran every enrolled CVE processing regression. All invocations passed with no `FAILED` or `error: test failed`, including remote claim/session fencing, environment authorization, build-priority and cache-gated fallback, atomic post-build enqueue, digest idempotency/conflict, failure/requeue/recovery transitions, affected/whitelisted persistence, concurrent claims, and POA&M/composite lock-order tests. Final verification also passed: `nix develop --command cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check`; offline `cargo check` for `cf-protocol`, `cf-builder`, and `cf-server` libraries with `SQLX_OFFLINE=true`; offline no-dependency rustdoc for those three libraries; and `git diff HEAD --check`. Existing repository compile/rustdoc warnings remain outside this slice. The disposable PostgreSQL server was stopped and confirmed inactive. No commit, push, MR update, broad flake check, or authoritative Web UI check was performed. TASK-440 remains In Progress because acceptance criteria #24 and #27 are still open.
---

created: 2026-09-17 08:13
---
2026-09-17 CVE diagnostics P1 follow-up: resolved timeout/failure stderr capture for remote and local scanners, including bounded post-termination draining and parse-failure stderr; added generation fencing for overlapping same-scan refreshes and close/reopen requests; made diagnostic event rows append-only while preserving parent-scan cascade cleanup; and documented builder compatibility, bounds, redaction, digest independence, and the admin scan-detail API. Verification passed: focused cf-builder timeout test; cf-server vulnix runner and diagnostic preparation tests; Web UI scanning unit tests and cargo check; offline cf-builder/cf-server all-target checks; default and Web UI fmt checks; JavaScript syntax check; cf-protocol/cf-builder/cf-server rustdoc; full `nix run .#devScripts.runCveProcessingTest` against a fresh disposable PostgreSQL cluster with migrations through 0265, including direct UPDATE/DELETE rejection and cascade cleanup; and the authoritative `16c-scanning-view` NixOS browser workflow via an explicit `path:` flake reference, with release WASM, deterministic overlap and close/reopen race assertions, and dark/light screenshots (1/1 passed). The first Git-backed Web UI build did not reach the VM because Nix excluded the intentionally untracked diagnostic module; rerunning via `path:` included worktree files without staging. Existing compiler/rustdoc warnings remain. The disposable PostgreSQL server was stopped and port 55439 confirmed inactive. No files were staged, committed, or pushed; migrations 0263/0264 and generated `packages/web-ui/assets/tailwind.css` were not modified.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Hardened V2 config snapshot reads with fail-closed corruption handling, comparison-unavailable state, literal LIKE search escaping, structured path identity coverage, provenance/global-unavailability coverage, query bounds, and side-effect regression checks. Verified targeted Rust tests, Nix config-inspector and evaluator-snapshot-isolation checks, formatting, diff checks, and cargo check. Committed and pushed as deb37a5d6f9653fda55ec12a36abfa75425d4986.
<!-- SECTION:FINAL_SUMMARY:END -->
