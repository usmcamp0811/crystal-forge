---
id: TASK-529
title: Show the real deployment failure reason in history and logs
status: In Progress
assignee:
  - automated-agent
created_date: '2026-11-17 14:00'
labels:
  - agent
  - server
  - web-ui
  - deployments
dependencies: []
priority: high
type: bug
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner report (chesty, 2026-11-17): every failed deploy shows "Deploy failed to activate" in the History tab and in the Logs tab. The Logs tab also shows made-up lines "deploy: evaluating configuration", "activation failed", and "deploy: rolled back to previous generation". The agent journal shows that the deploy failed in the cache copy step, before activation, and no rollback occurred.

The agent already reports a failure message (`POST /agent/deployment-failed`). The server stores it in `pending_system_deployments.failure_message` and in the `cf_deployment_failed` event metadata (`failure_message`). The History API and the agent-events log query ignore it and return a fixed title.

The stored message itself is not useful for chesty: `Authenticated cache copy failed (Nix failed for an unrecognized reason; details are redacted)`. `classify_read_failure` knows only five categories. The owner wants more fixed categories. Raw Nix stderr must not leave the agent because it can contain cache URLs and credential paths.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `classify_read_failure` recognizes more common Nix copy failures with fixed, credential-free categories: DNS resolution, connection refused or unreachable, network timeout, interrupted transfer, cache server error (every HTTP 500-599), rate limiting (HTTP 429), hash mismatch, local disk full, and Nix daemon or store permission problems. Raw stderr is never logged or reported. (Agent unit tests, farm build.)
- [x] #2 History shows a role-projected structured failure view (see #9). Its title and `change_reason` carry only role-permitted text. Legacy raw `failure_message` and `display_failure_message` metadata is never displayed or returned. (Supersedes the earlier text-only contract. Scenario `failure-reporting`.)
- [x] #3 The agent-events entry for a `cf_deployment_failed` event has level `error`, carries the same role-projected view, and exposes `event_id` and `deployment_id` for correlation. (Scenario `failure-reporting`.)
- [x] #4 The Logs tab shows one primary entry per recorded deployment event. A reconstructed line is dropped when a real agent event with the same event ID exists, for a start, a success or a failure. For a legacy History row with no real event, it shows one reconstructed line that restates only the recorded outcome. It never shows invented evaluation, build, activation or rollback steps for any deployment. Reconstructed lines for restarts, reboots and on-host rebuilds are unchanged and out of scope. (Browser checks cover failures. Success and legacy reconstruction have unit tests only.)
- [x] #5 Unit tests cover the new categories, the role projection, the legacy/partial/unknown report states, and the failure message formatting. (Farm builds of `cf-agent`, `cf-server-core`, `web-ui`.)
- [x] #6 `docs/knowledge/caches/niks3-cache.md` lists the stages, codes, roles, suppression rules and compatibility behavior. (`okf-knowledge` validation.)
- [x] #7 For complete stderr without a response-body delimiter, the agent keeps at most the last three masked `error:` lines. It masks every value it gave Nix for the read (cache URLs, Basic credentials, credential paths and their directories) with `****`, removes URL user information, and masks every URL query string. If a response-body delimiter appears or the capture is truncated or incomplete, it suppresses all detail. (Owner decision and second-review requirement, 2026-11-17. Agent unit tests.)
- [x] #8 Current agents report a typed stage, fixed cause and summary, available exit status and actual attempts for cache configuration and acquisition, generation creation and verification, activation submission and execution, and other deployment setup failures. Old agents, incomplete reports and current unknown causes remain distinguishable. (Agent unit tests for every stage. Real systemd test for activation execution.)
- [x] #9 History, agent-events and deployment-status enforce the existing global-role and environment-access model in the response JSON. Viewer gets stage, fixed summary and status. Operator adds cause, remediation, exit status and attempts. Only Admin gets bounded sanitized diagnostics. An unauthenticated caller gets 403 and an Operator without environment membership gets 404, with no failure content in either body. (Scenario `failure-reporting`, direct API reads.)
- [x] #10 Logs correlate persisted event IDs and show one primary entry per recorded failure without discarding independent events or repeated deployments. Exact History-to-Logs navigation, the severity filter, the auto-scroll toggle and downloads work. The timezone toggle and Clear display use unchanged code and are NOT exercised by these browser checks.
- [x] #11 Authenticated API and browser tests execute for Admin, Operator and Viewer, and API tests for unauthenticated and out-of-environment callers. Browser evidence covers reload, desktop and 375 px layouts, Admin detail, suppression, download contents, and duplicate prevention.
- [x] #12 The actual progress query was measured with `EXPLAIN (ANALYZE, BUFFERS)` against 100,000 historical failure events. Pending and succeeded progress ran zero failure-event scan loops. No index was added and no speedup is claimed.
- [x] #13 Provider-body, incomplete-capture and short-secret protections are preserved. Ordinary copies, which inherit ambient credential sources, suppress diagnostic lines and endpoint with `credential_inventory_unavailable`. The authenticated copy masks every value its child inherits (`PATH`, `NIX_REMOTE`, `SSL_CERT_FILE`, `SSL_CERT_DIR`), derived from the same table that sets the child's environment. All roles keep the fixed stage, cause and status. (Agent unit tests.)
- [x] #15 The server normalizes an agent's failure report before persisting it: a size limit, unknown fields and free-form strings dropped, diagnostics sanitized, and an oversized or unreadable report stored as a fixed incomplete marker with only an enum-valid stage and cause. The role projection of the stored form equals that of the reported form, except that an oversized report becomes incomplete. (Server unit tests, including a projection-invariance check, and a `failure-reporting` case that inspects the stored rows.)
- [ ] #14 Diagnose the full Delivery blocker without weakened assertions or deadline increases. An independent reporting scenario is an additional required proof, not a replacement for a passing full lifecycle. NOT complete: the exact-head GitLab CI result is unverified (see the final record).
<!-- AC:END -->

