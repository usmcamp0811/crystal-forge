---
id: TASK-398
title: Make attention alerts recent occurrence-based and permanently dismissible
status: In Progress
assignee: []
created_date: '2026-07-20 00:00'
updated_date: '2026-07-20 00:00'
labels:
  - backend
  - frontend
  - database
  - alerts
  - navigation
  - builds
  - evaluations
  - flakes
  - environments
  - sprint-ready
dependencies: []
references:
  - packages/web-ui/src/alerts/mod.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/views/systems_list.rs
  - packages/default/crates/cf-server/src/handlers/api/navigation.rs
  - packages/default/crates/cf-server/src/queries/navigation.rs
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/migrations/0159_user_alert_acknowledgments.sql
  - packages/default/crates/cf-server/migrations/0171_alert_ack_fingerprint.sql
  - packages/default/crates/cf-server/migrations/0172_alert_ack_seen_ids.sql
priority: high
ordinal: 398000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge attention badges currently become persistent background noise instead of notifying users about new problems.

Observed production behavior includes:

- numerous sidebar build alerts from historical failed builds;
- dismissed alerts returning after refresh or polling;
- Flakes and Environments each repeatedly showing the same two alerts after dismissal;
- the Builds `Completed` tab remaining red because completed history contains failed rows;
- the Evaluations `History` tab similarly conflating historical failures with new attention; and
- old unresolved conditions continuing to alert indefinitely.

The intended product behavior is different: attention indicators are a short-lived inbox for new problems that may need investigation. They are not permanent health totals and they are not a historical failure counter. A new occurrence may alert for at most 24 hours. It remains visible during that window until the user acknowledges/dismisses it. Once dismissed, that exact occurrence must never return. If the condition resolves and later happens again, the new occurrence may alert independently.

The current implementation cannot reliably provide those semantics:

- `user_alert_acknowledgments` stores one mutable baseline and one last-seen ID array per user/category. Replacing that array is not durable per-occurrence dismissal history.
- Build alerts are selected from failed jobs inside the latest 100 terminal jobs, while Evaluation alerts are selected from failed evaluations inside the latest 10,000 terminal evaluations. These are position-bounded history windows, not a time-bounded alert window.
- Flake occurrence IDs contain `last_sync_at`. A retry of the same unresolved sync problem can change the ID and make a dismissed problem appear new.
- Environment occurrence IDs contain current critical/offline/CVE counts. Normal rollup changes can change the ID without a genuinely new environment incident.
- System occurrence IDs contain derived health, `last_seen`, and CVE counts. Heartbeat/time changes can manufacture new IDs for the same uninterrupted condition.
- Individual row dismissals are stored only in browser LocalStorage. They do not reliably follow the authenticated user across browsers/devices and are separate from the server-owned sidebar acknowledgment state.
- Builds/Evaluations tab styling uses an attention count alongside neutral historical counts, making it too easy for a tab containing old failures to remain or become red even when there is nothing new.
- Some views acknowledge on load, some on tab activation, and some only after loading a large bounded history. This produces inconsistent and sometimes impossible-to-complete acknowledgment behavior.

This task was specified against `dev` commit `7c2b78ba3c6d36c3443b08a17cfe08c8478555d9` (2026-07-20), after the alert work from MR !298 and the paged Builds/Evaluations work from TASK-394. Implementation must revalidate the exact base commit and preserve the later fixes for view-owned acknowledgment cursors, failed-request throttling, successful-refresh behavior, authorization scoping, and evaluation occurrence timestamps.

## Goal

Replace mutable category snapshots and browser-only row dismissals with one server-owned, occurrence-based attention contract shared by sidebar badges and in-view attention styling.

For every supported category:

