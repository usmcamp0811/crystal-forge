---
id: TASK-475
title: Restore runnable Attic and S3 regression VM fixtures
status: Backlog
assignee: []
created_date: '2026-10-03 18:21'
labels: []
dependencies: []
references:
  - checks/web-ui/default.nix
  - packages/cf-test-suite/cf_test/tests/conftest.py
priority: high
type: bug
ordinal: 496000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-470 verification found that the existing Web UI mega cache regression fixture requires minio.service and port 9000 while its VM supplies Garage on port 3900. The first Attic test fails during shared fixture setup; S3 and builder phases never run. The CF_WEB_UI_RUN_MEGA_PHASES variable is runtime-only and does not cross the Nix sandbox when supplied to nix build. User approved deferring this baseline fixture repair and documenting the Niks3 MR verification gap. No Attic/S3 pass is established.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented reproducible entry point runs the existing Attic and S3 cache regression phases rather than silently skipping them
- [ ] #2 Shared fixture startup matches the actual VM object-store service and endpoints
- [ ] #3 Attic and S3 tests execute and report their real results separately
- [ ] #4 The regression runner uses only isolated VM data and does not depend on a host development database
<!-- AC:END -->
