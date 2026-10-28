---
id: TASK-506
title: >-
  P1: Agent authenticated cache reads fail (Niks3 mTLS private CA) - restore
  deployments
status: In Progress
assignee:
  - '@agent'
created_date: '2026-10-25 17:03'
updated_date: '2026-10-25 17:03'
labels:
  - cache
  - agent
  - niks3
  - incident
dependencies: []
priority: high
type: bug
ordinal: 557000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Production incident: agent deployments from a Niks3 cache with mTLS read auth and a server-configured private read CA fail with only `Authenticated cache copy failed with status exit status: 1`. The authenticated `nix copy` child runs with `env_clear()` and discarded stderr, so the cause is invisible.

Contract: when a cache is configured in CF and assigned to an environment, CF supplies agents with everything needed to read it (URL, signing keys, client identity, CA trust, required Nix settings). Agents must not need host NixOS substituter/CA/experimental-feature configuration.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause of authenticated agent cache copy failure identified with local reproduction evidence against CF's packaged Nix
- [ ] #2 Authenticated agent nix copy works against a private-CA client-certificate-required binary cache with no host CA no host nix.conf experimental features and no ambient cache settings
- [ ] #3 Signature verification stays mandatory and TLS verification is not weakened
- [ ] #4 Authenticated copy failures report a bounded sanitized error category; no credentials PEM paths or raw Nix diagnostics are logged or returned
- [ ] #5 Credential files survive until child exit/kill and are removed afterward (existing guarantees preserved)
- [ ] #6 Public cache reads keep working
- [ ] #7 Follow-up gaps for Attic/HTTP/S3 read auth recorded as Backlog tasks
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Worktree /home/mcamp/code/crystal-forge/TASK-506-agent-cache-read-auth, branch TASK-506-agent-cache-read-auth, base origin/dev f0dea246. No production host access; reproduce locally only.
<!-- SECTION:NOTES:END -->