```text
new underlying incident
    -> stable occurrence is opened
    -> eligible to alert for 24 hours
    -> sidebar and relevant tab/row may show attention

user acknowledges/dismisses occurrence
    -> server records per-user dismissal
    -> exact occurrence disappears immediately
    -> exact occurrence never returns for that user

incident remains unresolved past 24 hours
    -> it may remain visible as ordinary status/history
    -> it no longer contributes red attention or sidebar count

incident resolves, then recurs
    -> a new occurrence ID is opened
    -> it may alert for a fresh 24-hour window
```

Historical failed builds and evaluations remain queryable and visibly failed in their tables. Historical data must not keep a sidebar badge or tab red.

## Product Semantics

### Attention window

- Define one server constant/configuration value for the attention lookback, defaulting to 24 hours.
- Eligibility is based on the immutable occurrence `opened_at`, not the latest poll, retry, heartbeat, update, or `last_observed_at` time.
- An occurrence is attention-eligible only when `opened_at >= observed_at - 24 hours` and it has not been dismissed by the current user.
- At exactly the boundary, use one documented inclusive/exclusive comparison consistently in queries and tests.
- The 24-hour window applies to sidebar badges, red tab attention, row/card attention, and attention flash behavior.
- The ordinary status of a currently failed/offline object may remain red or unhealthy according to existing status design; it must not use the special attention pulse/badge after dismissal or expiry.

### Dismissal and acknowledgment

- Dismissal is per authenticated user and per immutable occurrence ID.
- Dismissal is durable in PostgreSQL and survives page refresh, logout/login, browser changes, and multiple devices.
- Dismissing an occurrence is idempotent.
- Dismissing a category/view records every attention occurrence actually represented by the rendered dataset and observation cursor, not failures that arrived afterward and not unseen pages outside the rendered/eligible set.
- Opening Builds `Completed` or Evaluations `History` may acknowledge the currently rendered eligible occurrences after the relevant data successfully loads. A failed/partial load must not consume alerts it did not show.
- Opening a Flake, Environment, or System attention row/card, using an explicit dismiss control, or completing the established view-level acknowledgment action must persist dismissal for the same canonical occurrence used by the sidebar.
- Optimistic local hiding is allowed for responsiveness, but server state is authoritative. On persistence failure, the alert must become retryable and must not be silently lost.
- Never acknowledge using client `NOW()`. Preserve the server `observed_at` cursor contract so an occurrence created after the observed response remains new.

### Recurrence

- One uninterrupted condition has one stable occurrence ID even if details, counts, timestamps, retry attempts, or messages change.
- Resolution closes the occurrence.
- A later transition from healthy/successful to alerting opens a new occurrence with a new immutable ID.
- Repeated failed sync attempts while a flake remains continuously errored update diagnostic details/`last_observed_at` but do not open new occurrences.
- A system remaining offline while `last_seen` ages does not open new occurrences.
- An environment whose existing incident changes from one critical system to two critical systems does not automatically recreate already-dismissed occurrences for the original system. A genuinely newly alerting member system may create a new environment occurrence tied to that new underlying system occurrence.
- Re-running or retrying the same evaluation/build is a new occurrence only when it creates a new terminal failure event. Preserve evaluation occurrence uniqueness using `evaluation_completed_at` microseconds; preserve the lifecycle fix that clears this timestamp on start/reset and sets a fresh timestamp on terminal failure.

## Required Design

### 1. Add a canonical attention-occurrence model

Add a migration for a server-owned occurrence table and per-user dismissal table. Exact names may follow repository conventions, but the model must support:

```sql
attention_occurrences (
    id uuid primary key,
    category text not null,
    subject_type text not null,
    subject_id text not null,
    source_occurrence_key text not null,
    opened_at timestamptz not null,
    last_observed_at timestamptz not null,
    resolved_at timestamptz null,
    metadata jsonb not null default '{}'::jsonb,
    unique (category, source_occurrence_key)
)

user_attention_dismissals (
    user_id uuid not null references users(id) on delete cascade,
    occurrence_id uuid not null references attention_occurrences(id) on delete cascade,
    dismissed_at timestamptz not null,
    primary key (user_id, occurrence_id)
)
```

