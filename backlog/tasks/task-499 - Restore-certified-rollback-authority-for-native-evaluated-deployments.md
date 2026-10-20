---
id: TASK-499
title: Restore certified rollback authority for native-evaluated deployments
status: To Do
assignee: []
created_date: '2026-10-20 04:21'
updated_date: '2026-10-20 20:30'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/models/primary_evaluation.nix
  - packages/default/crates/cf-server/src/services/composite_enforcement.rs
  - packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - docs/knowledge/evaluation/evaluation-snapshot-retention-and-rollback.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17305508230'
priority: high
type: bug
ordinal: 550000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Restore certified retained-generation rollback authority for real CF-managed native deployments. At df11c800 two actual UI-requested deployments and changed-state heartbeat correlation succeeded, but both deployment snapshot bindings were null and the first generation remained rollback-ineligible. Native PRIMARY produces exact build identity but no available V1 Config inventory; Inspector V2 has an independent selector and cannot be assumed to authorize retained rollback. The owner now explicitly authorizes the smallest coherent producer/binding/retention repair while preserving exact immutable lineage, old artifact semantics, read-only historical reads and bounded fast evaluation. A forward migration is allowed only for the genuinely required typed artifact representation; no historical recertification or authority seeding.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted native deployment workflow obtains certified rollback authority with exact system/configuration/commit/derivation/output identity.
- [ ] #2 Two actual CF-managed native deployments permit authorized retained-generation UI rollback and actual target restoration without seeded authority.
- [ ] #3 Artifact schema versions and selector/retention boundaries remain explicit and are covered by regressions and updated source documentation.
- [ ] #4 Missing or mismatched certified lineage remains ineligible and cannot be bypassed by a supplied store path.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified baseline: df11c8005de5ab7088015757dafc7fa7602da188; hosted pipeline2966113736 Deliveryjob17305508230 failed mandatory rollback eligibility after13passingcases. Access23 and database517/0/0 passed; component/CVE/dashboard passed; complexity/coverage skipped. Downloaded /tmp/opencode/hosted-delivery-df11c800.json confirms fresh generation1 is not current after B, retained UUID null, eligibility false. The current owner assignment authorizes implementation. Prior nonauthorization wording is obsolete and removed. Research identifies a missing bounded native deployment-authority producer; do not solve by accepting V2 as V1 or restoring an eager full Config crawl.
<!-- SECTION:NOTES:END -->
