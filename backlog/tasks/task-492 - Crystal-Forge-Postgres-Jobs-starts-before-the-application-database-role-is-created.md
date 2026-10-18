---
id: TASK-492
title: >-
  Crystal Forge Postgres Jobs starts before the application database role is
  created
status: Backlog
assignee: []
created_date: '2026-10-18 03:24'
updated_date: '2026-10-18 03:24'
labels:
  - bug
  - nixos
  - postgresql
  - startup-order
references:
  - modules/nixos/crystal-forge/default.nix
  - /nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/crystal-forge.journal
priority: medium
type: bug
ordinal: 543000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A fresh NixOS Access VM starts `crystal-forge-postgres-jobs.service` after `postgresql.service` but concurrently with the module's PostgreSQL setup script. On the fresh empty database, run-postgres-jobs connects over `/run/postgresql` using configured user `crystal_forge` before setup creates that role, logs `FATAL: role "crystal_forge" does not exist`, and the oneshot service fails. The module setup later creates the role and the Crystal Forge server starts, so the Access bootstrap scenario still passes; this is an independent startup-order defect, not a failure of TASK-490.25 or .26. The production exception in the current overhaul assignment does not authorize fixing this issue.

Observed during the real Access VM at tested revision `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`, command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Evidence is `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/journals/controller/crystal-forge.journal`: jobs service attempted connection before role creation; later setup logged CREATE ROLE. Source is `modules/nixos/crystal-forge/default.nix` systemd service `crystal-forge-postgres-jobs` around lines 2542-2577, which orders only after PostgreSQL. Expected: the one-shot job does not run before its configured role/database setup is complete and exits successfully on fresh boot. Actual: it fails once with role missing. Verify that any ordering/retry fix preserves exactly-once/at-least-once semantics for the job scripts before implementation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A fresh NixOS boot does not run the Postgres jobs before its configured database role is available
- [ ] #2 The Postgres jobs service completes successfully after normal module database setup
- [ ] #3 The fix preserves the job runner's intended execution behavior without silently dropping or duplicating jobs
<!-- AC:END -->
