---
id: TASK-522
title: Speed up view_system_deployment_status latest-system-state selection
status: Backlog
assignee: []
created_date: '2026-11-13 04:30'
labels:
  - performance
  - database
  - backend
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during TASK-521 (production HTTP 502 on the CVEs page). On the reckless production snapshot, one evaluation of `view_system_deployment_status` costs about 0.34 s: its latest-state subquery sorts every `system_states` row (about 170,000 rows, `Sort Method: external merge  Disk: 21000kB`) to keep one row per hostname. The same pattern makes the Systems page `view_system_list` page and count queries take about 1 s each; one logged `COUNT(*) FROM view_system_list` took 26 s.

TASK-521 only fenced the view inside the CVE projection so it runs once per statement. The view itself is still slow for every consumer.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The latest system state per host is selected without sorting all `system_states` rows (for example an index on `(hostname, timestamp DESC, id DESC)` with a per-host LATERAL lookup), with unchanged results
- [ ] #2 EXPLAIN ANALYZE on a production-sized snapshot shows no disk sort for `view_system_deployment_status` and `view_system_list`
- [ ] #3 Schema changes use a new migration and SQLx metadata is updated
<!-- AC:END -->
