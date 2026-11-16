---
id: TASK-527
title: Pending cache publications are never re-verified after a probe failure
status: Backlog
assignee: []
created_date: '2026-11-16T14:06:00Z'
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
