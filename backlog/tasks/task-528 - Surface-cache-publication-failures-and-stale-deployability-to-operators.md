---
id: TASK-528
title: Surface cache publication failures and stale deployability to operators
status: Backlog
assignee: []
created_date: '2026-11-16T15:43:00Z'
labels:
  - web-ui
  - server
  - caches
  - deployments
  - alerts
dependencies:
  - TASK-527
priority: high
type: feature
ordinal: 576000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

On 2026-11-12 the niks3 cache destination on the owner's dev server was saved with a wrong cache read URL. Uploads kept working, but the server's post-build availability check (`nix path-info` against the read URL) failed in about 80 ms on every build. For four days:

- every campground build showed "Complete" in Builds;
- the cache push rows stayed `pending` with `cache_probe_failed` and 0 attempts;
- `view_system_deployment_status` kept `7103359` as the newest deployable build;
- the Systems list showed "Up to Date", auto_latest kept targeting `7103359`, and the Deploy tab lane showed hosts 30 commits behind HEAD.

No surface said that publication was failing or why. Finding the cause took several rounds of manual SQL and shell reproduction (see TASK-525.1 review notes and TASK-527).

## Goal

A misconfigured or failing cache destination becomes obvious to an operator in the UI, at the place where they would notice the effect, with the actual reason, without SQL or shell access.

## Desired behavior (to refine into acceptance criteria)

- **Save-time validation:** saving or editing a cache destination runs the same read check the server uses (read URL, keys, read auth, CA) and blocks or warns with the redacted Nix error. A "Test read" action exists for an unchanged destination.
- **Destination health:** the Caches view shows each destination's last successful and last failed availability check, failure count since the last success, and the redacted reason (for example "TLS: unable to get local issuer certificate", "HTTP 404 at <read URL>/nix-cache-info").
- **Attention and alerts:** when a destination's checks fail repeatedly, or builds stay in pending publication beyond a threshold, an attention item and notification appear (sidebar badge or bell), linked to the destination.
- **Builds:** a build whose output is not confirmed in the cache shows "Built · publication pending: <reason>" instead of plain "Complete".
- **Deploy staleness:** when the newest deployable build is older than branch HEAD because newer builds are pending publication, the Systems list chip, the Overview, and the Deploy tab lane say so (for example "Up to date with newest deployable 7103359 · 30 newer builds waiting on cache publication").

## Constraints

- Diagnostics MUST NOT expose credentials, client keys, signed URLs, or response bodies; reuse the existing redaction rules in `classify_cache_probe_failure`.
- Server owns the checks and health state; the Web UI reads them through APIs.
- Re-verification and error classification are owned by TASK-527; this task consumes their results.

## Dependencies

TASK-527 (re-verify pending publications, classify probe errors).

## Status

Backlog. Raised by the owner while reviewing TASK-525.1 (MR !343).
<!-- SECTION:DESCRIPTION:END -->
