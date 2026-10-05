---
id: TASK-480
title: Fix stale HTTP 426 text in builder cache-credential comments and NixOS option
status: Backlog
assignee: []
created_date: '2026-10-05 01:43'
labels:
  - docs
  - builder
  - cache
dependencies: []
references:
  - docs/knowledge/builders/remote-build-execution-strategies.md
  - docs/knowledge/meta/cleanup-record.md
priority: low
type: chore
ordinal: 501000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during the TASK-478 documentation review at commit 3b23d36f.

Three places say that an unverified credential-bearing builder cache job gets HTTP 426 Upgrade Required:

- packages/default/crates/cf-config/src/config/server.rs (doc comment on trust_forwarded_builder_https)
- packages/default/crates/cf-server/src/config/server.rs (same comment)
- modules/nixos/crystal-forge/default.nix (description of the matching option)

The code does not send 426. `get_next_job` fails the claimed job as a transient `[dispatch:cache_config]` failure through `mark_job_failed_with_retry` and answers HTTP 404 (`fail_claimed_job_at_dispatch` in cf-server handlers/api/builders.rs). The knowledge base already states the correct behavior in docs/knowledge/builders/remote-build-execution-strategies.md.

Note: TASK-470 is changing credential dispatch. Coordinate with it before editing these lines.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The three comments and option text describe the 404 and transient cache_config failure, not 426.
- [ ] #2 No behavior change; the diff touches only comments and option description text.
- [ ] #3 NixOS module option documentation output is rebuilt or checked if the option description changes.
<!-- AC:END -->
