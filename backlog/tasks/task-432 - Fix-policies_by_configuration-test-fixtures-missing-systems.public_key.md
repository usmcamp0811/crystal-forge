---
id: TASK-432
title: Fix policies_by_configuration test fixtures missing systems.public_key
status: Backlog
assignee: []
created_date: '2026-08-21 23:57'
labels:
  - tests
  - live-db
dependencies: []
type: bug
ordinal: 432000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Six live-DB tests in `queries::deployment_policies::tests::policies_by_configuration_*` fail against the dev database with:

```
null value in column "public_key" of relation "systems" violates not-null constraint
```

The test fixture at `crates/cf-server/src/queries/deployment_policies.rs:2292` inserts into `systems` without supplying `public_key`, which is NOT NULL.

Confirmed pre-existing: all six fail identically on commit `3cbd53f3`, before the collation-ordering work. Discovered while verifying the policy pagination collation fix; out of scope for that task.

Affected tests:
- policies_by_configuration_different_flakes_do_not_leak
- policies_by_configuration_disabled_policy_excluded
- policies_by_configuration_duplicate_assignment_deduplicated
- policies_by_configuration_hostname_fallback
- policies_by_configuration_inactive_system_excluded
- policies_by_configuration_two_environments_same_flake

Reproduce:
```
cd packages/default
CRYSTAL_FORGE_TEST_DATABASE_URL="postgresql://crystal_forge:password@127.0.0.1:3042/crystal_forge" \
  SQLX_OFFLINE=true nix develop -c cargo test -p cf-server --lib -- \
  queries::deployment_policies::tests::policies_by_configuration --ignored --test-threads=1
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The six policies_by_configuration_* tests pass against a live dev database
- [ ] #2 The systems insert fixture supplies a valid non-null public_key
- [ ] #3 No production query or schema behavior is changed
<!-- AC:END -->
