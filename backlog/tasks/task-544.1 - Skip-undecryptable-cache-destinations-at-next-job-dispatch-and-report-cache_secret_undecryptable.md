---
id: TASK-544.1
title: >-
  Skip undecryptable cache destinations at next-job dispatch and report
  cache_secret_undecryptable
status: In Progress
assignee: []
created_date: '2026-11-24T23:27:00Z'
updated_date: '2026-11-26T01:32:26Z'
labels:
  - builders
  - cache
  - security
dependencies: []
parent_task_id: TASK-544
priority: high
type: bug
ordinal: 580000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-544 slice A. A cache secret that cannot be decrypted makes next-job preflight return an empty 500 for the queue head on every poll. The head job is never failed or skipped, so every queued job for every builder is blocked (production incident on reckless, 2026-11-24). The server log names neither the destination nor the column.

## Goal
A bad cache destination only affects jobs that resolve to it. Other jobs still dispatch. When nothing dispatchable remains, the builder gets a structured, credential-free 409 and the server logs one actionable ERROR.

## Non-goals
- No startup validation, config-health or Caches page change (TASK-544.2).
- No UI change (TASK-544.3). No CVE worker change (TASK-544.4).
- No change to key derivation, the enc:v1 format, or HTTPS-only credential delivery.
- Post-claim paths (output publish, closure publish) keep their current status mapping.
- No dispatch without credentials when the destination requires them.

## Architectural constraints
- cf-protocol: add NextJobConflictReason::CacheSecretUndecryptable (wire name cache_secret_undecryptable). NextJobConflictResponse keeps its shape. No destination identifier on the wire, so the credential-free response does not reveal configuration to builders. Older builders treat the unknown reason as an unstructured 409, return no job and log a warning (additive, deployed-builder compatible).
- cf-server queries/cache_destinations.rs: decrypt_destination_secrets returns a typed error that carries destination id, destination name and column name, never the value. Its Display text still contains 'failed to decrypt cache secret'.
- cf-server handlers/api/builders.rs get_next_job: a candidate whose cache preflight fails with that typed error marks the candidate's environment (or the global/NULL environment) blocked for this poll and selects the next candidate. Destinations depend only on the job environment (eligible_cache_destinations_for_environment), so one failure covers every job in that environment. Both peek_next_server_derivation_job and peek_next_verified_source_job take the blocked set. The claim stays limited to the preflighted job id (existing CONCURRENCY invariant).
- When the peek finds nothing and at least one environment was blocked, return 409 cache_secret_undecryptable, not 404.
- The server logs one ERROR per poll per blocked destination with destination id, name and column. No secret value or ciphertext in any log or response.

## Verification plan
Nix farm builds only (nix build, no local cargo). cf-protocol wire-name test. cf-server unit test of the typed decrypt error with a wrong key, and a database-backed test of both peek functions with a blocked environment (isolated PostgreSQL per docs/agents/database-safety.md). cargo fmt check (rustfmt --check may run locally). nix build .#okf-knowledge -L.

## Impact areas
cf-protocol src/builder.rs; cf-server queries/cache_destinations.rs, queries/builders.rs, handlers/api/builders.rs; cf-builder builder/api_client.rs; docs/knowledge builder dispatch and cache secrets concepts.

## Risk
Medium. next-job is security- and concurrency-sensitive, and slice A edits candidate selection. Mitigation: keep the claim limited to the preflighted id, add no new claim path, and test the blocked and unblocked cases.

## Dependencies
None. Slices 544.2, 544.3 and 544.4 stack on this one.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A cache destination whose secret cannot be decrypted does not make next-job return 500. Jobs whose environment resolves to a healthy destination still dispatch in the same poll.
- [ ] #2 When no dispatchable candidate remains and an environment was blocked, next-job returns 409 with reason cache_secret_undecryptable and does not claim or mutate any job.
- [ ] #3 The server logs one ERROR per blocked destination per poll naming destination id, name and column; no secret value, ciphertext or key appears in logs or responses.
- [ ] #4 The builder logs the cache_secret_undecryptable reason as a warning, and an empty error body from the server is reported as 'no response body' instead of a blank.
- [ ] #5 A wire-name test covers the new reason; a unit test covers the typed decrypt error with a wrong key; a database test covers both peek functions with a blocked environment and with the NULL environment blocked.
- [ ] #6 docs/knowledge describes the new 409 reason, the per-environment skip and the log; nix build .#okf-knowledge -L passes.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on vm-agent in /home/mcamp/code/crystal-forge/TASK-544.1-cache-secret-dispatch

