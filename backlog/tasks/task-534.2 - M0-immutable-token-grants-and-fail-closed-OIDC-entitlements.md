---
id: TASK-534.2
title: M0 immutable token grants and fail-closed OIDC entitlements
status: In Progress
assignee: []
created_date: '2026-11-16'
labels: [security, authorization, oidc, agent-access]
parent_task_id: TASK-534
dependencies: []
references:
  - TASK-534
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Token authority must remain bounded when POA&M authorization context refreshes. A mutable role refresh must not discard or widen the original token grant. Managed OIDC entitlements must not accumulate removed privileges or authorize requests using stale, missing, or unsuccessfully reconciled authority.

## Goal

Establish M0's server-side authorization groundwork: immutable token-grant context carried through POA&M role refresh, effective-authority intersection with current owner entitlements, and fail-closed managed OIDC reconciliation/freshness with meaningful regression and acceptance tests. This independent slice implements the explicit invariants in this request; exact v1.1 contracts come from the user-provided specification message dated 2026-11-16, available in the main agent's conversation.

## Non-goals

CFAT issuance/storage endpoints, rotation/revocation API, MCP tools/transport, production UI/design changes, service principals, CLI, mutation access, or broad identity refactoring. Do not introduce an Admin gate for Operator, Viewer, or Admin owner self-service.

## Architectural constraints

- The server owns authorization. Separate the original immutable token grant from refreshed owner role/POA&M context; every authorization decision remains bounded by both. Role promotion, context reconstruction, and entitlement refresh cannot widen granted scopes, environment IDs, audience, or expiry.
- Reconcile managed OIDC entitlements from authoritative validated identity data, including removals. Preserve unrelated locally managed grants under existing ownership semantics; do not indiscriminately delete all user roles.
- Commit reconciliation and freshness state atomically. Failed, missing, invalid, or stale managed identity evidence must deny authority that depends on that evidence, without retaining old privileges as a success fallback. A freshness timestamp advances only after successful authoritative reconciliation.
- Reuse existing role mapping, identity-source ownership, session, and clock policy mechanisms. Do not invent a freshness duration or expiry limit absent from v1.1. Implement/test fresh, stale, missing, and failed states using existing policy inputs; if no policy input exists, record the missing design decision before adding a numerical public contract.
- Use forward migrations only if necessary, isolated task-owned database tests, and matching SQLx metadata. Keep public Rust contracts and affected OKF security concepts accurate. The authoritative spec destination remains the parent reference; reconcile implementation with the full source before claiming full M0 completion.

## Verification plan

Use `nix develop --command` for focused existing server authorization unit tests and isolated PostgreSQL reconciliation tests. During preflight, identify the actual test filters and commands from the affected modules and record them in the task. Extend the smallest applicable existing `integration-access` authorization scenario to prove the request boundary; no new top-level integration owner/check. Run its focused gate through `nix build .#checks.x86_64-linux.integration-access.scenarios.<owning-scenario> -L` after resolving the actual registered name. Run `nix build .#okf-knowledge -L` for affected knowledge docs and SQLx metadata checks if query shapes change. Preserve actual output/status; unrun or failed acceptance tests block completion.

Required cases: token grant survives POA&M refresh; owner promotion cannot widen grant; demotion/removal denies removed authority; cross-environment access remains denied; managed OIDC role removal/replacement does not accumulate grants; locally owned grants retain their source semantics; stale/missing/invalid claims or reconciliation failure deny affected access; failed reconciliation does not advance freshness or partially persist privileges; successful fresh reconciliation restores only current permitted authority. Exercise concurrent reconciliation/authorization where persistence permits races.

## Impact, risk, and dependencies

