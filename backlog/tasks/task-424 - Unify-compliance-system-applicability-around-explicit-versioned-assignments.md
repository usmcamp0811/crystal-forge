---
id: TASK-424
title: Unify compliance system applicability around explicit versioned assignments
status: In Progress
assignee:
  - '@Matt Camp'
created_date: '2026-08-16 15:17'
updated_date: '2026-08-16 15:48'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/316'
  - packages/default/crates/cf-server/src/queries/compliance.rs
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
  - packages/web-ui/src/views/compliance.rs
priority: high
type: bug
ordinal: 419000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The TASK-422 compliance view can report every system as applicable when a current published bundle has zero compliance_bundle_environments rows. Legacy applicability helpers interpret an empty environment-membership set as fleet-wide, while active compliance_bundle_assignments correctly represent explicit applicability. This produces contradictory results such as zero active assignments alongside all fleet systems being shown. Make the authoritative versioned assignment model determine applicability for every exact bundle version including the current published version. Empty active assignment scope must mean assigned nowhere; fleet-wide scope must be represented explicitly if supported. Align bundle summaries and all system/evidence applicability endpoints so they agree, while excluding inactive historical assignments.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A published bundle with zero active assignments and zero environment memberships reports zero applicable systems
- [ ] #2 A bundle with one active environment assignment returns only systems in that environment
- [ ] #3 A bundle with one active explicit system assignment returns only that system
- [ ] #4 Inactive historical assignments do not produce applicable systems
- [ ] #5 Draft current-published and non-current-published versions use the same assignment semantics
- [ ] #6 Bundle summary applicable_system_count matches the bundle systems endpoint
- [ ] #7 System bundles and evidence applicability agree with bundle summaries and bundle systems
- [ ] #8 No applicability helper treats empty legacy environment membership as fleet-wide
- [ ] #9 Regression tests cover zero assignments environment assignment explicit system assignment inactive history and version variants
- [ ] #10 MR !316 is blocked until the corrected applicability behavior is verified
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace current-published applicability fallback with exact bundle-version active assignment resolution.
2. Make the unversioned alias select the canonical published-first version and use the same explicit assignment query as all exact-version requests.
3. Update system-bundles and evidence applicability paths to use active versioned assignments and exclude inactive history.
4. Remove remaining compliance applicability predicates that treat no legacy environment rows as fleet-wide.
5. Add regression coverage for zero assignments, environment assignments, explicit system assignments, inactive history, and draft/published version consistency.
6. Run formatting and targeted/server verification in the dedicated worktree.

Approved scope addition: inspect design commit 68bde218 and align the shared web-ui PolicyCard description rendering with the design example using a two-line clamp and full-description title tooltip. Verify the policy UI build after the change.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Preflight completed: TASK-423 and TASK-424 are being implemented together because both correct the mixed legacy/versioned assignment model and both block MR !316. Dedicated worktree is /home/mcamp/code/crystal-forge/TASK-423-424-compliance-assignment-corrections on branch TASK-423-424-compliance-assignment-corrections from dev.

Implemented and pushed commit 11836d03 on origin/TASK-423-424-compliance-assignment-corrections. Current and exact-version applicability now use active explicit assignments; empty legacy environment membership no longer means fleet-wide; system bundles and evidence use the same assignment scope. PolicyCard now matches design commit 68bde218 with two-line clamping and a full-description title. Server build passed; web-ui build passed with 172 tests; cargo fmt and git diff checks passed. Focused database/browser validation remains.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: OpenAI
created: 2026-08-16 15:17
---
Implementation authorized by user now; this bug blocks MR !316. Work will be coordinated with TASK-423 in one focused compliance backend change set.
---
<!-- COMMENTS:END -->