The schema may use a generated text ID instead of UUID only if it remains immutable and collision-safe. Do not use mutable counts, `last_seen`, `last_sync_at`, current error text, or polling timestamps as the identity of one uninterrupted incident.

Store only safe routing/presentation metadata. Do not duplicate full logs, credentials, repository secrets, raw authorization failures, or unredacted sync errors in `metadata`.

Add indexes supporting:

- unresolved occurrences by category and `opened_at`;
- eligible recent occurrences by category/time;
- subject resolution/reconciliation;
- per-user dismissal anti-joins; and
- retention cleanup.

Do not add redundant indexes already covered by primary/unique constraints. Use a new migration and do not edit migrations 0159, 0171, or 0172.

### 2. Define canonical source occurrences by category

Implement one server module that owns category constants, source occurrence keys, the 24-hour eligibility predicate, open/observe/resolve transitions, and response mapping. Do not independently recreate occurrence IDs in several UI views.

Required categories:

| Category | Opens when | Stable source occurrence | Resolves when |
| --- | --- | --- | --- |
| `builds` | A build job enters terminal `failed` | Build job UUID/ID plus its terminal failure generation if a row can fail more than once | Terminal event is immutable; occurrence remains historical and expires from attention after 24h |
| `evals` | A commit evaluation enters terminal `failed` | `eval:<commit_id>:<evaluation_completed_at_microseconds>` | That failure event is immutable; retry creates a later event, and the old occurrence expires after 24h |
| `flakes` | Effective sync state transitions from non-error to explicit error, or from syncing to stale | Server-generated incident ID tied to the continuous effective error episode | A successful/non-error sync is recorded |
| `systems` | Active system transitions into an attention health state such as critical/offline | Server-generated incident ID tied to the continuous unhealthy episode and alert reason family | System recovers, becomes inactive, or leaves the relevant reason family |
| `environments` | A newly opened underlying system occurrence causes the environment to need attention | Environment ID plus underlying system occurrence ID | Underlying occurrence resolves or system leaves the environment |
| `cves` | A critical CVE occurrence first becomes fleet-relevant | Existing stable CVE/first-seen occurrence identity, normalized server-side | Existing CVE resolution semantics; otherwise expiry/dismissal removes attention |

If a build job row is immutable after terminal completion, its ID alone is sufficient. If the same row can be reset and fail again, include a persisted terminal occurrence timestamp/generation and test it.

Environments must not manufacture an occurrence from aggregate count tuples such as `environment_id:critical_count:offline_count:cve_count`. Tie environment attention to stable underlying occurrences so count fluctuations do not recreate dismissed alerts.

### 3. Produce occurrences at state transitions

Prefer writing/opening/resolving occurrences in the mutation or background paths that already know a transition happened:

- build completion/failure persistence;
- evaluation terminal-state persistence;
- flake sync status transitions and stale-sync reconciliation;
- agent heartbeat/system health reconciliation;
- environment membership/system occurrence reconciliation; and
- CVE ingestion/visibility transition.

Do not create a new occurrence on every navigation badge GET. Badge GET must be read-only. If time-derived states such as offline or stale syncing require periodic detection, add or extend a bounded background reconciliation job. It must:

- use existing background-job lifecycle patterns;
- be idempotent;
- avoid opening duplicate occurrences under concurrent runs;
- resolve occurrences when the source recovers;
- apply authorization-independent global state only, leaving user scoping to reads;
- run frequently enough that a new offline/stale condition appears within the existing badge polling expectations; and
- avoid scanning unbounded history on each cycle.

Opening/resolving attention occurrences must not be allowed to fail the underlying build, evaluation, heartbeat, or sync transaction after that domain event has already succeeded. Use the same transaction where safe; otherwise log and reconcile idempotently.

### 4. Replace category-baseline badge computation

Rewrite `fetch_navigation_badges` so attention counts come from eligible canonical occurrences anti-joined against `user_attention_dismissals` for the requesting user.

