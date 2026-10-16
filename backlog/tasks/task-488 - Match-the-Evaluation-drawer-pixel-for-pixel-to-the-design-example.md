---
id: TASK-488
title: Match the Evaluation drawer pixel-for-pixel to the design example
status: In Progress
assignee:
  - opencode-claude
created_date: '2026-10-16 00:27'
updated_date: '2026-10-16 05:59'
labels:
  - web-ui
  - design-parity
  - evaluations
dependencies: []
references:
  - packages/web-ui/src/views/evaluations.rs
  - checks/web-ui/design-parity/manifest.json
  - checks/web-ui/design-parity/compare-design-parity.js
  - TASK-466
  - TASK-441
  - TASK-447
  - TASK-386
documentation:
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/styles.css
  - docs/design/CrystalForge/serve.sh
  - docs/design/CrystalForge/fixtures/README.md
priority: high
type: enhancement
ordinal: 510000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Goal and approved scope expansion
Bring the complete Evaluation drawer into functional and visual parity with the checked-in EvalDrawer design, using real evaluation data. The user's detailed specification supersedes this task's original presentation-only restriction and its exclusion of the bundle matrix. Backend API and additive persistence work are now in scope. Do not modify the design to hide gaps or manufacture production results.

## Reference and control semantics
Reference: docs/design/CrystalForge/components/EvalDrawer.jsx and styles.css. The user describes two screenshots but no attachments are available in this conversation; request them for final exact-target comparison. The source reference defines a control as a distinct evaluated policy identity in the deduplicated union of applicable bundle policy memberships. Use exact policy-version identity, not display labels. Framework requirement satisfaction is a different model: a policy can map to multiple requirements and a requirement can need multiple policies. Do not claim requirement compliance merely from a policy count.

## Required behavior
Implement Bundle/Control column modes, dense defaults, bundle rollups and drilldown, total unique and shared-control metadata, per-system passing/applicable health, consistent bars and unknown/incomplete coverage, descriptive exact-version bundle metadata, distinct row/cell actions, state-preserving filters/sort/mode transitions, large-set horizontal scrolling and sticky identity/header cells. Preserve all existing working actions, loading/error states, report-only semantics, cancellation and retry behavior. Unknown, stale and missing outcomes must never become pass or not-assigned solely because a result is absent.

## Backend correctness
The current flat matrix DTO and mutable derivation results cannot prove historical bundle membership. Capture the actual resolved evaluation plan before evaluator execution, without resolving current catalog state again later. Preserve exact represented attempt, planned system/configuration identity, eligible policy versions, bundle versions and assignment-version provenance, and explicit empty/conflict/unavailable resolution states. Separate applicability from outcome availability. Keep shared-configuration evaluator deduplication and per-system provenance. Read capture and outcomes consistently, validate outcome attempt markers, and retain planned systems with partial or absent results. Existing evaluations without capture remain explicitly legacy/unavailable; never reconstruct historical assignments from today's catalog.

## Architecture and coordination
Server owns persistence, authorization and evaluation policy semantics; UI uses typed DTOs and pure aggregation/state helpers outside markup. Preserve current authentication, security, evaluator scheduling, deployment gating, scanning and cache behavior. Add new migrations only; current task base's highest migration is 0302, so recheck before selecting a next version. SQLx metadata must match any checked query changes. TASK-466 has overlapping unfinished work and a conflicting 0269 migration in a separate worktree. Inspect it read-only as a reference; do not overwrite that worktree or blindly copy old backend/UI files. TASK-441 owns dependency-graph domain corrections; do not change graph domain semantics to imitate arbitrary reference fanout.

## Verification
Keep TASK-488's isolated preview and its fixture data available. Backend work must restart the task backend before browser checks. Use pure aggregation/status/state tests, migration-backed snapshot/attempt/history/authorization tests in an isolated disposable test database, UI tests and WASM check, focused authoritative browser interactions and matched-viewport design/production captures. Fixtures demonstrate behavior but do not establish actual persisted bundle provenance. Do not report completion from successful rendering or existing unit tests alone.