STATUS 2026-11-24 (commit 2560b510 on TASK-544.1-cache-secret-dispatch, local only, not pushed, no MR yet). Implemented: typed CacheSecretUndecryptable, BlockedEnvironments + *_excluding peeks, per-poll skip loop, 409 cache_secret_undecryptable, ERROR log, builder warning + empty-body text, docs/knowledge. rustfmt --check clean (nix run nixpkgs#rustfmt, edition 2024). VERIFIED on the farm (cf-server-core-drv, checkPhase): new tests pass in both runs: blocked_environments_hold_each_environment_once, undecryptable_secret_names_destination_and_column_without_the_value, plaintext_and_absent_secrets_are_not_a_fault, cf_protocol next_job_conflict_reasons_have_stable_wire_names (cf_protocol 45 passed). NOT VERIFIED / BLOCKED: (1) cf-server-core checkPhase fails 16 unrelated tests (deployment_policies, compliance, compliance_interchange: 'Nix syntax validation unavailable: the Nix parser did not finish within 2 seconds'); the TASK-542.1 branch on the same dev base shows the same 16, and one of its runs passed 1635/0, so this is load-dependent, not this change; a run with 0 failures is still needed. (2) cf-builder-drv: three runs failed before compiling, farm niks3 publish returns 503 'IO error: No space left on device (os error 28)' for one NAR (Garage out of space); builder change is one match arm plus a message tweak, untested by compile. (3) nix build .#okf-knowledge fails on dev itself (TASK-545: arrow chain in TASK-542 Goal); all 58 validator tests pass and STALE=0, the only finding is that line. (4) blocked_environments_hide_only_their_own_jobs_from_both_peeks is #[ignore] (needs isolated migration-backed PostgreSQL) and has not been run. Do not move to Review until 1-4 are resolved.

VERIFICATION 2026-11-25. MR !357 (Draft). Head bbdbad19. Evidence is at 7d15cdce (pipeline 3114292824) unless stated; bbdbad19 differs only in one PostgreSQL test body (pipeline 3116744599, running). ACs are left unchecked until the head pipeline passes. EXECUTED AND PASSED: component-tests job: cf-server-core checkPhase 1638 passed 0 failed (with --cores 2); okf-knowledge UNCLASSIFIED=0 STALE=0 (TASK-545 reword incorporated as commit d9ad5a94); server unit tests only_the_typed_decryption_fault_sets_an_environment_aside, blocked_queue_answer_is_a_credential_free_reason_and_empty_queue_stays_404, undecryptable_secret_names_destination_and_column_without_the_value, plaintext_and_absent_secrets_are_not_a_fault, blocked_environments_hold_each_environment_once, update_and_read_gate_reject_combined_certificate_private_key_fields; cf-protocol next_job_conflict_reasons_have_stable_wire_names and rolling_upgrade_older_builders_reject_only_the_new_reason. Builder: nix log of cf-builder-drv at head shows cache_secret_conflict_takes_no_job_and_the_client_keeps_polling ok, 101 passed 0 failed (output was substituted from niks3, so the Garage publish failure no longer blocks it). state-machine-tests job, PostgreSQL, registered under cache_secret_dispatch_: 7 of 8 passed: blocked_environments_hide_only_their_own_jobs_from_both_peeks, empty_queue_stays_404, fully_blocked_queue_answers_409_and_changes_nothing, global_fault_hits_only_environments_that_fall_back_to_it, mixed_queue_skips_bad_environment_and_keeps_priority, recovers_without_a_restart_when_the_credential_is_fixed, unrelated_failure_still_fails_the_poll. FAILED, and fixed in bbdbad19: concurrent_polls_claim_each_healthy_job_once. Cause was a wrong assertion, not a code fault: a poll whose previewed job another builder took gets 404 and must not substitute another job, so only one of two healthy jobs was claimed in the round; nothing was claimed twice. The test now asserts at least one winner, no double claim, only healthy jobs, an untouched undecryptable head, then drains the rest. NOT YET VERIFIED: (1) state-machine-tests at bbdbad19, including the rewritten concurrency test and the phases after it, which did not run on 7d15cdce because the job stops at the first failure; (2) integration-access and integration-delivery passed at 7d15cdce; integration-assurance and integration-upgrade hit the 2 h job timeout. No trace line mentions cache_secret_undecryptable, decrypt, or next-job. Assurance was still compiling cf-server at the kill. Upgrade loops on 'path nixos-system-targetA-test is not valid' in version-transition. Other MRs passed both in 1772 to 3934 s, so rerun at head and compare; (3) the log line is asserted for destination name and column, not for destination_id (it is logged); (4) the builder warning text for the new reason is not asserted, only the no-job-and-keep-polling behavior. PARSER TIMEOUTS: the 16 'Nix syntax validation unavailable ... did not finish within 2 seconds' failures appeared only on farm runs without a core limit (3 runs, 16 to 17 failures) and not on the CI runner run with --cores 2 (0 failures); the validator runs nix-instantiate with a fixed 2 s deadline, so many parallel tests on a loaded builder exceed it. It is a test-reliability problem in unrelated code, left unchanged per instructions.
<!-- SECTION:NOTES:END -->