The query must enforce existing authorization scope:

- admins see globally eligible occurrences;
- non-admin users see only systems/environments and derived incidents in environments they can access;
- build/evaluation/flake/CVE visibility must remain consistent with current route authorization and any applicable environment scoping.

Return counts and bounded occurrence descriptors/IDs sufficient for correct acknowledgment. Do not return an unbounded array of every historical failure.

The sidebar count means “undismissed occurrences opened in the last 24 hours,” never “all currently unhealthy objects” and never “all failed rows in retained history.” Update badge tooltips accordingly.

Keep `systems_total`, `flakes_total`, and `environments_total` as neutral inventory totals if used by tooltips. Do not combine these totals with attention counts.

### 5. Add a durable dismissal API

Replace or evolve `POST /api/v1/navigation/acknowledge` to persist occurrence dismissals.

The request must include:

- category;
- server observation cursor;
- exact occurrence IDs represented by the successful rendered dataset/action; and
- optional acknowledgment source for diagnostics (`sidebar`, `view`, `row`) if useful.

The server must:

- authenticate the user;
- validate the category allowlist;
- validate that supplied occurrence IDs belong to the category;
- validate that the user is authorized to see each occurrence;
- ignore duplicates idempotently;
- refuse or ignore occurrences opened after the supplied observation cursor;
- insert dismissals in one bounded transaction;
- return the resulting undismissed count or refreshed badge state; and
- never accept a raw client count as proof of which occurrences were seen.

Do not allow a malicious client to dismiss another user’s alerts or infer inaccessible occurrence IDs.

Retain compatibility with currently deployed web UI code during rollout if server and UI can be deployed separately. This may require temporarily accepting the old payload and translating only safely identifiable IDs. Document the compatibility window and remove dead paths only when repository deployment policy permits it.

### 6. Unify row/card and sidebar dismissal

Remove LocalStorage as the authoritative dismissal store for server-backed attention categories. It may remain only as an optimistic cache keyed by canonical server occurrence ID.

All views must use occurrence IDs supplied by the server. Delete UI-generated keys based on mutable fields, including:

- environment aggregate count tuples;
- system `last_seen`/current count tuples; and
- flake `last_sync_at` retry timestamps.

Opening/dismissing a row/card must call the durable dismissal API and immediately update `NAV_BADGES` from the server response or a safe optimistic subtraction. On failure, restore/reload authoritative state and show a retryable nonblocking error where the view already has an error/notice surface.

Garbage-collect stale LocalStorage dismissal keys or change the namespace/version so old generated keys cannot interfere with canonical IDs. Preserve authentication namespacing during the transition.

### 7. Separate neutral tab counts from attention styling

Builds and Evaluations must display two different concepts:

- neutral tab badge: count of records/history represented by that tab; and
- attention state: whether the user has undismissed eligible failure occurrences from the last 24 hours.

For Builds:

- The `Completed` numeric pill remains a neutral total of completed/terminal history according to existing UI semantics.
- It receives red attention styling/pulse only when `builds_failed_new > 0` under the new occurrence query.
- Historical failed builds remain failed/red at the row status level but do not make the tab or sidebar alert.
- Opening `Completed` acknowledges only eligible occurrences represented by the successfully loaded attention dataset, without loading 10,000 rows or every historical failure.

For Evaluations:

- The `History` numeric pill remains a neutral history count.
- It receives attention styling/pulse only when `evals_failed_new > 0` under the new occurrence query.
- Historical failed evaluations remain visible as failures but do not make the tab or sidebar alert.
- Opening `History` must not require loading all 10,000 reachable evaluations before acknowledgment.

The tab must stop pulsing/red styling immediately after successful dismissal and must remain neutral across polling, refresh, logout/login, and another browser session unless a different eligible occurrence opens.

### 8. Define retention and cleanup