- Impact: server request authorization context, POA&M role refresh, OIDC identity/role mapping and persistence, session-derived authority, tests, and security knowledge contracts.
- Risk: high; privilege escalation, stale access, accidental removal of unrelated grants, or partial reconciliation.
- Dependencies: no implementation prerequisite on TASK-271, token issuance or MCP. TASK-534.1 preserves the complete CF-AGENT-ACCESS-001 v1.1 specification at the authoritative path. This task is now active in its dedicated worktree; legacy OIDC grant ownership/cutover remains a rollout gate, not a missing specification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tests prove that POA&M role/context refresh preserves the immutable original token grant and that all post-refresh authorization decisions enforce original audience, explicit environment IDs/scopes, expiry, and current owner authority without widening.
- [ ] #2 Tests prove that owner role promotion cannot expand a token grant and that owner demotion or removal immediately denies affected authority at the request boundary.
- [ ] #3 Authoritative managed OIDC reconciliation removes obsolete managed privileges, applies current permitted mappings, and preserves unrelated locally owned grant semantics; positive and removal/replacement tests pass.
- [ ] #4 Freshness is recorded only on successful atomic reconciliation. Stale, missing, invalid, or failed reconciliation denies access dependent on managed OIDC entitlements; rollback and concurrency tests prove no partial grant update or false freshness.
- [ ] #5 Focused component/database tests and the affected existing integration-access authorization scenario pass, including cross-environment denial and all specified failure cases; exact commands and results are recorded.
- [ ] #6 Public authorization contracts and affected OKF docs describe grant immutability, effective-authority intersection, managed/local ownership, freshness, and fail-closed behavior; required documentation/SQLx validation passes. Full v1.1 alignment and acceptance evidence are required before marking M0 complete.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-526.2-agent-authorization

Allocation provenance: old TASK-526.2 was a session allocation collision resolved as TASK-534.2 after the origin/dev `881dbc76` rebase; parent is TASK-534. This task remains In Progress with the same lock path. The existing branch is still named `TASK-529.2-agent-authorization` and the worktree path still uses 526.2; neither was renamed. Historical branch names, commit hashes, tmux identifiers and temporary derivation paths below retain their original values. The owner will handle Git/worktree and external references separately.

User selected implementation on 2026-11-16. Branch `TASK-526.2-agent-authorization` starts at the pushed specification/design checkpoint `f0416e36`, which contains `dev` baseline `bd8d068d` plus the two handoff commits. New worktree verified clean. Integration `dev` has pre-existing untracked locks, task copies and a session file; none will be modified. No `main` worktree exists. Scope is backend authorization groundwork; no browser-visible changes or live database access.

OIDC owner decision: administrator-managed grants remain independent of IdP-managed grants. User has two OIDC accounts, Admin and Operator. Existing grant ownership is ambiguous and MUST NOT be inferred from the Users screenshot, null actor attribution or the displayed `all` environment summary. Preserve existing grants until an explicit classification/cutover decision; unresolved legacy ownership blocks OIDC PAT rollout. Do not claim M0 reconciliation complete while this gate remains. Initial implementation can independently establish immutable authorization-grant semantics and test service refresh.

Verification: focused authorization/POA&M tests using `nix develop`, isolated PostgreSQL only for database tests, existing Access owner scenario where applicable, and `nix build .#okf-knowledge -L`. Record actual commands/results. No milestone is complete yet.

Implementation checkpoint: introduced nonsecret private `RestrictedGrant` fields and explicit `CredentialIdentity` on every PoamActor construction. Four transaction-refreshed readers require mcp audience, both CVE/POA&M read scopes, future expiry and current-owner/original-grant intersection. Restricted actors never inherit Admin wildcard or mutation flags. Unsafe shared-plan reads fail closed; other readers, exports and every restricted mutation are denied until explicit capability/admission policy exists. Browser authentication behavior is unchanged. No token secret, API endpoint, schema migration or OIDC assignment change is included.

Before the user's build-farm-only instruction, focused authorization unit tests passed 2/2 and `cargo check --tests` passed. Subsequent SQLx test/public-contract refinements have not been locally compiled. From that instruction onward, use `nix build --max-jobs 0` only, not local Cargo. Read-only review identified missing Admin-owner mutation/hydration tests and public error contracts; those have been added. `git diff --check` passes. Farm execution and final docs validation pending; no acceptance criterion is marked complete.

Farm checkpoint: `nix build .#okf-knowledge --max-jobs 0 --no-link -L` passed (58 validator tests, 198 concepts, 219 Mermaid blocks). Focused PostgreSQL proof uses a temporary override of the existing core derivation, on the farm only, with a fresh sandbox-owned Unix-socket-only cluster and verified data_directory; no host database is contacted. The first attempt timed out; a transient Nix stale-file-handle evaluation was retried. The next attempt failed because the temporary harness inherited core test selection and exposed an empty database to online SQLx macros. PostgreSQL stopped cleanly. The harness is corrected to use SQLX_OFFLINE and select the exact integration test; the regression itself is not yet claimed passed.

SQLx metadata disposition: no schema or compile-time query-macro changes; the added projection guard uses runtime-bound SQL. No offline metadata regeneration is required for this checkpoint. Token-row validation, fresh OIDC persistence, request-boundary and concurrent admission evidence remain outstanding.

