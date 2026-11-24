---
id: TASK-544.1
title: >-
  Skip undecryptable cache destinations at next-job dispatch and report
  cache_secret_undecryptable
status: To Do
assignee: []
created_date: '2026-11-24T23:27:00Z'
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