## Risk and non-goals
High risk due to evaluation-time persistence and overlapping WIP. No blanket refactor, fabricated bundle membership, requirement-satisfaction redesign, weakened security/checks, edits to applied migrations, shared database access, production deployment, or automatic merge.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bundle and Control modes both exist and render columns derived from authoritative evaluation-time identities and memberships
- [ ] #2 Bundle cells show passing/applicable counts with distinct failure warning and unavailable states and drill down to constituent control results
- [ ] #3 Total unique controls and controls shared by multiple real bundles are computed from exact identities without counting synthetic groups as real bundle overlap
- [ ] #4 System health counts each applicable control once regardless of overlap; pass counts and bar denominator agree; unknown unassigned and missing results never count as pass
- [ ] #5 Exact bundle and policy version names and framework metadata come from captured evaluation data rather than names hardcoded from the design
- [ ] #6 Filters counts worst-first and name sorting survive column-mode transitions; row expansion and bundle-cell drilldown have distinct actions
- [ ] #7 Dense matrices default to readable bundle columns and support Controls horizontal scrolling with sticky system identification and matrix headers
- [ ] #8 Planned systems remain visible during partial evaluation; missing results and incomplete applicability are explicitly unavailable rather than falsely clean
- [ ] #9 Historical catalog or assignment edits cannot change an evaluation's captured memberships; retry reads cannot combine provenance and outcomes from different attempts
- [ ] #10 Successful empty assignments no-applicable-control bundles legacy unavailable metadata and resolution conflicts are represented truthfully
- [ ] #11 Drawer header metrics actions top issues filters summaries legend spacing and typography match the design without losing existing actions
- [ ] #12 Backend tests cover capture atomicity attempt ownership stale results overlap history immutability partial rows authorization and legacy behavior
- [ ] #13 Pure frontend tests cover aggregation denominator overlap statuses filters sorting focus and mode transitions
- [ ] #14 Authoritative browser workflows cover bundle/control modes bundle and row drilldown large fixtures loading errors partial results and preserved actions
- [ ] #15 Matched-viewport design/production comparison includes dark light and narrow states and explicitly reports remaining differences
- [ ] #16 Changed-file formatting UI unit tests WASM backend checks applicable SQLx metadata and git diff checks pass; unrelated baseline failures are reported
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Preview URL, review path, backend freshness, and data mode reported at handoff
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit current resolver/evaluator persistence, matrix API, drawer, source design and TASK-466 WIP read-only. Record control identity and historical/partial-result limitations.
2. Define and test pure, explicit aggregation semantics and additive DTO contract: unique policy-version controls, per-system applicability and outcome states, bundle memberships, attempt/provenance availability. Do not infer bundle memberships from legacy labels.
3. Implement new attempt-scoped capture persistence with additive migration and exact resolver provenance; capture before execution under existing attempt/cancellation guards. Preserve resolver merge and evaluator semantics.
4. Implement consistent snapshot reads and API projection, preserving existing fields and truthful legacy/partial rows. Add isolated DB regressions and exact runner membership.
5. Wire typed pure model and state transitions into bundle/control drawer components; port source design CSS and preserve all actions. Keep preview current after each coherent batch.
6. Verify full scenarios and browser interactions using real persisted data; compare same-viewport design and implementation, record every discrepancy and required screenshot gap. Do not commit/push/open MR without authorization.

Backend delegated implementation: retain the existing task lock/worktree and running preview. Add 0303 JSONB first-write attempt capture; resolve and load eligible policies in a caller-owned repeatable-read transaction; retain per-system resolution and exact authoritative bundle/assignment version provenance before execution. Add typed additive matrix DTO and repeatable-read outcome projection with attempt-marker validation, pure status tests and disposable-DB regression tests. No frontend edits, service operations, builds, commits or pushes; owner coordinates verification and backend restart.