Attention eligibility ends after 24 hours, but occurrence/dismissal rows may be retained longer for deduplication and audit-safe recurrence behavior.

Implement bounded cleanup with conservative defaults, for example:

- retain unresolved occurrence rows long enough to maintain a continuous incident identity even after the attention window expires;
- retain resolved occurrences and associated dismissals for at least 30 days;
- never delete an unresolved occurrence merely because it is older than 24 hours;
- delete in bounded batches using an indexed timestamp; and
- do not make cleanup required for badge correctness.

Document the chosen retention. The product’s 24-hour attention rule must be a query rule, not dependent on cleanup running exactly on time.

## Recommended Implementation Order

1. Reproduce and document the recurring Flakes/Environments IDs and historical Builds/Evaluations tab alerts on the task base commit.
2. Define the canonical occurrence contract and category transition matrix in code/tests.
3. Add occurrence and per-user dismissal migrations/indexes.
4. Add idempotent open/observe/resolve query functions.
5. Wire build and evaluation terminal failure producers.
6. Wire flake sync error/recovery transitions.
7. Add bounded reconciliation for time-derived system, environment, and stale-flake conditions.
8. Wire CVE occurrences or preserve existing stable CVE identity through the canonical model.
9. Rewrite badge queries and dismissal API with authorization checks and the 24-hour rule.
10. Update shared API models and SQLx metadata.
11. Update sidebar, Builds/Evaluations tabs, and Flakes/Systems/Environments row dismissal to use server occurrence IDs.
12. Remove/version old LocalStorage keys and obsolete category snapshot logic after compatibility is proven.
13. Run migration, lifecycle, authorization, UI, polling, multi-session, and time-boundary verification.

## Non-Goals

- Do not hide or delete historical failed builds/evaluations from their history tables.
- Do not change a failed build/evaluation’s ordinary status color merely because its attention occurrence was dismissed.
- Do not turn the sidebar into a full notification center, add email/Slack notifications, or add notification preferences in this task.
- Do not add arbitrary snooze durations; the product rule is dismissal plus a fixed default 24-hour eligibility window.
- Do not repeatedly alert on an unresolved condition after 24 hours.
- Do not treat polling, retrying, changing counts, aging heartbeat timestamps, or changing error messages as recurrence.
- Do not weaken environment membership authorization or viewer/operator/admin boundaries.
- Do not store raw logs, secrets, credentials, or unredacted errors in attention metadata.
- Do not change build/evaluation retention, pagination limits, sync intervals, heartbeat thresholds, health thresholds, or CVE severity policy except where required to detect the existing transitions.
- Do not redesign sidebar/tab visuals beyond making attention and neutral counts semantically correct.
- Do not combine this task with Flakes performance optimization, CVE scan write-path tuning, or unrelated background-job work.

## Architectural and Correctness Constraints

- PostgreSQL is authoritative for occurrence identity and user dismissal.
- Canonical occurrence IDs are generated/validated server-side and supplied to the UI.
- One continuous incident has one ID; recovery followed by recurrence has a different ID.
- `opened_at` is immutable after insertion.
- `last_observed_at` and safe metadata may update without changing identity or extending the 24-hour alert window.
- Resolution must be reversible only by opening a new occurrence, not by clearing `resolved_at` on the old row.
- Producer and reconciler operations must be idempotent under retries and concurrent execution.
- Navigation badge GET is read-only and bounded.
- User dismissal cannot acknowledge occurrences newer than the dataset observation cursor.
- Acknowledgment failure remains retryable; a failed payload must not be permanently throttled.
- Successful dismissal refreshes or reconciles shared sidebar state without creating a reactive request loop.
- Sidebar polling must not overwrite a just-dismissed badge with stale pre-dismissal data.
- Server and client use exactly the same category keys and occurrence IDs.
- Browser time is not used for eligibility or acknowledgment cutoffs.
- All timestamp comparisons use PostgreSQL/server UTC.
- The 24-hour predicate is identical across badge counts, view attention queries, and tests.
- Keep event arrays/results bounded; no endpoint returns 10,000 occurrence IDs merely to dismiss them.
- SQL parameters remain bound and category values are allowlisted.
- Preserve evaluation timestamp lifecycle correctness from TASK-394 follow-up.
- Preserve the authenticated-user LocalStorage namespace behavior until LocalStorage ceases to be authoritative.

