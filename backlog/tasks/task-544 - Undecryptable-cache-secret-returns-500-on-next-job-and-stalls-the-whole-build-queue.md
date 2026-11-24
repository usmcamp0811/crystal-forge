---
id: TASK-544
title: >-
  Undecryptable cache secret returns 500 on next-job and stalls the whole build
  queue
status: Backlog
assignee: []
created_date: '2026-11-24T21:41:00Z'
labels:
  - builders
  - cache
  - security
dependencies: []
references:
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
  - packages/default/crates/cf-server/src/security/cache_secrets.rs
  - packages/default/crates/cf-server/src/queries/cache_destinations.rs
priority: high
type: bug
ordinal: 579000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Observed on reckless (production, CF dev @ f86ff39f, still present on dev @ ef3a14bb) on 2026-11-24. The stored cache credential (cache_destinations.attic_token, enc:v1) could not be decrypted with the server's current CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY. Effects:

- POST /api/v1/builders/:id/next-job returns 500 with an empty body on every poll. resolve_cache_destinations_for_derivation (cf-server handlers/api/builders.rs:265) maps every error except AmbiguousDerivationEnvironment to INTERNAL_SERVER_ERROR. The preflight runs on the queue head before the claim, so the same head job (derivation_id=3424580) fails forever and blocks all 286 queued jobs for every builder.
- The server logs only 'failed to resolve canonical cache destinations: failed to decrypt cache secret' at WARN, every 5 s per builder. It does not name the cache destination or the column.
- The builder logs 'Get next job failed with status 500 Internal Server Error:' with nothing after it.
- Builders still heartbeat, so the UI shows them active and ready. Nothing in the UI shows that dispatch is failing.
- The CVE worker fails with the same error ('Error materializing scan inputs ... failed to decrypt cache secret'), so CVE scans stop too.
- Nothing checks at startup that the configured key decrypts the stored secrets. A changed or wrongly rendered key file (here, from the fmf crystal-forge-setup Vault template) is detected only through these side effects.

## Goal
A cache secret that cannot be decrypted is a visible, actionable configuration fault scoped to the affected cache destination. It does not stall unrelated work, and it does not produce opaque 500 responses.

## Non-goals
- No key rotation or re-encryption tooling (separate task if wanted).
- No change to the encryption format, the key derivation, or the HTTPS-only credential delivery rule.
- Do not fall back to dispatching without credentials when the destination requires them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At server startup and when cache destinations change, the server tries to decrypt every enc:v1 value in cache_destinations. On failure it logs one ERROR naming the destination id/name and column (never the value) and shows the fault in admin config-health (GET /api/v1/admin/config-health) and on the Caches page.
- [ ] #2 A next-job poll whose head candidate needs an undecryptable cache destination does not return 500. It returns 409 with a structured, credential-free reason (for example cache_secret_undecryptable plus the destination id), or it skips that candidate so other claimable jobs still dispatch. The chosen behavior is documented. A single bad destination cannot block jobs that do not use it.
- [ ] #3 The builder logs the server's error code and reason for every non-success next-job response instead of an empty body.
- [ ] #4 Builders and jobs blocked by this fault show a visible state in the Builders / build queue UI. The UI must not show 'ready' while every poll fails.
- [ ] #5 The CVE worker records the same fault as a classified scan failure that names the destination, not as a repeating generic error.
- [ ] #6 Regression coverage in the owning integration scenario (integration-delivery): set a destination secret encrypted with key A and start the server with key B. Assert the config-health fault, the non-500 next-job response, that unrelated jobs still dispatch, and that no secret appears in logs or responses.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Diagnosis 2026-11-24: builders reckless and webb both enabled and active with fresh heartbeats. 286 queued jobs, all passing artifact/environment predicates. Builder journal shows a 500 every 5 s. Server journal shows the decrypt WARN for derivation_id=3424580 at the same moments. Root cause on the host: the cache-encryption-key file content changed. It is under investigation in fmf modules/nixos/services/crystal-forge (Vault template added 2026-11-23).
<!-- SECTION:NOTES:END -->