Pre-rebase farm proof at `3105dabc`: the corrected focused SQLx derivation passed the exact `restricted_credential_refresh_preserves_grant_and_current_owner_intersection` test, 1 passed / 0 failed / 0 ignored, against real migrations in a fresh farm sandbox cluster owned by nixbld, Unix socket only; cleanup stopped PostgreSQL. Derivation `qp0akvh5839ykk728qv5wpc0q9alg4hh-cf-restricted-authorization-regression-0.3.0.drv`, output `ch7fnz4kkl9j0h626sh3bzm7irkyac8g-cf-restricted-authorization-regression-0.3.0`. Another attempt had compiled successfully but stopped in Crane's install-from-build-log hook before execution; that harness issue was corrected. Normal core package build exceeded the foreground 900-second budget; no core package pass is claimed.

User requested latest-base rebase. Fetched origin/dev to `881dbc76` and rebased cleanly. `git range-diff bd8d068d..3105dabc origin/dev..HEAD` showed all three task patches unchanged; backend checkpoint is now `ccabd80d`. Read the newly merged AGENTS build-farm section. Confirmed Nix configuration `builders = @/etc/nix/machines` and `max-jobs = 0`; subsequent commands use those settings without overrides. User explicitly approved force-with-lease; pushed the rebased branch using an exact expected remote tip `3105dabc829c4b41625386542720ad99d891d1b2`.

Rebased-source verification is running separately in persistent tmux server `cf-task5262`, session `farm`: window 0 owns `nix build .#default.cf-server-core-drv --no-link -L`, window `regression` owns the focused farm-only SQLx derivation from `/tmp/opencode/task-526-poam-regression.nix`. A transient stale-file-handle core evaluation was retried. Inspect panes before starting another copy. Pre-rebase passes do not prove the new base. No local compiler or host PostgreSQL was used after the user's farm-only instruction. Task remains In Progress with every full-M0 acceptance item unchecked.
Integration HTTP impact (TASK-534.2, owner-selected bounded slice): authority is approved CF-AGENT-ACCESS-001 v1.1 sections 4.4 (unchanged human-session boundary) and 5.3 (separate managed/local ownership), plus the owner's independent administrator/IdP grants decision above. Consumers are Admin user PATCH/list and existing session-derived role guards. Disposition: retain every existing lifecycle-local-bootstrap assertion; extend its existing authorization.py with `authorization-admin-replacement-preserves-idp-and-classifies-explicit-category` and `authorization-source-union-does-not-admit-operator-admin-mutations`. Explicit prerequisite source rows in the fresh scenario VM database distinguish local, IdP and unresolved legacy contributions. Assert role-only classification leaves legacy membership unresolved; explicit environment replacement resolves that category; IdP role/environment survive local removal; requested Viewer returns effective Operator in PATCH and list; the existing Operator session cannot PATCH itself or another user (403 forbidden, no source/business/audit change). Restore the Operator fixture for later fragments. This proves the admin session+CSRF HTTP boundary only, not a signed real-OIDC callback, freshness/reconciliation, PAT admission, immutable token intersection or cross-environment resource denial. No new check, scenario derivation or VM. Focused gate `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` remains unrun by user instruction; no local builds, databases, services or tests were started.
<!-- SECTION:NOTES:END -->

## OIDC implementation checkpoint (verification pending)

Migration 0335 preserves effective legacy grants and adds independent local/IdP
source tracking, unresolved ownership, durable sync state and a flow generation.
New authoritative reconciliation retains independent local sources and replaces
only the bound identity's source. Valid empty evidence yields an IdP-owned
Viewer/no managed environments, not retention of removed managed privileges.
Unknown legacy ownership preserves current browser login/grants without marking
the identity fresh, including when group evidence is absent.

An ordered database generation is reserved before redirect and stored in the
server-side OIDC flow. Older callbacks cannot overwrite newer successful grants
or invalidate a newer sync. User/identity locks, the shared administrator advisory
lock, pending state and attempt fencing cover cancellation and replacement.
Admin category replacement resolves legacy ownership as explicit local grants;
it does not silently classify those grants as IdP-owned.

Callback, strict claims, positive configurable freshness policy (86400-second
default), source-aware admin writers, fixture role writer, and guarded deletion
are wired. New PostgreSQL tests cover ownership union, migration, freshness,
rollback, old evidence, concurrent admin operations and authoritative revocation.
Two existing Access-owner cases exercise real session/CSRF admin HTTP boundaries.
These changes are not a PAT issuance API or MCP transport.