## Acceptance Criteria

- [ ] Sidebar badges represent only undismissed authorized occurrences opened within the last 24 hours.
- [ ] Failed builds/evaluations older than 24 hours never contribute sidebar or tab attention, regardless of their position in retained history.
- [ ] Historical failed rows remain available and keep ordinary failed status presentation.
- [ ] A dismissed occurrence stays dismissed after polling, refresh, logout/login, server restart, and use from another browser/device.
- [ ] Dismissing one occurrence does not dismiss a different occurrence that arrived after the rendered observation cursor.
- [ ] A failed dismissal request remains visibly/reliably retryable and is not permanently suppressed by client payload throttling.
- [ ] A continuously failing flake produces one occurrence across repeated sync attempts and changing `last_sync_at`/error details.
- [ ] A flake recovery resolves that occurrence; a later sync failure creates a new occurrence eligible for attention.
- [ ] A continuously offline/critical system does not reopen attention as `last_seen` ages or CVE/health counts fluctuate within the same reason family.
- [ ] System recovery followed by a later critical/offline transition creates a new occurrence.
- [ ] Environment attention uses stable underlying occurrence identities and is not recreated merely because aggregate counts change.
- [ ] A newly alerting system in an already unhealthy environment can create a new environment occurrence without resurrecting previously dismissed underlying occurrences.
- [ ] Build failure identity is stable and retry/refailure semantics are explicitly tested.
- [ ] Evaluation occurrence identity remains `eval:<commit_id>:<evaluation_completed_at_microseconds>` or an equivalent server canonical ID with distinct retry failures.
- [ ] `opened_at` never changes when an occurrence is re-observed, and re-observation never extends its 24-hour window.
- [ ] Navigation badge GET performs no acknowledgment/dismissal writes and does not manufacture occurrences.
- [ ] The dismissal API validates user, category, occurrence ownership/visibility, and observation cursor.
- [ ] Users cannot dismiss or infer occurrences outside environments/resources they are authorized to access.
- [ ] Builds `Completed` and Evaluations `History` numeric pills are neutral historical counts.
- [ ] Builds `Completed` and Evaluations `History` tabs use red/pulse attention only when their category has at least one eligible undismissed occurrence.
- [ ] Successful dismissal removes sidebar and tab attention immediately and it does not return on the next poll.
- [ ] Opening Builds/Evaluations attention views can acknowledge the rendered eligible occurrences without loading all retained history or 10,000 rows.
- [ ] Flakes, Systems, and Environments rows/cards use server canonical occurrence IDs rather than UI-generated mutable tuples.
- [ ] LocalStorage is no longer authoritative for server-backed alert dismissal and obsolete keys are safely versioned/cleaned.
- [ ] Time-boundary tests cover just before, exactly at, and just after 24 hours using a controlled database clock/input rather than sleeping.
- [ ] Concurrent producer/reconciler runs cannot create duplicate open occurrences.
- [ ] Cleanup never deletes unresolved occurrence identity merely because it is older than 24 hours and is not required for correctness.
- [ ] Existing badge totals, inventory totals, sync error text sanitization, health/status logic, infinite scrolling, focus navigation, and authorization remain correct.
- [ ] Migrations apply cleanly to fresh and upgraded isolated databases, and SQLx offline metadata is updated.
- [ ] No unrelated notification channel, visual redesign, retention change, or health-threshold change is included.

## Impact Areas