Check-wiring delegation: in the existing locked TASK-488 worktree, register self-contained 26e-evaluation-policy-bundle-controls with browser-local queue/log fixtures and the existing structured matrix workflow; capture intermediate states and clean drawer/routes. Add the four new ignored capture regressions to the exact-name guarded server runner and document verified membership counts. Limit writes to the six user-owned check files; run only cheap syntax/static consistency checks. Parent owns preview, backend/frontend implementation and VM verification.

Focused header delegation: limit writes to evaluations.rs and optional new evaluations/drawer_header.rs. Reuse parent-owned preview. Add shared expand/restore state, confirmation before existing force-cancel handler, and truthful completed/queued UTC DTG timestamps using escaped RSX. Preserve action ownership and parent policy work. No builds, service operations, database operations or commits; parent owns CSS and runtime verification.

Focused live-matrix correctness delegation: only evaluations.rs and coordinated eval-policy-matrix-workflow.js. Key both PolicyTab callsites by commit, react to commit/live props, use component-owned browser five-second polling with cancellation on completion/unmount and retained displayed response during refresh, and key captured presentation state by attempt. Add browser-local matching-attempt unknown-to-pass polling assertions with 12-second assertion bounds and preserved filter/sort/health. Parent owns preview/build/runtime verification; no service/build/DB/commit operations.

Follow-up lifecycle synchronization limited to evaluations.rs and optional pure drawer snapshot helper: subscribe to selected target plus existing queue/history signals; refresh only exact commit identity with nondecreasing attempt number; prefer terminal same-attempt history for authoritative completion fields, otherwise retain terminal queue evidence; allow newer retries to return History->Queue and preserve selection on absent/older candidates. Compare PartialEq before setting. Add identity/isolation/retry/completion/old-attempt/missing unit cases. No extra requests/builds/services/DB/commits; parent retains header/CSS ownership.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude on sledge in /home/mcamp/code/crystal-forge/TASK-488-eval-drawer-pixel-parity

Preflight: branch TASK-488-eval-drawer-pixel-parity was created from the fetched origin/dev at 33de73b013b735304dfac9c69745a074a3e76ebd (merge of TASK-470). Local dev was not used as the base because it is 544 backlog-only commits ahead and 10 behind origin/dev. The new worktree is clean, 0 ahead and 0 behind origin/dev, and its upstream was unset so a bare push cannot target dev. The dev integration worktree is clean except for the owner-preserved untracked TASK-478.lock and session-ses_f927.md. No main worktree is registered. Task selected by the user on 2026-10-16 ("do what needs to be done to get to work on doing a focused pixel for pixel exact matching of the design eval drawer"); this was treated as moving the task to To Do and starting it.

Coordination: TASK-466 (policy-matrix data and CSS) has 17 uncommitted files in /home/mcamp/code/crystal-forge/TASK-466-eval-policy-matrix-bundles, including packages/web-ui/src/views/evaluations.rs and assets/app.css, and a migration numbered 0269 that now collides with origin/dev's 0269_scanning_lifecycle_archive.sql. This task does not touch that worktree. Expect rebase conflicts in evaluations.rs and app.css when TASK-466 lands.

Preview started on 2026-10-16 from /home/mcamp/code/crystal-forge/TASK-488-eval-drawer-pixel-parity with `nix develop .#devenv --command devenv up -d` (log /tmp/opencode/task488/devenv-up.log). `devenv processes list` reports api, postgres, and web as ready with 0 restarts. Process cwd values are inside this worktree: PostgreSQL 17 pid 31192 (data directory .devenv/state/postgres, 127.0.0.1:5432), cf-server-core pid 31379 (0.0.0.0:3445), dx 0.7.3 pid 31569 (127.0.0.1:8080). Browser URL http://127.0.0.1:8080/. GET / returned 200 on :8080 and GET /status returned 200 on :3445. The API seeded the design golden fixture (35 systems, 48 CVEs, 5 builders, 175 system events) with mock execution mode. Local fixture login is the devenv bootstrap admin from devenv.nix (a development fixture credential, not a real one).