## Non-goals

- Sending unmasked Nix stderr or non-`error:` lines to the server.
- Changing retry, timeout, or cache selection behavior.
- Changing the non-Niks3 copy path (TASK-507).

## Integration test impact (review P2)

- Authority: the MR !344 review requires a safe display boundary for legacy and
  non-Niks3 failures. The implementation adds optional agent `display_error`,
  persisted `metadata.display_failure_message`, and neutral
  `Deployment failed` history/log fallback. Raw `metadata.failure_message` is
  never display-safe. New pending rows contain the safe diagnostic or neutral text.
- Consumers: deployment-failed authentication/persistence, History API,
  agent-events API, and Systems History/Logs. Extend the existing Delivery
  `delivery-lifecycle` with a separate unbooted probe registration. Retain all
  existing activation, membership, rollback and management assertions.
- Disposition: add signed report, unsigned/no-mutation, safe long diagnostic,
  legacy report, blank safe report, and historical missing/blank/raw-only metadata
  cases. Historical rows are compatibility fixtures, not proof of agent reporting.
  Browser assertions protect failed state, error severity, one reconstructed line
  per failure, absence of invented evaluation/activation/rollback, reload and overflow.
- The signed fixture proves reporting and display boundaries only. The separate
  agent failure-wrapper test owns actual Nix diagnostic redaction proof.
- Verification: focused farm gate:
  `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`.
  JavaScript syntax, Python AST parsing, Nix parsing and `git diff --check`
  passed locally using existing Nix-store tools. The new API/browser cases have
  not executed; no browser or screenshot evidence is claimed.

## MR !344 review remediation

- P1 short secrets: mask all nonempty values without trimming or a minimum
  length. Tests cover one-, two-, and three-character credentials, including an
  authenticated-copy child failure and safe report serialization.
- P1 display boundary: only typed authenticated-copy errors supply `display_error`;
  outer contexts and other paths supply neutral text. Ingress does not persist
  legacy raw `error`. History, agent-events, and progress read only explicit safe
  event metadata; History also strips legacy raw metadata from its response.
  TASK-507 still owns non-Niks3 local journal behavior.
- P2 classification: recognize every status from 500 through 599, tested
  exhaustively; reject 499, 600, and 5000 as server-error categories.
- P2 fallback: use `Deployment failed` for History without safe diagnostics.
- P2 regression: extend the existing Delivery scenario; do not claim the authored
  signed report proves agent-side redaction. The agent child-process tests prove
  masking and selection for display separately.
