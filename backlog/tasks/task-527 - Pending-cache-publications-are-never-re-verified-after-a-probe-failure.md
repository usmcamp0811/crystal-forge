---
id: TASK-527
title: Pending cache publications are never re-verified after a probe failure
status: Backlog
assignee: []
created_date: '2026-11-16T14:06:00Z'
updated_date: '2026-11-16T15:44:00Z'
labels:
  - server
  - builders
  - caches
  - deployments
dependencies: []
priority: high
type: bug
ordinal: 575000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

On the owner's dev server (reckless), every campground `nixos` build since 2026-11-12 ~14:00 UTC has a `cache_push_jobs` row with `status = 'pending'`, `attempts = 0`, `error_message = 'cache_probe_failed'` (destination niks3). The last completed row is `7103359` (2026-11-12 02:13). The deployable-build rule in `view_system_deployment_status` requires a completed push, so auto_latest and the "Up to Date" chip stay on `7103359`, about 30 commits behind branch HEAD. The Builds view reports these builds as successful.

On 2026-11-16, `nix path-info --refresh --store https://niks3.lan.aicampground.com <path>` run by hand on reckless succeeded for the HEAD (`45afcde`) webb output, so the closure is in the cache now.

## Findings (code at dev `bd8d068d`)

- `cache_probe_failed` is `CacheProbeFailure::Process` in `handlers/api/builders.rs`: the fallback for any probe failure that the stderr classifier does not recognize (not 401/403/429/404/timeout/listed network strings). It also covers spawn failure, oversized output, and the server-side `path-info` exit when `verify_store_path_available_from_cache` sees a non-zero status. HTTP 5xx is not classified. A host build on 2026-11-15 observed niks3 returning HTTP 503.
- `Process` is non-retryable. The builder makes one completion attempt and the job is stored as pending publication.
- Nothing re-probes a pending publication later. `get_pending_cache_push_jobs` is consumed only by `builder/cache_worker.rs` (`run_cache_push_workers` / `run_cache_push_loop`), and no binary calls those functions. `retry_cache_push_job` only resets `failed`/`permanently_failed`/`cancelled` rows, not `pending`.
- Result: one transient cache error stalls deployability for that build permanently, with no UI signal.

## Desired outcome

- A pending publication is re-verified until it is confirmed, fails permanently with a visible reason, or is superseded. A transient niks3 outage self-heals.
- Server 5xx and similar responses are classified as retryable, with an operator-visible diagnostic (no secrets or signed URLs).
- Operators can see "build succeeded, publication pending: <reason>" in Builds/Caches and can trigger a re-verify.
- Recovery note for existing rows on the dev server.

## Status

Backlog. Discovered while reviewing TASK-525.1 (MR !343); not part of that MR.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner diagnosis 2026-11-16 on reckless: failure is not transient. Probe fails in ~81 ms on every build. The niks3 destination (id 2) was edited 2026-11-12 02:27 UTC (14 min after the last confirmed push, 7103359) to niks3_read_auth_mode=mtls. The same path-info run as an ordinary user with the server nix 2.35.3, stripped env and no client cert succeeds, so reads do not require mTLS. In mTLS mode the probe adds ?tls-certificate=&tls-private-key= to the store URL and, when a CA is configured, sets NIX_SSL_CERT_FILE to only that CA (cf-config cache_credentials.rs PreparedCacheRead). Suspected: configured CA does not validate the niks3 server certificate, or the store does not accept those parameters; Nix stderr is discarded into cache_probe_failed, so the server cannot say which. Additional required outcome: classify TLS/certificate errors and surface a redacted diagnostic; validate mTLS read settings when the destination is saved.

Resolution 2026-11-16: root cause was a wrong cache read URL on the niks3 destination (edited 2026-11-12). Owner corrected it and publications now verify. The defects in this task remain: no re-verification of pending publications, Nix errors discarded into cache_probe_failed, no save-time validation. Operator-visibility work is split into TASK-528.
<!-- SECTION:NOTES:END -->