- Alert occurrence/dismissal database schema and migrations.
- Build/evaluation terminal state persistence.
- Flake synchronization error/recovery lifecycle.
- System health/offline and environment attention reconciliation.
- CVE attention occurrence mapping.
- Navigation badge queries and acknowledgment API.
- Sidebar polling/shared alert state.
- Builds/Evaluations tab attention styling and acknowledgment.
- Flakes/Systems/Environments row/card attention and dismissal.
- SQLx metadata, backend tests, web UI tests, and browser/integration checks.

## Risk Level

High. This changes alert identity and dismissal persistence across multiple domains. Incorrect occurrence lifecycle handling could suppress genuinely new failures, repeatedly resurrect old incidents, leak alerts across authorization boundaries, or create unbounded database growth. Implement category producers incrementally, preserve the server observation-cursor race guarantee, and test recurrence, failure, concurrency, and time boundaries before replacing the old baselines.

## Dependencies

- No known task dependency, but implementation must account for MR !298 alert acknowledgment behavior and TASK-394 Builds/Evaluations pagination/focus behavior already present on `dev`.
- Before entering `To Do`, rebase the specification against current `dev` and identify any in-flight work touching navigation badges, build/evaluation terminal lifecycles, flake sync state, system health, environment rollups, or migration numbering.
- Coordinate the next migration number from the actual base branch.
- If decomposed, keep schema/server occurrence lifecycle first, then dismissal/badge API, then UI consumers. Do not ship a mixed state where UI mutable IDs are persisted as canonical occurrences.

## Verification Plan

### Reproduction and baseline

Before implementation, record the base commit and reproduce:

- historical failed builds generating sidebar attention;
- the Builds `Completed` tab staying red after acknowledgment;
- historical failed evaluations generating/recreating History attention;
- a flake remaining failed across multiple sync retries and receiving changing IDs;
- an environment’s aggregate counts changing and receiving a changing ID; and
- dismissal appearing to succeed, followed by the badge returning after the 30-second sidebar poll.

Capture the badge responses, canonical/current IDs, acknowledgment request, post-ack response, and next poll response. Redact repository URLs, credentials, logs, and error details.

### Migration and query tests

Using only a repository-created isolated PostgreSQL database:

- apply all migrations from empty;
- upgrade a fixture containing existing acknowledgment rows from migrations 0159/0171/0172;
- verify uniqueness/indexes/cascades;
- verify open, observe, resolve, recurrence, dismissal, and cleanup behavior;
- verify exact 24-hour boundary behavior with supplied timestamps;
- verify duplicate/concurrent open attempts converge to one occurrence;
- verify authorization-scoped anti-join counts;
- verify navigation reads do not write; and
- inspect `EXPLAIN (ANALYZE, BUFFERS)` for badge queries at representative occurrence/dismissal cardinalities.

Do not run reset, migration experiments, or benchmarks against development, staging, production, or an unspecified local database.

### Backend tests

Add focused tests for every category transition matrix plus:

- observation cursor race: occurrence B opens after cursor A and is not dismissed by A;
- duplicate/idempotent dismissal;
- wrong category and inaccessible occurrence rejection;
- same flake error across retries;
- flake recovery and recurrence;
- system aging without recurrence;
- environment count changes without resurrection;
- new underlying system incident within an unhealthy environment;
- build retry/refailure;
- evaluation retry/refailure timestamp identity;
- 24-hour expiry while unresolved;
- old unresolved occurrence remains stored but non-alerting;
- cleanup batching; and
- compatibility behavior for old acknowledgment rows/client payloads.

Run through the repository Nix environment, adapting package names to the current workspace:

```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check
SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-server --all-targets
SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server navigation
SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server alerts
```

Run relevant full server query, handler, background-job, flake-sync, build, evaluation, system, environment, and CVE tests. Regenerate SQLx offline metadata using the repository’s documented isolated-database workflow.

### Web UI tests

Add tests proving:

