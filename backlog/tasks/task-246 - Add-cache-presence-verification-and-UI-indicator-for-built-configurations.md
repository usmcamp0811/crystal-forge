---
id: TASK-246
title: Record tiered cache-presence observations for built configuration closures
status: Backlog
assignee: []
created_date: '2026-04-05 22:14'
updated_date: '2026-09-09 03:32'
labels:
  - cache
  - backend
  - api
  - database
  - observability
  - scanning
dependencies: []
references:
  - git commit e1b7434899e23f43770632e59d80a76a8fc8459e
  - TASK-440.1
documentation:
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/data-flake-explorer.js
modified_files:
  - packages/default/crates/cf-server/src/
  - packages/default/migrations
  - packages/web-ui/src/api/models.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

A successful build and push receipt prove that a configuration closure was available at one point, but they do not prove that the closure remains fetchable from a configured cache. Design commit `e1b74348` makes that distinction explicit because deployment and scanning readiness depend on current or honestly stale cache observations.

## Desired outcome

Record authoritative per-cache push receipts and timestamped closure-presence observations for exact built configuration revisions. Re-verify relevant revisions with a tiered policy: hot revisions are deployed or block a scan, warm revisions are recent and deployable, and cold revisions are checked only on demand. Expose bounded status that distinguishes present, partial, evicted, unverified, pending, and not-applicable states without polling one store path per request or presenting stale observations as current.

## Non-goals

- Do not change cache eviction policy.
- Do not automatically rebuild or repopulate evicted closures.
- Do not make Flake Explorer reads trigger cache traffic.
- Do not treat a historical push receipt as current presence.
- The Pipeline pane that consumes this contract is tracked by TASK-440.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every successful cache publication records an authoritative receipt for the exact cache destination revision configuration and closure paths with an observation timestamp
- [ ] #2 Presence observations distinguish present partial evicted unverified pending and not-applicable states and never infer current presence only from a historical push receipt
- [ ] #3 Hot revisions that are deployed or block a scan are re-verified at the configured short cadence and warm recent deployable revisions use a longer cadence
- [ ] #4 Cold revisions are not polled on a timer and an authorized explicit verification can refresh them on demand
- [ ] #5 Verification batches narinfo or equivalent checks per cache and tier with bounded work rate limits backoff and no request-path N+1 polling
- [ ] #6 Each per-cache verdict includes when it was observed and stale or missing observations remain explicitly unknown
- [ ] #7 Deploy scan and push operations opportunistically refresh observations when those operations already contact the cache
- [ ] #8 Bounded visibility-scoped APIs expose exact full-revision per-configuration closure status without disclosing hidden systems environments caches or signed URLs
- [ ] #9 Additive migrations preserve existing build and cache records and SQLx metadata matches all changed query shapes
- [ ] #10 Focused receipt scheduler batching staleness on-demand authorization and migration tests pass through the repository Nix environment and the observation contract is documented
<!-- AC:END -->
