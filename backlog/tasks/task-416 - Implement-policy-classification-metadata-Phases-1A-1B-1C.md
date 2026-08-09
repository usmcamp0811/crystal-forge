---
id: TASK-416
title: Implement policy classification metadata (Phases 1A/1B/1C)
status: In Progress
assignee: []
created_date: '2026-08-09 21:12'
labels: []
dependencies: []
type: feature
ordinal: 411000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add category/framework/severity/control_family/cmmc_level/cis_section/rationale to DeploymentPolicyVersionSummary, DeploymentPolicySummary, Create/Update request structs; extract from and merge into compliance_metadata JSONB; add legacy classification fallback; add GIN index migration; add unit tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 classification fields added to DeploymentPolicyVersionSummary
- [ ] #2 classification fields added to DeploymentPolicySummary
- [ ] #3 classification fields added to Create and Update request structs
- [ ] #4 extract_classification helper extracts fields from compliance_metadata
- [ ] #5 merge_classification_into_metadata merges on create/update
- [ ] #6 infer_legacy_category fallback function present
- [ ] #7 migration 0208 adds GIN index
- [ ] #8 unit tests in classification_tests pass
- [ ] #9 web-ui DeploymentPolicyVersionSummary and PolicyRevisionSummary updated
- [ ] #10 cargo check passes with SQLX_OFFLINE=true
<!-- AC:END -->
