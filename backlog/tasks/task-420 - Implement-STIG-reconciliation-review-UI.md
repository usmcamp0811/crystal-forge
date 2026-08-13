---
id: TASK-420
title: Implement STIG reconciliation review UI
status: Backlog
assignee:
  - '@agent'
created_date: '2026-08-13 02:21'
updated_date: '2026-08-13 02:30'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
  - packages/web-ui
  - 861fd877
documentation:
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
modified_files:
  - packages/web-ui
priority: high
type: feature
ordinal: 415000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the production STIG reconciliation/review UI using the existing reconciliation preview and candidate APIs. Present authoritative, inherited, exact technical, related CCI/SRG, and no-candidate states with appropriate review and reuse semantics. Defer fuzzy/title similarity matching. Include the policy mapping relationship/coverage/rationale workflow needed by reviewed exact and related reuse, and preserve loading, empty, error, and authorization states.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 STIG import reconciliation presents requirement states and candidate match types from the backend
- [ ] #2 Authoritative candidates are shown as auto-resolved and related candidates require explicit review
- [ ] #3 Reviewed exact and related reuse capture explicit relationship coverage and rationale semantics
- [ ] #4 No-candidate requirements support create refine or manual handling without inventing evidence
- [ ] #5 Loading empty error and authorization states remain represented
- [ ] #6 Policy mapping add/edit workflow exposes relationship coverage and rationale consistently
- [ ] #7 Fuzzy title or semantic similarity matching is not introduced
- [ ] #8 Targeted frontend tests or checks cover the review and mapping interactions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation plan
1. Inspect the existing web-ui architecture, STIG import/reconciliation API models, policy mapping add/edit views, and the 861fd877 design reference.
2. Identify the smallest production UI surface that can present reconciliation states and candidate evidence without adding fuzzy matching or changing backend contracts.
3. Implement the reconciliation review workflow with explicit candidate selection, review semantics, relationship, coverage, and rationale fields while preserving loading, empty, error, and authorization states.
4. Implement or align policy mapping add/edit controls with the same relationship, coverage, and rationale semantics.
5. Add targeted frontend behavior tests or the repository-authoritative web-ui test coverage for review and mapping interactions.
6. Verify with focused web-ui checks, formatting, diff checks, and any required Rust/server contract checks. Skip the full web-ui build only if the targeted checks adequately cover the changed surface and document the exception.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @agent
created: 2026-08-13 02:22
---
Implementation preflight: dedicated worktree `/home/mcamp/code/crystal-forge/TASK-420-stig-reconciliation-ui`, branch `TASK-420-stig-reconciliation-ui`, based on `dev`. Scope is production STIG reconciliation/review and policy mapping UI only; fuzzy matching remains deferred. Verification will use targeted web-ui checks, formatting, diff checks, and server contract checks as applicable; the long web-ui build remains omitted unless required by the changed surface.
---

author: @agent
created: 2026-08-13 02:30
---
Closed as an accidental split. The UI scope belongs to TASK-418. No application changes were made in the TASK-420 worktree.
---
<!-- COMMENTS:END -->