- Farm component verification passed for `default.cf-agent-drv` and
  `default.cf-server-core-drv`. The Delivery gate did not reach runtime: initial
  attempts failed building VM dependencies, then the embedded server test
  `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition`
  failed (`Process` versus `AuthenticationFailed`). That code is unchanged here.
- Preview is blocked: the supported Dioxus launchers compile locally, prohibited
  by the build-farm policy. The embedded server farm build failed as above. No
  browser screenshot, overflow result, or green pipeline is claimed. Keep this
  task In Progress pending successful Delivery/browser verification.
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/344

## Second review: provider-controlled response bodies

- Suppress the entire masked detail when stderr contains `response body:`
  anywhere, case-insensitively. Do not infer that a later `error:` line marks
  the end of a body. The fixed category and exit status remain available.
- The bounded collector now records whether it retained the entire stream.
  Truncation, read failure, drain timeout, or join failure suppress detail;
  a retained tail could have lost the response-body delimiter.
- Tests cover a body beginning with `error: internal-token=secret-value-12345`,
  mixed-case and inline delimiters, and a large body that evicts its delimiter.
  Fake-Nix tests exercise authenticated-copy failures, typed safe-display
  selection, report serialization, and credential cleanup. The older oversized
  signature test now expects a fixed category without any captured detail.