Security review found and corrected stale-success ordering, mapped-environment
normalization and shared administrator lock-order gaps. Formatting/whitespace
checks pass. Current-source farm compilation and test execution are pending.
No migration has been applied to the user's database or a persistent preview.

Prior rebased grant-isolation regression passed, output
`gy7lli6dcvn40jx5ly6qqkzr5zmd5w1d-cf-restricted-authorization-regression-0.3.0`.
The normal core build at that checkpoint failed 18 library tests: 16 parser
deadline/validation failures tracked by the upstream TASK-526, plus the two Niks3
failures captured in TASK-532. It is not a passing package result. Current
OIDC-specific tests and the extended Access scenario still require farm results.

Task IDs were renumbered because origin/dev independently allocated TASK-526.
Active branch is `TASK-529.2-agent-authorization` (named before the second collision below); the worktree path and
retained tmux socket still use the original 526.2 allocation. The previous pushed
526.2 branch remains historical. User requests an MR when the scoped work and
required checks are ready; do not move to Review or claim M0 completion yet.

Farm verification update: at checkpoint `c2c22b02`, all three integration targets
and the server library test target compiled on the farm. The real PostgreSQL
run passed the restricted-grant regression and all 10 `admin_source_grants`
tests. `oidc_entitlements` passed 16/17; its normalization fixture attempted to
insert `prod`, already seeded by migrations. Fixed only that fixture to reuse
the seeded environment and re-running the database tests. This is not a passing
OIDC-suite result yet. The standalone database derivation now runs database
targets only; ordinary component builds retain ownership of unit tests rather
than duplicating their expensive compilation in the focused database proof.

NixOS evaluation passed: default age `86400`, configured age `3600`, and rejection
of zero, negative and u32-overflow policy values, with correct systemd environment
wiring. OKF validation passed after correcting source-provenance descriptors:
58 validator tests, 199 concepts, 219 Mermaid blocks. The first Access-owner
attempt failed before VM startup because farm `etc-lsb-release` derivation
`r5fsmdq1f9mh9w9mbki32sa2vz7ik1xp` crashed with signal 11. It was not an
application/test failure and does not establish any HTTP-case result. The gate
is being retried on the farm; no local compilation fallback.

Static re-review found no remaining blocker in the corrected generation,
normalization, binding or shared-lock code. This is source review, not runtime
acceptance. Current requests continue in retained tmux windows `regression`
and `access`; inspect their exit markers before launching more builds.

Corrected database proof at `89e7689b` passed on the farm: restricted grant 1/1,
admin sources 10/10, OIDC entitlements 17/17, no ignored tests. Derivation
`wr5b72gvhllqrrrx2vp543f3zp3x800g-cf-oidc-authorization-regressions-0.3.0.drv`,
output `ad6wmqavp3y5iqpjaaca6vn8c4chlgyp-cf-oidc-authorization-regressions-0.3.0`.
The sandbox-owned PostgreSQL instance was stopped after execution. Static SQLx
metadata audit confirmed no checked query/result shape or cached entry references
the altered identity/assignment tables; no offline cache update is required.

Final identity-binding review found that the existing first-sign-in creation
helper could upsert an identity onto a different new user during a conflict.
Changed it to reject the conflicting insert and roll back the new user and
preferences. Added an isolated regression asserting the original owner and
claims remain unchanged and no orphan user persists. This final source change
requires a fresh farm run; the preceding 28-pass result is not its verification.

Access prerequisite retry additionally encountered signal 6 in farm
`p5lfir5qd2c1q9hinxn59ga2cp0wm3g5-unit-systemd-oomd.service.drv`; its log had no
application output. A further farm-only retry is running. No VM case has a
passing result yet. Core component verification is also still pending after
the earlier documented failures. The user requested MR creation when ready;
GitLab CLI API authentication is absent, but authenticated Git push options may
be used to request an MR after readiness instead of requesting secret tokens.

Final source database proof at `24b3bad6`: 29/29 passed, none ignored (18 OIDC,
10 admin-source, one restricted-grant case). Derivation
`h0xpgx1fjhchb5sp5z74d5qdsf5s7y3m-cf-oidc-authorization-regressions-0.3.0.drv`,
output `dpnm1gk4q057w539fynkd3nqkgq20q4x-cf-oidc-authorization-regressions-0.3.0`.
PostgreSQL stopped cleanly. Source-aware binding conflict rejection is covered.