Known gaps at preview start: (1) the API listener binds 0.0.0.0:3445 although the generated config says server.host 127.0.0.1; this was observed and not changed, and no firewall change was made. (2) The seed log says build_jobs and hardening seeding were skipped (needs derivations FK), so the evaluation queue, history, log, policy, and graph data for the drawer may not match the design fixture. This must be checked before any pixel comparison. (3) Login, the /evaluations route, and an open drawer have not yet been verified in a browser. (4) ImageMagick (identify, compare) is not on the default PATH; the parity harness needs it inside nix develop.

First implementation batch: added drawer-scoped header typography/spacing, 11px monospace identity metadata, design evaluation glyph, design 12.5px timestamp typography, native tab font metrics and 11px/1.55 log metrics. Existing cancellation, retry, polling and policy state logic is unchanged. Only evaluations.rs, icon.rs, app.css plus the new capture-eval-drawer-shell.cjs and eval-drawer.md are changed. No commit or push.

Browser baseline verified local fixture login HTTP 200 and real Completed evaluation drawer. Reference is the repository app's Nix-built offline design at /nix/store/97akjbrnqsvrmg868cf6m1z61x6bvk5i-cf-design-example-offline. New comparison mounts the unchanged reference component with matched queue identity and intercepts only production queue/log responses in an isolated browser context. It closes Coach using its real accessible button when present; asynchronous saved preference can remove Coach, so the normal interaction is retried for that race. No force clicks or hidden screenshot differences.

Focused browser command: NODE_PATH=/nix/store/fg778x0631ld4xd8fim2kbvxll5715xp-playwright-test-1.59.1/lib/node_modules PLAYWRIGHT_BROWSERS_PATH=/nix/store/f0rap655j6wmqbfvqdw445kwcxkxwf7n-playwright-browsers /nix/store/l07gdbxylzfl8pbx9pxy6fyg95w4hjwy-nodejs-24.19.0/bin/node checks/web-ui/design-parity/capture-eval-drawer-shell.cjs /nix/store/97akjbrnqsvrmg868cf6m1z61x6bvk5i-cf-design-example-offline /tmp/opencode/task488/shell-first-pass. Actual exit 0: desktop dark, desktop light, 720px dark all pass selected typography, metadata color and header/tab height assertions. Paired screenshots and JSON measurements are saved there. This does NOT verify full pixel parity or authoritative VM coverage.

