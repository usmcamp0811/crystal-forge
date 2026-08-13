---
id: TASK-421
title: Paginate or search deployment-policy library server-side
status: Backlog
assignee: []
created_date: '2026-08-13 21:16'
labels:
  - scalability
  - policies
milestone: m-22
dependencies: []
priority: medium
type: feature
ordinal: 416000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The policies view loads the first 100 policies at startup and performs all search/filtering locally. This means policy 101+ is effectively invisible in the UI — it cannot be found by name search or any other filter once the library exceeds 100 entries.

Discovered during TASK-418 20aa browser test: after creating a new policy in an environment with enough existing policies, the new card was not discoverable via the first-100 list. The immediate workaround (insert the created record at the front of local state) was applied in TASK-418 but does not fix the underlying pagination gap.

The fix should change `load_policies()` (or the policies view) to either:
- Use server-side search/filter with a debounced query param so the user's typed search is sent to the API, or
- Implement infinite scroll / pagination using the existing `limit`/`offset` params on `GET /api/v1/deployment-policies`

Do not simply increase the limit to 10,000 — that postpones the defect and degrades startup performance.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy search/filter in the Policies view is served by the backend, not limited to the first 100 records in memory
- [ ] #2 A deployment with >100 policies does not cause any policy to be invisible in the UI
- [ ] #3 The first load is bounded and fast (e.g. first 20-50 policies, load more on scroll or search)
<!-- AC:END -->