The full Access gate passed on the farm at source `89e7689b`, output
`f29h090fgamgm4xsxbajngrrwg4bjzi3-cf-it-access-lifecycle-local-bootstrap`.
Both new source-union/Admin HTTP cases passed, alongside retained session,
CSRF, registration, administration, environment and preference cases. Its
production server package passed 1570 library tests and Web UI passed 878;
ignored tests are not counted as passes. The final source differs in the
explicitly tested identity-creation conflict fix and the corrected task comment.
Exact review-revision CI must still run; these are not exact-head CI claims.

The separate core run at the earlier checkpoint passed 1568 library tests,
103 cf-config tests and 43 protocol tests, including new strict claims and age
policy assertions. Its additional native-Nix post-checks are still running;
no complete core-package pass is claimed. Source-specific required database,
module-policy, documentation and Access evidence is recorded above.

Implementation checkpoint is being submitted as a DRAFT M0 MR to collect
exact-head CI. Keep this task In Progress, not Review or Done, until the remaining
verification is resolved. No milestone or full PAT/MCP feature is marked complete.

Draft MR opened and verified: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/345
(`TASK-529.2-agent-authorization` into `dev`). GitLab push options used the
existing authorized SSH identity; no API token was requested or exposed.
The MR page confirms draft status and the scope/evidence caveats. No pipeline
result was available at creation. The MR is not merge-ready and was not merged.

## Maintainer review changes requested

The user reviewed MR !345 at `1e41fc32` and confirmed two P1 findings (effective
grants round-tripped into local overrides; non-atomic PATCH/audit) and two P2
findings (remaining mutable identity helper; concurrent first-sign-in recovery).
Keep the MR Draft, this task In Progress, and all six acceptance items unchecked.
PAT issuance, transport, and future token UI remain excluded. The review expands
this task to the existing production Users editor because its integration is a
security boundary, not a cosmetic parity exercise.

Review-fix implementation (execution still pending unless explicitly stated):

- User summaries now expose separate local, IdP and unresolved legacy vectors.
  Effective role and assigned memberships are read-only summaries, never local
  editor defaults. Source metadata is batched, and absent metadata disables grant
  edits instead of inferring ownership. The editor omits unchanged categories,
  uses explicit local-override controls/legacy confirmation, and corrects the
  old next-login-overwrite warning. Admin browser access remains distinct from
  assigned memberships and restricted credential grants.
- Grant-bearing PATCH requires `edit_local_grants=true`; old cached clients
  fail closed with 400 rather than create local authority. `clear_local_role`
  explicitly removes local roles only, preserves IdP/legacy contributions, and
  cannot accompany a replacement role. Account/password-only requests need no
  grant marker. These fields are validated before mutation.
- All requested account, role, environment, password, notification eligibility
  and audit changes commit in one transaction, with final-state last-Admin
  guards and environment validation before writes. Fault-injection regressions
  verify failure points and full rollback, including late audit failures.
- Public identity upsert inserts or returns the same-owner binding without
  changing claims, timestamps or freshness; different-owner reuse is rejected.
  First creation serializes on the stable provider/subject/issuer key before
  inserting a user. Typed/key-specific conflict recovery reloads the winning
  binding, validates it and retains the original ordered flow generation.
  Unrelated username/email uniqueness failures are not account-linking signals.
  Signed RS256/code-exchange/JWKS/PKCE HTTP regressions cover identical/different
  profile races, the real named-index conflict fallback, and malformed claims.
- Existing Access scenarios gain real browser PATCH-body and SQL source/audit
  checks for unrelated edits on a pure-IdP account, plus no local fallback after
  explicit fixture withdrawal. Existing positive grant mutations now carry the
  explicit intent marker; unrelated account edits do not. No production test
  endpoint or new top-level integration check was added.

The production application preview was started before UI edits at
`http://127.0.0.1:8529/admin` on vm-agent, with a fresh task-owned database on
35529. It initially serves the older `89e7689b` farm binary; keep that freshness
limitation visible until the new farm-built embedded binary replaces it. Runtime
ownership, sandbox constraints and refresh procedure are retained in
`/tmp/opencode/task529-preview/HANDOFF.md`. No user database or real credentials
were used. This is not the earlier static-design preview exception.