Verification: changed-file `rustfmt --edition 2024 --check .../icon.rs .../evaluations.rs` passed through nix develop. Initial cargo test --lib command was invalid because web-ui is a binary target; corrected `CARGO_BUILD_JOBS=2 cargo test --manifest-path packages/web-ui/Cargo.toml` passed 677 tests, 0 failed, 1 ignored. Subsequent `CARGO_BUILD_JOBS=2 cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed. Combined command actual exit 0; retained output /home/mcamp/.local/share/opencode/tool-output/tool_14237dab6001kYEX6qvhQErToO. Package-wide cargo fmt check fails on pre-existing caches.rs/cves.rs/poams.rs formatting; those files are untouched (existing TASK-371.1 tracks formatting drift). git diff --check passed.

Preview remains available at http://127.0.0.1:8080/evaluations. Task-owned frontend was restarted using devenv processes restart web and the new glyph/styles were verified in browser. Backend and database were not restarted. Remaining visual differences include timestamps/DTG rendering, status wording, action availability and missing expand/confirmation controls, development chrome offsets, log toolbar metrics and empty-state differences. Policy/graph comparison, populated log fixtures, pixel-diff reporting and authoritative browser integration are still pending. TASK-488 remains In Progress.

User requested 0.0.0.0:8080. Applied runtime-only devenv process overrides: original web exec with --addr changed to 0.0.0.0; API exec allows exact CORS origin http://10.8.0.177:8080. No repository config or firewall changes. Restarting a single process did not reload its exec override. Restarted only this task's stack; one restart timed out, then verified task-owned manager cwd and stopped it gracefully before evaluating ports with listeners free. Final startup succeeded; frontend PID10773 listens 0.0.0.0:8080 and LAN-address GET returns 200. Existing task database preserved. Runtime-only overrides must be retained when restarting the stack; normal default startup reverts to loopback.

Network preview URL: http://10.8.0.177:8080/evaluations (host sledge). Exact-origin CORS preflight succeeds. Browser login over LAN HTTP remains blocked: login returns HTTP200 with a Secure Set-Cookie, but Chromium stores no cookie on non-loopback plain HTTP. No cookie security was weakened. Need HTTPS for direct network login, or SSH-forward UI8080 and API3445 and browse localhost. Loopback browser preview remains the supported authenticated review path.

User requested persistent mock data for reviewing the evaluation view/sidebar. Verified PostgreSQL through SQL as mcamp: database crystal_forge, port5432, data_directory /home/mcamp/code/crystal-forge/TASK-488-eval-drawer-pixel-parity/.devenv/state/postgres. Seeded only that cluster, in one guarded transaction, using /tmp/opencode/task488/seed-eval-preview.sql. No application or migration source changed for this request.

The built-in seeder does not import evaluation scenarios; its dotfiles registry seed had 500 imported commits being consumed continuously by mock execution. The overlay preserves all 500 original commit JSON rows/archive flags in task488_preview_archive_backup, archives them from the active queue, and retires the one imported in-progress mock row. No imported commit was deleted. Existing golden infrastructure/web-services/edge-gateway/build-farm/lab-nodes flakes were reused. Added 35 clearly labelled TASK-488 preview fixtures: 1 in_progress, 4 pending, 15 complete, 10 failed, 5 cancelled; 310 NixOS policy/closure snapshots without executable derivation/store paths; 560 [MOCK PREVIEW] log lines; branch snapshots and 10 failed-evaluation attention occurrences.

First two seed transactions rolled back on the actual live schema's unique idx_commits_single_in_progress index. Contrary to an earlier exploration inference, its predicate includes BOTH in_progress and cancelling. The successful overlay respects the constraint and shows one running plus four pending. No schema/index was weakened. Queued attempts have available_at seven days ahead, so the mock worker does not consume review fixtures. Use Cancel on the running row to exercise cancelling. Backend startup recovery changes active states, so reapply the guarded overlay after a backend restart.

Verification exit0: /tmp/opencode/task488/verify-eval-preview.cjs uses real API and browser, not intercepted responses. Confirmed mock execution mode, 5 active, 30 history entries, real logs, 4 policy columns with pass/warn/fail results, and 9 infrastructure graph rows with cached/pending counts. Browser opened active drawer Log/Policy matrix/Dependency graph and Completed history. Separate assertion confirmed failed infrastructure results include nix_eval_failure. Captures: /tmp/opencode/task488/mock-active-queue.png, mock-drawer-log.png, mock-drawer-policy.png, mock-drawer-graph.png, mock-history.png. Inspected queue/history screenshots.

Opening history during verification acknowledged the fixture badge. Removed exactly 10 test-created dismissals for review user c4579ef5-8bc1-4612-a8f9-356cd56281f0 and attention occurrences linked to TASK-488 fixture commits; unrelated dismissals are untouched. Final real API/browser verification exit0 confirms sidebar eval badge10, queue5, history30. Ready screenshot /tmp/opencode/task488/mock-preview-ready.png. Review through forwarded http://localhost:8080/evaluations. UI remains bound 0.0.0.0:8080. Limitations: static presentation snapshots, not a running evaluator/integration proof; fixture policy names do not establish published compliance bundle identities; graph uses current API's system-level closure model, not the design's arbitrary package fanout. Existing no-cache configuration banner remains visible rather than hidden.

User explicitly expanded TASK-488 to full bundle/control functional parity, including backend changes; the former presentation-only and bundle-matrix exclusions no longer apply. TASK-466 worktree remains untouched and locked to its existing record. Backend and frontend research completed read-only: current API lacks stable structured identities, historical bundle capture and separate applicability; current health bar denominator differs from displayed ratio. TASK-466 architecture is useful but skips captured rows without derivations, does not validate outcome attempt markers, reads independently, and mislabels applicable missing outcomes as not_assigned. Do not carry those defects forward. Source design defines controls as deduplicated policy identities, not normalized framework requirements. Two requested screenshots are not attached; final screenshot-specific parity cannot yet be verified.

Implementation now includes additive migration0303 and exact-attempt immutable JSONB plans; transactional real resolver/loader capture with per-system memberships/context metadata, including empty assigned bundles and genuine virtual baselines; a consistent MVCC matrix read validates outcome-attempt markers, preserves partial planned rows and UUID-bound detail strings, and keeps legacy provenance unavailable. No current-catalog historical reconstruction or fabricated production memberships. Capture excludes config/expressions/overlays from public metadata. New schema applied to the task preview by current backend startup; SQLx checks use separate disposable databases, not the review database.

Frontend now has structured Bundles/Controls rendering, deduplicated control health and overlap counts, bundle rollups/drilldown, typed mode/filter/sort state, unknown/N/A/legacy handling, detailed actual assertion strings, sticky identification/header and horizontal scrolling. Existing flat results remain an explicit legacy fallback, and global CF-agent evidence is retained separately rather than assigned a fake policy UUID. Added Expand/Restore and a confirmation before force-cancel; no force-cancel request occurs until confirmed. Header timestamps now use actual completion time or honestly label the available queue timestamp Queued (DTO lacks actual started_at).

Verification completed in this session: initial cf-server lib cargo check passed. Actual resolver/loader SQLx capture cases passed (2 migration-backed cases), plus plan immutability/attempt/read cases (2 migration-backed cases), along with pure projection/capture/handler tests. Current-source standalone logs /tmp/opencode/task488/backend-final-current-tests.log show capture group4 passed and query group10 passed. Subsequent Nix packaged server tests passed1805 with686 ignored. Native frontend latest header check passed701 tests,0 failed,1 ignored and WASM check passed (/tmp/opencode/task488/frontend-header-final-checks.log). New glyph/CSS-only edits followed that native run; packaged re-verification is still pending for the newest header/style edits.

Registered critical authoritative browser workflow26e-evaluation-policy-bundle-controls, using clearly marked browser-local 60-system/128-control/six-bundle/32-shared fixtures; no persisted results are altered by this test. The exact backend capture cases are added to server-regressions ignored membership (59->63). Fixed conditional test PKI/observer entries so non-cache focused web-ui checks can evaluate without changing enabled cache fixtures or assertions. Authoritative selected web-ui VM passed1/1 plus OSCAL/SARIF before latest header/style additions (/tmp/opencode/task488/authoritative-matrix-web-ui-final.log). First run was interrupted by tool1200s limit while building server tests, not assertion failure; extended foreground run passed. Do not label earlier VM pass as verification of subsequent edits.

Current full registered browser workflow including Expand/Restore, all matrix interactions, loading/error/empty/legacy transitions and force-cancel zero-requests-on-dismiss / exactly-one-on-confirm passed1/1 against fresh release preview via direct integration-test.js (/tmp/opencode/task488/direct-current-authenticated.log). The host web-ui-test preflight assumes development JS layout and waits on the release-mode bundle; use direct runner with pinned Node/Playwright plus CF_UI_TEST_USERNAME=admin, CF_UI_TEST_PASSWORD=password, CF_UI_API_BASE_URL=http://localhost:3445 and selected step26e until that runtime limitation is addressed. Do not change checks to hide it.

Preview freshness repair: old served hot-reload markup retained modal z-index90 despite source fixes. Replaced only the task frontend with a task-owned tmux session task488-frontend running /tmp/opencode/task488/start-preview-frontend.sh (Dioxus0.7.3 release mode, bound0.0.0.0:8080, fresh session cache, wasm-bindgen0.2.108 explicitly first on PATH). API/PostgreSQL remain devenv-owned; native API PID26917 uses current packages/default/target/debug/server. Verified real above-drawer modal class/z-index200 and normal Keep running click without force. Release CLI cannot combine --release with --hot-reload false, so use --release alone. Latest CSS refresh frontend rebuild is in progress; inspect /tmp/opencode/task488/frontend-release-preview.log and a fresh browser before claiming current-ready. Do not use devenv start/restart web while the separate frontend owns8080.

Direct matched-viewport reference audit completed1440dark/light and720dark, both modes. Known-result fixture values matched every system/bundle and all7680 control glyphs per case; this is browser-local test evidence, not persisted backend evidence. Actual image comparisons still differ. Audit artifacts /tmp/opencode/task488/AUDIT.md, fresh-audit-report.json and fresh-audit-*.png/json retain exact measurements. Improvements: bundle headerheight69.67 matches; narrow stats68.59 matches; controls toolbar shrank133.72->99.78px before newest native-button line-height fix. Remaining measured issues include extra version/year content, compact cell badge styling and intentional unknown/N/A/platform explanations; environment banners make drawer944px high versus reference1000px. Missing supplied screenshots still block exact requested-target comparison. No visual parity/completion claim.

Preview fixtures remain synthetic and labelled. They do not contain historical bundle capture; do not backfill fake provenance to make them appear real. Background poller ignores configured auto_poll=false and reimports/unarchives dotfiles; filed TASK-489 in Backlog rather than changing evaluation behavior. Task-only seed now preserves the dotfiles flake original row in task488_preview_flake_backup, soft-deletes only that imported demo flake to exclude it from polling, and retains original500commit rows/flags in task488_preview_archive_backup. Five design flakes remain active. Runtime seed replay fixed immutable evaluation_enqueued_at update by preserving it. No production/integration data touched. No commit, push, MR or merge performed.

Focused live-matrix fix implemented in evaluations.rs and coordinated eval-policy-matrix-workflow.js only. Both callsites key PolicyTab by commit and pass live (default false). A use_reactive(commit,live) resource owns sequential browser-only fetch/delay polling: immediate fetch, then 5 seconds after each response; live=false cancels prior future and fetches once; unmount drops request/timer futures. Commit-tagged displayed response survives in-flight refresh; explicit failures replace the success view and retry on live cadence. CapturedMatrix had no attempt key on inspection; added attempt UUID + attempt number key so same-attempt successful updates retain local presentation while changed attempts reset. Workflow adds browser-local initial legacy->captured discovery and same-attempt host00 unknown->pass, asserting 125/128->126/128 health, 93/96->94/96 bundle health, retained Failing/Name/Bundles/expanded evidence; polling assertions use 12000ms bounds. Existing loading/error checks retained. Nix devenv rustfmt changed-file check and node --check passed; git diff --check scoped to these two files passed. No build, browser workflow, WASM/native compilation, service restart, database operation or commit performed. Parent-owned preview localhost:8080/evaluations returns HTTP200; :30273 unavailable. New Rust logic is not runtime-verified in the served build. Important parent-coordination gap: drawer_target is a cloned Queue/History row and is not synchronized by current queue/history refresh effects, so terminal completion may not propagate live=false while the drawer stays open. Poller correctly cancels on supplied live=false, but automatic terminal propagation needs parent resolution.
<!-- SECTION:NOTES:END -->