- Verification on the configured farm:
  - `nix build .#default.cf-agent-drv .#okf-knowledge --no-link -L`: passed;
    agent test groups passed 25, 13, and 1 tests.
  - An ephemeral `cf-server-drv.overrideAttrs` narrowed only `checkPhase` to
    `cargo test --offline --locked --release --package cf-server --features
    cf-server/embedded-ui --lib
    handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition
    -- --exact --nocapture`: passed independently (1 test) on the same farm.
  - `nix build .#default.cf-server-drv --no-link -L`: passed normally, including
    the full library group (1575 passed, 853 ignored under existing policy).
    No production package test was skipped or changed to obtain this result.
  - `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle
    --no-link -L`: ran the normal VM scenario, but hit its 1800-second timeout
    in `publication-rejects-untrusted-signature-then-accepts-same-signed-output`.
    The driver was waiting for the existing `services` node's `nix copy --from
    daemon --to ...` command. The final bad-file-descriptor exception followed
    VM shutdown. The new failure-report/browser cases were not reached.
- Keep the MR Draft and task In Progress. Delivery browser behavior, overflow,
  screenshots, and exact-head CI remain unverified. The embedded-server test
  is no longer the current blocker; the full Delivery timeout is.

## Delivery timeout and independent reporting authority

- Owner authorization (2026-11-17): diagnose the full Delivery timeout on the
  Nix farm without skips, weaker assertions, or an unsupported timeout increase.
  The retained run `/tmp/opencode/mr344-body-delivery.log` spent 1504.6 seconds
  in publication receiver dependency copying before the 1800-second deadline.
  `origin/dev` contains the same receiver-copy implementation.
- Add bounded process, daemon, disk, inode, memory and pressure diagnostics
  around the unchanged receiver-copy command. Retain the original signature
  rejection, signed acceptance, exact NAR identity and source nonce assertions.
- The owner permits an independent Delivery reporting scenario because the
  publication infrastructure timeout prevents execution of reporting assertions.
  Register `failure-reporting` as a required sibling of `delivery-lifecycle`.
  The full Delivery check continues to require both gates. This is not a new
  top-level check or a replacement for real build/publication/activation proof.
- Consumers: signed agent reporting, protected persistence, History,
  agent-events, deployment status, and browser reads for Admin/Operator/Viewer.
  Reuse the existing failure-detail fragments unchanged by the infrastructure
  owner. Seed only explicitly preexisting artifact prerequisites in this new
  scenario's disposable VM/database; create deployment requests through the API
  and submit failures through real Ed25519 authentication.
- Retain raw `EXPLAIN (ANALYZE, BUFFERS)` text/JSON for the exact production
  progress query with 20,001 deployments and 100,000 historical failure events
  on a separate probe system. Measure pending, succeeded and failed states.
  No index or improvement claim is authorized without measured evidence.
- Full Delivery attempted: `nix build path:.#checks.x86_64-linux.integration-delivery
  --keep-going --no-link -L`; log `/tmp/opencode/task529-delivery-final.log`.
  The path flake includes concurrently authored untracked application files.
- The final instrumentation uses the existing 1800-second global deadline
  without a new copy deadline. Earlier instrumented snapshots were cancelled
  before runtime after correcting the systemd-run PATH and removing a diagnostic
  deadline that was shorter than known progressing batches.
- A standalone five-path receiver transport probe passed on the farm (76.42 s
  driver, including VM boot): `/tmp/opencode/task529-receiver-probe-v2-retry.log`,
  output `/nix/store/c2pfs6zgh09yarbk939wamv8lr7psx9s-vm-test-run-task529-receiver-transport-probe`.
  The first probe failed because its diagnostic systemd-run had no runuser in
  PATH; a subsequent evaluation hit a transient stale file handle and was retried.
- The original full-run final copy path is the pinned nixpkgs source tree,
  `/nix/store/8gs4wzj7sc6is1sc2qffhzp3mpc5r7br-source`. Its registered NAR size
  is 206,233,976 bytes. The bounded receiver diagnostic below establishes slow
  9p-backed source reads independently of the application. No infrastructure
  fix or completed full Delivery run is claimed.
- Knowledge validation passed on the farm: 58 tests, 216 Mermaid blocks with
  zero syntax errors, 195 concepts. Log `/tmp/opencode/task529-delivery-knowledge.log`;
  output `/nix/store/hx1sccg7z666nb8d8hnmvcwmz9h8gj73-okf-knowledge-validation`.
  Python AST parsing, `git diff --check`, and the exact four top-level integration
  check names also passed.
- The full Delivery attempt failed before either scenario started. Embedded
  server derivation `9ysazyii817l3kybhhifgdf4ciyn106d-cf-server-0.3.0.drv`
  failed `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition`
  at `builders.rs:7283`: actual `Process`, expected `MissingPath`. The library
  group recorded 1578 passed, 1 failed and 854 ignored under its existing policy.
  The Web UI dependency recorded 880 passed and 1 ignored. Neither reporting
  API/browser assertions nor the publication instrumentation ran in this attempt.
  This is a package test failure, not a farm transport error or Delivery pass.
- The standalone source-pressure experiment was cancelled after about 20 minutes
  without reaching its VM; it has no measured source-pressure result. Its log is
  `/tmp/opencode/task529-source-pressure-probe.log`.
- A separate SQL-only farm VM applies the actual repository migrations and
  measures the exact query without an application build. Command:
  `nix build --impure --expr 'import /tmp/opencode/task529-query-plan.nix'
  --no-link -L`; log `/tmp/opencode/task529-query-plan.log`. This is measurement
  evidence only and cannot replace either required Delivery scenario.
- SQL-only farm verification passed after correcting its initial fixture's
  missing required `systems.derivation` column. Actual production migrations
  applied successfully. Log `/tmp/opencode/task529-query-plan-v2.log`; immutable
  output `/nix/store/9qw8vwnamb7hx3f7g1mwfsfx2x58qa84-vm-test-run-task529-progress-query-plans`.
  Its `progress-query-performance/` directory retains unbound/bound SQL,
  text/JSON plans for all three states, measurements, full schema and successful
  migration checksums. Production query SHA-256:
  `e18c9f91f593f890d251f60868498c545c1d8e8c2b531d9aa43018103ca16a33`.
  Each state has 20,001 deployments. Pending/succeeded have 100,000 historical
  failure events; failed adds one current event (100,001 total).
  JSON-plan planning/execution milliseconds: pending 5.704/7.764,
  succeeded 5.117/8.031, failed 5.117/7.982. Failure-event scan loops are 0/0/1.
  The native plans show the failed-status One-Time Filter, with the event scan
  never executed for pending/succeeded. Failed uses the preexisting migration
  0242 index and four event-lookup buffer hits. No index was added and no
  baseline-relative speedup is claimed.
- A source-only receiver diagnostic reproduced slow progressing transfer using
  the same Nix 2.34.8 and unchanged signature receiver, without a CF server or
  agent. The experiment's own 120-second bound returned 124; this is a timed-out
  copy, not a passing copy. Its diagnostic run completed in 158.30 seconds.
  Log `/tmp/opencode/task529-large-receiver-probe.log`; immutable output
  `/nix/store/qa5rq0z5qhdnrnml2lqamm6sikxdqzvk-vm-test-run-task529-large-receiver-diagnostic`.
  `copy-result.json`, `sample-*.log` and `source.strace` retain raw evidence.
  Receiver contents grew from 38 MiB to 137 MiB. Sample 8 (97 seconds) shows
  the copy client's main thread in `D` / `p9_client_rpc` at 8.2% CPU; both daemon
  connections wait on Unix socket reads. The root disk has 7.3 GiB free and
  501,786 free inodes; memory has 664 MiB available, no swap use and zero memory
  pressure. I/O pressure is nonzero. This demonstrates 9p source-read latency,
  not a fully stalled transfer, in the same operation that exhausted the
  original full-run deadline. It does not measure the original run's entire
  interleaving or rule out additional pressure there.
- Final knowledge validation passed again after the coverage-map update:
  `/tmp/opencode/task529-delivery-knowledge-final.log`, output
  `/nix/store/din0zfs8sd7zyfgy22vmy6ydby001b9p-okf-knowledge-validation`.

## Expanded role-based model: implementation and review record

The owner expanded TASK-529 after commit `43fbbfc1`. Remote MR head and `dev`
were independently fetched and inspected before implementation; they were
`43fbbfc1` and `881dbc76`. The existing task branch/worktree was clean. This
section supersedes the earlier text-only display contract, not the security
requirements or historical verification records above.

- Shared protocol enums own the fixed stage/cause vocabulary and remediation.
  Signed reports carry optional structured failure data; compatibility text
  contains fixed summaries only. Server-side projections govern all three
  deployment APIs, including their message/title fields and metadata. Roles are
  existing global assignments with environment membership for non-Admins.
- History retains its existing card, rail, status, generation/commit/actor/time
  metadata and actions. It adds role-projected fields in the message area;
  Admin evidence remains a separate disclosure. Logs correlate real event IDs,
  not timestamps or commits. Downloads use the same projected content.
- Invalid local profile paths no longer become remote-cache diagnoses. Only
  operation-independent disk/permission causes can replace local command-stage
  fallbacks; Nix-daemon causes apply only to generation creation.
- Detached activation retains `--no-block` and `--collect`. Its ExecStopPost
  invokes the originating executable in early report-only mode. Observed
  non-success produces `activation_execution`; success produces no request.
  Reporter failure is ignored by systemd and uses the existing delivery deadline.
  Host loss and unsuccessful callback/report delivery remain best-effort limits.
- Ordinary reads retain ambient authentication semantics, so their credential
  inventory cannot be proven complete. They suppress technical lines/endpoints
  with `credential_inventory_unavailable`, retaining any higher-priority body or
  incomplete-capture suppression. Stage/cause/status/attempts remain useful.
- Agent security tests, server role-projection tests, and the new independent
  `failure-reporting` scenario protect the expanded contract. The full
  `delivery-lifecycle` remains a required sibling gate. Existing assertions and
  its 1800-second limit are unchanged.

### Final verification record

All results below are from the Nix build farm. No local compilation was run.

**Gates on the final snapshot** (`nix build .#checks.x86_64-linux.integration-delivery --keep-going`, exit 0):