The frontend farm build passed 887 tests (nine new grant-state regressions), one
ignored; a subsequent text-only membership/Admin-bypass clarification still
requires the final build. Static security review found no remaining concrete
P1/P2 blocker by inspection, not by runtime execution. Server/database/callback
and new browser regressions are being built on the farm. A stale-file-handle
evaluation failure is not a test result and must be retried. No local Cargo or
Dioxus compilation is permitted. The public pipeline API returned 403 for the
review-provided pipeline, so its status is unverified here, not assumed pending
or passing. Migration 0335 is unchanged; no schema migration is required by these
fixes. Do not mark review findings resolved until the corresponding current
source regressions and required gates actually pass.

Review-fix evidence at `386b0052`: the pinned farm database derivation
`2xlnaj0w9fjzinzmb0xqpcy74hb8h0q5-cf-oidc-authorization-regressions-0.3.0.drv`
passed 46 nonignored tests: one restricted-grant case, 21 admin-source/HTTP/
atomicity cases and 24 OIDC cases. This includes real signed callback races for
identical/different profiles and the named-index fallback, as well as rollback
after each mutation/audit failure. Output:
`g9n649wycnli2lp94fp69cyydql5rqqm-cf-oidc-authorization-regressions-0.3.0`.
The sandbox PostgreSQL instance stopped cleanly.

The final embedded application farm build passed, output
`7d1pzdndbwixlv9ybdx0ablc7qqm3x73-cf-server-0.3.0`, deriver
`cj8hj4k05si56rq77kwwimmwwxl2wpjr`. The earlier WIP application build failed in
the existing `niks3_probe_failure_never_polls_success_transition` test; its
failure is not counted as a pass. OKF validation passed 200 concepts and 219
Mermaid blocks. No migration/checksum or checked SQLx metadata changed.

The preview is now refreshed to the review-fix application at 8529; application
PID 115127, task PostgreSQL PID 102192/data directory unchanged. Manual browser
smoke observed only a password field in a separate local-Admin fixture's PATCH,
HTTP 200 and unchanged grant metadata, with no page/request errors. It is preview
evidence, not the pure-IdP integration gate. The Users modal screenshot and exact
ownership proof are in the retained preview handoff/runtime directory.

The first expanded Access run failed before its new write because the test
expected the obsolete “Effective environments” wording. The reviewed contract
distinguishes assigned memberships from the Admin browser-role bypass, so the
assertion now expects “Assigned environment memberships”; all source, payload,
HTTP, persistence and audit assertions remain intact. Re-run the full gate before
claiming either new browser case passed. The old native core post-check run is
also confirmed complete with exit 0 (1h51m54s checkPhase), but it is an older
derivation, not current review-head CI. All six acceptance boxes stay unchecked.

The corrected full Access gate passed on the farm at `2c298432`, including all
three new source-preservation/withdrawal cases and the Operator UI denial case,
plus retained admin/environment/profile/session cases. Gate derivation
`ffr7xr856ak3gb7xf2k4v9z9n5xgri81-cf-it-access-lifecycle-local-bootstrap.drv`,
output `a30jja298m7c49h70cjy094763bq2zzx-cf-it-access-lifecycle-local-bootstrap`;
its evidence/meta.json confirms the revision. Screenshot
`browser/admin-source-unrelated-save/admin-source-unrelated-save-reloaded.png`
was inspected: effective IdP role/memberships are separate, local drafts remain
empty and opt-ins unchecked, account disabled as requested and password field
blank after reload. Original section/action order is retained. The run before
this pass encountered a farm polkit-unit SIGSEGV before VM startup; that is not
an application pass and was retried without local compilation or assertion
reductions. The earlier obsolete-copy test failure remains recorded above.

These farm results address the four concrete review findings at the recorded
code checkpoints. They are not an exact-head GitLab CI claim, maintainer approval
or permission to merge. All six acceptance items remain open, MR !345 stays
Draft, and this task stays In Progress. A fresh normal core/native check for
`2c298432` is running in tmux window `review-native-core`, log
`/tmp/opencode/task529-review-native-core.log`; the older completed native run
is not substituted for it. GitLab's public pipeline endpoint returned 403, so
exact-head CI remains externally unverified here. The production cutover/source
classification/recovery rehearsal remains required before delegated access.

## Second maintainer review

Reviewed head: `063ab5e1`. The reviewer confirmed the four prior findings
(effective-grant round trip, non-atomic PATCH/audit, mutable identity helper,
concurrent first-sign-in recovery) as fixed. The review raised two new findings.
The fixes are in commit `9f9d907e`.

