---
id: TASK-476
title: Make cf-test-suite cleanup_test_data delete scenario rows reliably
status: Backlog
assignee: []
created_date: '2026-10-03 18:24'
labels:
  - test-suite
  - database
dependencies: []
modified_files:
  - packages/cf-test-suite/cf_test/__init__.py
priority: low
type: bug
ordinal: 497000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
CFTestClient.cleanup_test_data builds each delete by checking whether the pattern contains the substring WHERE. A pattern whose subquery contains WHERE, such as the progressive scenario's agent_heartbeats pattern "system_state_id IN (SELECT id FROM system_states WHERE hostname LIKE ...)", is therefore treated as a complete clause and produces invalid SQL ("DELETE FROM agent_heartbeats system_state_id IN ..."). The resulting error aborts the whole transaction, so nothing is deleted and the final commit silently rolls back. Scenario-owned cleanup deletes no rows. The suite only stays clean today because a separate clean_test_data fixture removes test hosts by name pattern. Tests that rely on cleanup_test_data alone leave rows behind and fail on the next run with unique violations such as systems_hostname_key. Found while reproducing a config-timeline failure on a disposable database.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A cleanup pattern that contains WHERE only inside a subquery is applied as a condition and deletes its rows.
- [ ] #2 A statement that fails is reported and does not discard the deletions that succeeded, or the failure stops the test loudly instead of rolling back silently.
- [ ] #3 A regression test proves a scenario's own cleanup removes its heartbeats, states, systems, derivations, commits and flake without the shared clean_test_data fixture.
- [ ] #4 No scenario's expectations change.
<!-- AC:END -->