- `failure-reporting`: all 10 cases passed. They cover signed reporting and persistence, legacy and historical metadata, direct API reads for the three roles, 403/404 scope denial, the browser matrix (3 roles x desktop/reload/narrow x History/Logs), and the systemd callback cases.
- `delivery-lifecycle`: with its 1800 s deadline and assertions unchanged, it passed in 6 of the 7 executions after the embedded fragment was removed, and failed once on a pre-existing race described below. An eighth invocation exited in under a second because Nix reused the cached failed evidence, so it is not counted as an execution. Its only difference from `origin/dev` is the best-effort diagnostics in `publication.py`.
- Components: `cf-agent` 41 + 14 + 1 tests, `cf-server-core` 1587 library tests, `web-ui` 883 tests, and `okf-knowledge` validation all passed.

**Problems found in my own work during verification, and their fixes**

1. The `publication.py` diagnostics used `succeed`, so a failing sampling command failed the lifecycle at 847 s while the copy was still progressing. Sampling is now best-effort. The copy's recorded status is still the only pass/fail signal.
2. I had embedded the role browser fragment in `delivery-lifecycle`. It needs about 560 s and the lifecycle had about 540 s of its deadline left, with recovery cases still to run. I reverted `delivery-lifecycle.nix` to `origin/dev`. The reporting workflow is owned by `failure-reporting`.
3. The browser spec depended on the 2-minute progress window. The banner now gets strict field checks in an early pre-pass that asserts HTTP 200. Later phases take one atomic DOM snapshot and check role safety.
4. The spec signed in at the previous role's 375 px viewport, where the sidebar is collapsed. Sign-in now sets the desktop viewport first.
5. The shared Admin login helper asserts a link named exactly "Systems". An attention badge makes the name "Systems 1", so it passed only by winning a race. The spec has its own login that verifies identity and roles through `whoami`.
6. `command_failure` reused cache-specific causes for local generation and systemd failures. It now reuses only disk-full and permission-denied (and Nix-daemon for generation creation).
7. A heartbeat can mark a request `succeeded` before the activation unit exits nonzero, which lost the failure. The agent now passes the request ID from `/agent/deployment-started` to the callback, and the server accepts an exact-ID `activation_execution` report for a `succeeded` request. Other report types keep pending-only matching.