- **P1: the acting Admin was not rechecked inside the PATCH transaction.**
  `require_admin_session` checked the actor before the transaction and took no
  lock, so a concurrent demotion, disablement or logout could commit after the
  check passed. `patch_admin_user` now takes the grant advisory lock, locks the
  actor and target user rows by ascending id, then rechecks that the actor is
  active, holds an Admin role assignment and owns a live session. A failed
  recheck returns 403 with no write and no audit event, before target lookup.
  The session row is share-locked because logout does not take the advisory lock.
- **P2: multi-local-role normalization.** The editor compared the selected role
  with only the highest local role, so an account with several local roles
  (for example Admin and Viewer) could not be normalized to one role. The editor
  now compares with the full deduplicated local role set. With the override
  opted in, keeping the pre-selected highest role sends `role` and
  `edit_local_grants`, and the server replaces all local role sources with that
  role. Zero or one local role stays a no-op. The Role section shows
  `multiple-local-roles-notice` when more than one local role exists.

Deviation recorded: the recheck tests session expiry with `clock_timestamp()`,
not `NOW()` as the existing session lookup does. `NOW()` is the transaction start
time, which precedes any lock wait, so it can accept a session that expired while
the request waited for the locks.

Follow-up: other admin mutations (create user, delete user, OIDC mapping
upsert/delete, classification and retry-policy writes) still authorize before
their transactions and are not rechecked in a transaction. TASK-535 tracks that
work. It is a Backlog capture, not part of this task.

Verification state, with no new farm pass claimed:

- The actor-race and multi-role normalization regressions are static-reviewed and
  queued. Farm results are **pending** unless the parent adds them below.
- A fresh core/native farm build at `2c298432` completed with exit 0, output
  `nj80dnqfmavslb0fvb5zah5vhrckd9di-cf-server-core-0.3.0`. That revision is not
  the current head, so this build is not current-head evidence.
- The reviewer reported the exact-head GitLab pipeline for `063ab5e1` as pending.
  It is unverified here. No pipeline result exists for `9f9d907e` in this record.

MR !345 stays Draft. This task stays In Progress. All six acceptance boxes stay
unchecked.

### Second-review fixes: farm evidence at `9f9d907e`

Both second-review findings are fixed at commit `9f9d907e` (docs follow in `c431dc0b`, which changes no code).

- P1, acting Admin recheck: PASSED on the farm. Pinned database derivation `qwnl38vq5na567lm5kx9da5sdxlq87p8-cf-oidc-authorization-regressions-0.3.0` ran 53 nonignored tests with 0 failures and 0 ignored: 1 restricted-grant, 28 admin-source (21 existing plus 7 new) and 24 OIDC. The new tests are `queued_patch_rejects_actor_demoted_while_waiting`, `..._actor_session_invalidated_while_waiting`, `..._actor_disabled_while_waiting`, `queued_patch_rejects_session_logged_out_without_grant_lock`, `queued_patch_applies_when_actor_remains_valid_admin`, `queued_self_grant_is_rejected_after_actor_demotion` and `patch_admin_user_requires_a_valid_session_of_a_current_admin`. Each queued test proves the request is waiting on the lock before the revocation commits, then asserts HTTP 403 with state, notification queue and audit rows unchanged. The positive control proves the interleaving itself does not reject a valid Admin.
- P2, multiple local roles: PASSED in the web-ui package unit tests, 893 passed, 0 failed, 1 ignored (887 plus 6 new). Preview smoke on a SQL-created two-local-role fixture sent exactly `{"role":"Viewer","edit_local_grants":true}` and the account ended with only that role.
- Full Access gate PASSED at source revision `9f9d907e`: 47 of 47 cases, including the three `admin-source-*` cases and `authorization-operator-has-no-usable-admin-mutation-in-ui`. Output `jmny3mnvp3v6q2s9s4zj3cw3i9v9b9dz-cf-it-access-lifecycle-local-bootstrap`.
- Embedded application build PASSED on retry, output `6c6ih031rl0dzh9cg0fq2ygi6miscqhf-cf-server-0.3.0` (server library 1571 passed, 0 failed, 853 ignored).
- OKF validation PASSED at the documentation commit (200 concepts).

Not a pass, recorded honestly: the same derivation `i125vc8vzll56iii1wgqvx1y6rhq89yx-cf-server-0.3.0.drv` first FAILED twice (application build and Access dependency) in `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition`, with a different expected value each run (`MissingPath`, `Network`, `AuthenticationFailed`) and always `Process` observed, then PASSED on retry with no source change. `builders.rs` is unchanged by this MR. The observed value points at the test's fake `nix` script failing to execute, but the root cause is not established; the test is added to TASK-532. It also failed once on an earlier work-in-progress build and passed on others, so treat it as an existing intermittent failure, not a result for this MR.