- sidebar uses only server undismissed-recent counts;
- neutral tab counts do not imply attention;
- completed/history tabs become neutral immediately after successful dismissal;
- polling cannot resurrect dismissed occurrences;
- dismissal failure restores/retries correctly;
- late sidebar cursor and same-cursor throttling fixes remain intact;
- a newer cursor permits retry after failure;
- stale responses from pre-dismissal polls cannot overwrite current state;
- row/card dismiss uses canonical IDs from the API;
- LocalStorage migration/versioning is safe across auth changes; and
- another session reflects the durable dismissal after refresh.

Run:

```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build -L .#web-ui
```

### Browser and integration verification

With seeded occurrences inside and outside 24 hours:

1. Verify only recent undismissed counts appear in the sidebar.
2. Open Builds `Completed`; verify the attention clears but the neutral total and failed rows remain.
3. Wait through at least two sidebar poll cycles; verify the badge stays cleared.
4. Repeat for Evaluations, Flakes, Systems, and Environments.
5. Refresh, log out/in, and open another browser session; verify dismissal persists.
6. Create a new failure after dismissal; verify only the new occurrence alerts.
7. Retry the same unresolved flake failure; verify it does not alert again.
8. Recover and fail the flake again; verify a new alert appears.
9. Age a fixture occurrence across the 24-hour boundary; verify attention disappears without deleting ordinary status/history.
10. Test a non-admin user with limited environment membership and verify no cross-scope counts or dismissal access.

Run the authoritative web UI/browser check and capture evidence of sidebar/tab state before dismissal, immediately after, after polling, and after a genuine new occurrence.

### Final regression verification

- Verify sidebar badges for all supported categories.
- Verify navigation focus/deep links and infinite-scroll behavior remain correct.
- Verify alert acknowledgment does not create reactive polling loops.
- Verify sync/error sanitization and credentials remain protected.
- Verify background reconciliation is bounded and does not materially increase database load.
- Build server and web UI flake outputs.
- Run `nix flake check --keep-going` because this task changes migrations, SQLx contracts, background lifecycle, API DTOs, and browser behavior.
- Record exact commands/results, migration behavior, query plans, screenshots, and polling/recurrence evidence in the MR.

## Notes

The 24-hour rule limits attention, not operational visibility. An unresolved flake sync error, offline system, failed build, or failed evaluation remains visible in its normal view after the alert expires or is dismissed. What disappears is the special “new item needs attention” signal.

Do not solve this by merely changing `LIMIT 100` to a different row limit, clearing all acknowledgments on every visit, or adding more LocalStorage keys. The durable unit of dismissal must be a stable server occurrence, and mutable incident details must not create a new identity.

Keep this task in `Backlog` until a human selects it for a sprint. Once selected, use a dedicated task worktree and the repository’s isolated database migration workflow.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Migration 0178 added: `attention_occurrences` + `user_attention_dismissals` + cleanup function + indexes.
- Canonical module created at `packages/default/crates/cf-server/src/queries/attention.rs`.
- Wired terminal-failure producers:
  - `packages/default/crates/cf-server/src/queries/builders.rs` `mark_job_failed_with_retry` (permanent failure branch) opens a `builds`/`build_job` occurrence.
  - `packages/default/crates/cf-server/src/queries/build_jobs.rs` `mark_job_failed` opens a `builds`/`build_job` occurrence (defensive wiring for the helper path).
  - `packages/default/crates/cf-server/src/queries/commits.rs` `mark_commit_evaluation_failed` opens an `evaluations`/`commit_eval` occurrence keyed by `evaluation_completed_at` microseconds.
  - `mark_commit_evaluation_complete` and `reset_commit_evaluation` resolve the corresponding `evaluations`/`commit_eval` occurrence.
  - `complete_job_atomic` resolves the `builds`/`build_job` occurrence.
- Verified: `SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-server --all-targets` passes.
- Verified: `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server attention` passes.
- Next: rewrite `fetch_navigation_badges` and the dismissal endpoint to use the canonical occurrence model.
<!-- SECTION:NOTES:END -->
