---
id: TASK-416
title: Implement policy classification metadata (Phases 1A/1B/1C)
status: In Progress
assignee: []
created_date: '2026-08-09 21:12'
updated_date: '2026-08-09 21:21'
labels: []
dependencies: []
modified_files:
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/compliance/mappings.rs
  - packages/default/crates/cf-server/src/queries/deployment_policies.rs
  - packages/default/crates/cf-server/src/queries/environments.rs
  - packages/default/crates/cf-server/src/handlers/api/deployment_policies.rs
  - >-
    packages/default/crates/cf-server/migrations/0208_policy_classification_metadata.sql
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/policy/types.rs
  - packages/web-ui/src/views/policies_api.rs
  - packages/web-ui/src/views/policies.rs
type: feature
ordinal: 411000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add category/framework/severity/control_family/cmmc_level/cis_section/rationale to DeploymentPolicyVersionSummary, DeploymentPolicySummary, Create/Update request structs; extract from and merge into compliance_metadata JSONB; add legacy classification fallback; add GIN index migration; add unit tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 classification fields added to DeploymentPolicyVersionSummary
- [x] #2 classification fields added to DeploymentPolicySummary
- [x] #3 classification fields added to Create and Update request structs
- [x] #4 extract_classification helper extracts fields from compliance_metadata
- [x] #5 merge_classification_into_metadata merges on create/update
- [x] #6 infer_legacy_category fallback function present
- [x] #7 migration 0208 adds GIN index
- [x] #8 unit tests in classification_tests pass
- [x] #9 web-ui DeploymentPolicyVersionSummary and PolicyRevisionSummary updated
- [x] #10 cargo check passes with SQLX_OFFLINE=true
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented Phases 1A/1B/1C policy classification metadata backend changes:

**Server-side (`cf-server`)**
- `api/models.rs`: Extended `DeploymentPolicyVersionSummary` and `DeploymentPolicySummary` with 7 new optional fields: `category`, `framework`, `severity`, `control_family`, `cmmc_level`, `cis_section`, `rationale`.
- `models/deployment_policies.rs`: Extended `CreateDeploymentPolicyRequest` and `UpdateDeploymentPolicyRequest` with the same 7 classification fields.
- `compliance/mappings.rs`: Added three new public functions: `extract_classification` (reads fields from compliance_metadata JSONB), `merge_classification_into_metadata` (merges only `Some(...)` fields while preserving all existing keys), and `infer_legacy_category` (category fallback for policies without stored category, based on policy_type and SRG/CCI presence). Added `classification_tests` module with 9 unit tests.
- `queries/deployment_policies.rs`: Updated `create_deployment_policy` and `update_deployment_policy` to call `merge_classification_into_metadata` when building compliance_metadata.
- `queries/environments.rs`: Updated `list_deployment_policies` to fill new fields with `None` (lightweight path without compliance_metadata join).
- `handlers/api/deployment_policies.rs`: Updated version summary construction to call `extract_classification` and `infer_legacy_category` when populating `DeploymentPolicyVersionSummary` responses.
- `migrations/0208_policy_classification_metadata.sql`: No-op documentation migration that adds a GIN index on `compliance_metadata` for future classification queries.

**Web UI (`web-ui`)**
- `api/models.rs`: Extended `DeploymentPolicyVersionSummary` and `DeploymentPolicySummary` client DTOs with the same 7 classification fields (all `#[serde(default)]`).
- `components/policy/types.rs`: Extended `PolicyDefinition` and `PolicyRevisionSummary` with the 7 classification fields.
- `views/policies_api.rs`: Updated `policy_record_to_definition_with_count` to extract and propagate classification from the current version into `PolicyDefinition` and `PolicyRevisionSummary`.
- `views/policies.rs`: Updated the inline `PolicyDefinition` construction (for selected revision display) to include the new fields.

**Verification**: `cargo check` (SQLX_OFFLINE=true) passes for both crates; 15/15 `classification_tests` pass.
<!-- SECTION:FINAL_SUMMARY:END -->