Still unverified: exact-head GitLab CI for the new head; a fresh normal core/native check at `9f9d907e` (the completed run was at `2c298432`); narrow-screen, keyboard and error-state UI coverage; an unmocked OIDC provider callback. TASK-535 tracks the other admin mutations that still authorize before their transactions. MR !345 stays Draft, this task stays In Progress, and all six acceptance items stay unchecked.

Preview note: the task-owned preview PostgreSQL lost its shared-memory segment around 21:54 local time on 2026-11-17 (cause not investigated) and was restarted on the same data directory after a backup copy; fixtures and all 317 migrations intact.

## Second allocation collision and rebase onto origin/dev `70001f2b`

`origin/dev` merged an unrelated TASK-529 ("Show the real deployment failure reason in history and logs", merge `70001f2b`) after this family had been renumbered to 529. A separate `scan-cache-recovery` worktree also holds task-530, task-532 and task-533. This family is therefore renumbered again: parent **TASK-534**, children **TASK-534.1** and **TASK-534.2**, and the admin-mutation recheck follow-up **TASK-535** (previously TASK-531). Earlier text in this file that says "TASK-529" for this family now reads TASK-534; commit hashes, farm derivation names and log file names recorded earlier are unchanged because they are historical identifiers.

Our former Backlog capture TASK-530 (Niks3 child startup failures) was removed because that ID belongs to another worktree. Its content is not lost: the flaky `handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition` result is recorded here and relates to the existing **TASK-532** ("Fix intermittent ETXTBSY/probe failures in cf-server core package tests"), which names the same failure family (a fake script executed right after being written, "Text file busy"). TASK-532 lists neighbouring tests, not this exact one, so the link is probable, not confirmed. Observed: that test failed twice on one derivation with the observed kind always `Process` and the expected kind varying between loop iterations, then passed on retry with no source change; `builders.rs` is not changed by this MR.

The rebase onto `70001f2b` replayed all commits without conflicts. `origin/dev` added no migration after 0334, so migration 0335 keeps its number; a later migration landing on `dev` before this merges would require renumbering it, because an applied migration must not be edited. The branch and MR !345 keep the name `TASK-529.2-agent-authorization`; renaming would close the MR and lose its review history. Every verification result recorded above was produced before this rebase and does not apply to the rebased commits until rerun.

### Verification on the rebased head `d0dfcba1`

All results below are for source revision `d0dfcba150f205a7f025abd8611e7308ae5743ea` (this family rebased onto `origin/dev` `70001f2b`, then renumbered). They replace the earlier pre-rebase results for the rebased commits.

- PostgreSQL/HTTP regressions PASSED: 53 nonignored tests, 0 failed, 0 ignored (1 restricted-grant, 28 admin-source, 24 OIDC), including the seven acting-Admin race tests.
- Full Access scenario PASSED: 47 of 47 cases; the evidence `meta.json` records source revision `d0dfcba1`.
- Embedded application build PASSED: Web UI 898 passed, server library 1588 passed, 0 failed.
- OKF validation PASSED: 200 concepts, 219 Mermaid blocks, 0 unclassified.
- Core package library tests PASSED on retry: 1586 passed, 0 failed, 856 ignored. The first attempt FAILED 15 tests, all in the Nix-parser deadline family ("Nix syntax validation unavailable: the Nix parser did not finish within 2 seconds"; `models::deployment_policies`, `handlers::api::deployment_policies`, `handlers::api::compliance`, `queries::compliance_interchange`). That is the known load-dependent farm flake tracked on `dev` as TASK-526; it ran while three other builds loaded the farm and no failing test touches authorization, OIDC or admin code. The retry ran on an otherwise idle farm. It is not counted as a pass for the failed attempt.
- Core package native post-checks (config inspection, observation, isolation, source parity, CA bundle, trust publication) are STILL RUNNING for this head; the earlier completed run was at `2c298432`. Do not treat the library result as the full package pass.

Still unverified: exact-head GitLab CI for `d0dfcba1`; narrow-screen, keyboard and error-state UI coverage; the system-detail and deploy-history surfaces that `origin/dev` changed, exercised only as an empty Systems page in the preview (no fixture systems); an unmocked OIDC provider callback.

The preview was refreshed to the rebased embedded build (output `l5r2k51li1nbzjrj8a7z8fnj85rzlq1f-cf-server-0.3.0`) with the database untouched: 317 migrations applied, checksum unchanged.