**Intermittent lifecycle failure, not caused by TASK-529.** In one execution the last case, `management-pending-deployment-conversion-and-concurrent-replay`, failed. `management-authority.cjs:31` races a management-mode conversion against two deployment requests and asserts the status is one of 202, 400 or 409. One request returned 500. The controller journal shows two lines, `failed to reserve deployment request identity ... error=Deployment requires an active managed system`. When the conversion commits after the deploy handler's early check but before the reservation's recheck, the handler maps that error to `internal_error(...)` instead of a 400. That mapping is identical on `origin/dev`, and the TASK-529 diff touches neither the handler nor `reserve_explicit_deployment_request`. It is a real product defect (the user caused a normal condition and gets a 500), and it should be fixed and filed by the owner. I did not change deploy behavior here. Evidence: `/nix/store/39ba1dd8l36dfspn6l13v45lg9maw91a-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

**Change to a pre-existing test:** `niks3_read_timeout_kills_and_reaps_before_credential_cleanup` used a 100 ms deadline and failed with ENOENT on a loaded builder, because its fake `nix` runs about a dozen subprocesses before it writes the marker the test reads. This matches commit `cade65d2` (a local backup branch that never reached `dev`). The deadline is now 5 s. The script sleeps 30 s, so the kill-and-reap path still runs, and no assertion changed.

**Measurements.** Progress query, 20,001 deployments and 100,000 historical failure events: failure-event scan loops were 0 / 0 / 1 for pending / succeeded / failed in every run. Execution time varied with farm load from 3.0 to 17.1 ms across runs (final run: 15.7 / 16.0 / 16.8 ms). The existing index `system_events_deployment_failure_bootstrap_idx` was used. No index was added and no baseline was measured, so no speedup is claimed. Final-run artifact: `/nix/store/y4lm8g0qxydf4l09psrjjpgch1150axa-vm-test-run-cf-it-delivery-failure-reporting-run/progress-query-performance/`.

**Delivery timeout.** Two earlier full runs hit the 1800 s limit in `publication-rejects-untrusted-signature-then-accepts-same-signed-output`. A standalone reproduction showed the copy client waiting in `p9_client_rpc` while reading a 206 MB source NAR through 9p, with no memory or disk exhaustion. The same case passed in the last four runs (311 to 332 s) and in the run before them (464 s). The latency is variable and I did not fix it, so a future slow run can still time out. This is infrastructure behavior, not evidence about TASK-529.

**Visual review.** Final screenshots were inspected against `docs/design/CrystalForge/components/SystemDetail.jsx` and the Systems design specification. All eight tabs, the card rail, kind, generation text, status chip, commit link, actor, time and action buttons remain in place. One deliberate difference: the single `tl-msg` line is now a stage / summary / status list, with Admin diagnostics in a collapsed disclosure. Long endpoints and error lines wrap inside the card and log row at desktop and 375 px with no page overflow. Observations that are not regressions of this task: the global "configuration issues" banner is sticky and overlaps the top of some framed captures, and the raw status words (`classified`, `legacy_agent`, `incomplete_report`) are shown to every role, which may deserve friendlier wording.

### Third review round

Three findings were raised against `5f8fa3c1`. All three were confirmed against the code before any change.

1. **P1, inherited environment not masked (fixed).** The authenticated copy child keeps `PATH`, `NIX_REMOTE`, `SSL_CERT_FILE` and `SSL_CERT_DIR` after `env_clear`, and the masking inventory held none of them. `NIX_REMOTE` can carry credentials. The two lists could drift, so both now come from one table (`AUTHENTICATED_COPY_INHERITED_ENV`): adding an inherited variable masks it automatically. The masking covers each whole value, each list element, and each certificate path's parent, and never `/`. Tests: a fake `nix` that prints the inherited certificate path, directory and `NIX_REMOTE` value (they must be masked, not merely suppressed), and a test that every variable in the table is masked.
2. **P2, unbounded failure JSON persisted (fixed).** The server stored the agent's `failure` value as sent. It now normalizes first: 8 KiB limit, unknown fields dropped, diagnostics sanitized, and an oversized or unreadable report stored as `{"incomplete": true}` with only an enum-valid stage and cause. The projection honors that marker, and an incomplete report cannot decide a state change. A `failure-reporting` case sends five signed reports (unknown fields, credential-bearing URL, oversized diagnostics, oversized unknown field, malformed) and inspects the stored rows, plus a marker scan of both tables.
3. **P2, invented steps for successful deployments (fixed).** The `Deploy` branch built five invented lines (evaluating, eval success, build completed, activating, generation activated). I chose removal over rescoping AC #4, because the original brief says not to fabricate evaluation, activation or rollback steps. The exact-ID skip rule now covers starts, successes and failures. A legacy row with no real event gets one line that restates only the recorded outcome. Reconstructed lines for restarts, reboots and on-host rebuilds remain; a few restate unrecorded steps (a service start, an out-of-band activation). They are outside this task and should be filed as a follow-up task by the owner. I did not allocate a task ID, because another worktree is using TASK-530.

During the build, `ordinary_copy_suppresses_ambient_credentials_but_keeps_fixed_cause` failed once with `Spawn` where `ServerError` was expected, and passed on rerun. It does not touch any code I changed. The repository's own commit `af39284b` documents that an executable-fixture writer inherited by a concurrent `fork` can make an unrelated test fail with `ETXTBSY`. That matches, but the assertion does not print the errno, so I did not prove it. It passed in every other run.

### Not verified

- **Exact-head GitLab CI.** `glab auth status` returns 401 (no token), so no pipeline was read. No CI result is claimed.
- **Interactive preview.** The supported Dioxus launchers compile locally, which the build-farm rule forbids. Browser evidence comes from task-owned isolated test VMs, never the owner's database.
- **Ignored SQLx tests** (`#[ignore]`, need a CREATEDB database) did not run, including `deployment_failure_api_role_projection_and_scope` and the two callback-correlation tests. The `failure-reporting` scenario exercises the same behavior through the real server and database. It does not cover a callback ID that belongs to a different system.
- **Callback hops.** The late-failure case seeds the `succeeded` row with the production row transition. It does not run the heartbeat path or the started-response-to-argument hop; those have unit tests only. The callback case proves real systemd and the production reporter, not a Nix system switch.
- **Real Nix activation failure,** and the non-callback generation/cache paths, are covered by unit tests, not by a VM test.
- **Delivery of the callback report** is best-effort (five-second deadline, no retry). Host loss or an unreachable server loses it.
- **Timezone toggle and Clear display** are not exercised by the browser checks.
- **MR !344 page.** The description could not be read or updated through the API for the same authentication reason.

Keep the task In Progress and MR !344 Draft until the exact-head CI result is checked.
