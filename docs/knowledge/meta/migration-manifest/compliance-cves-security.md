---
type: Reference
title: "Migration manifest: compliance, CVEs, and security"
description: "Records where each source document of the compliance, CVEs, POA&M, and security group went in the OKF migration, with section-level mapping and coverage status."
tags:
  - crystal-forge
  - migration
---
# Migration manifest: compliance, CVEs, and security

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/fleet-cve-triage.md` | [cves/fleet-cve-triage.md](../../cves/fleet-cve-triage.md), [poam/cve-poam-verification-and-closure.md](../../poam/cve-poam-verification-and-closure.md) | split | complete |
| `docs/stig-modules.md` | [compliance/stig-modules.md](../../compliance/stig-modules.md) | moved | complete |
| `docs/operator/compliance-interchange.md` | [compliance/cf-xccdf-interchange-operator-guide.md](../../compliance/cf-xccdf-interchange-operator-guide.md), [compliance/assignments-and-report-only-enforcement.md](../../compliance/assignments-and-report-only-enforcement.md) | split | complete |
| `docs/auth-session-security.md` | [security/session-cookies-and-csrf.md](../../security/session-cookies-and-csrf.md) | moved | complete |
| `docs/auth-role-mapping.md` | [security/oidc-role-mapping.md](../../security/oidc-role-mapping.md) | moved | complete |
| `docs/auth-provider-compatibility-validation.md` | [security/oidc-provider-compatibility-validation.md](../../security/oidc-provider-compatibility-validation.md) | moved | complete |
| `docs/task-433-design-parity-review.md` | [historical/task-433-design-parity-review.md](../../historical/task-433-design-parity-review.md) | moved | complete |
| `docs/design/CrystalForge/docs/crystal-forge-compliance-design/README.md` | retained in place; see [ui/compliance-view-design.md](../../ui/compliance-view-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-compliance-design/compliance-view-design-v0.1.md` | retained in place; see [ui/compliance-view-design.md](../../ui/compliance-view-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-compliance-design/cross-view-contract-ledger-v0.1.md` | retained in place; see [ui/cross-view-contract-ledger.md](../../ui/cross-view-contract-ledger.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-compliance-design/verification.md` | retained in place; see [ui/compliance-view-design.md](../../ui/compliance-view-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-cves-design/README.md` | retained in place; see [ui/cves-view-design.md](../../ui/cves-view-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-cves-design/cves-view-design-v0.1.md` | retained in place; see [ui/cves-view-design.md](../../ui/cves-view-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-cves-design/verification.md` | retained in place; see [ui/cves-view-design.md](../../ui/cves-view-design.md) | retained | complete |
| `docs/design/CrystalForge/cve-poam-evidence-continuity-design-spec.md` | retained in place; see [poam/cve-poam-evidence-continuity-design.md](../../poam/cve-poam-evidence-continuity-design.md) | retained | complete |
| `docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md` | retained in place; see [compliance/cf-xccdf-interchange-profile.md](../../compliance/cf-xccdf-interchange-profile.md) | retained | complete |
| `backlog/docs/design/doc-12 - Compliance-implementation-roadmap.md` | retained in place; see [compliance/compliance-implementation-roadmap.md](../../compliance/compliance-implementation-roadmap.md) | retained | complete |
| `backlog/docs/doc-20 - TASK-412-Slice-2-Implementation-Summary.md` | retained in place; see [historical/task-412-implementation-records.md](../../historical/task-412-implementation-records.md) | retained | complete |
| `backlog/docs/doc-21 - TASK-412-Complete-Implementation-Slice-1-5-Verification-Summary.md` | retained in place; see [historical/task-412-implementation-records.md](../../historical/task-412-implementation-records.md) | retained | complete |
| `backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md` | retained in place; see [ui/compliance-ui-redesign-spec.md](../../ui/compliance-ui-redesign-spec.md) | retained | complete |
| `schemas/cf-xccdf-1/PROVENANCE.md` | retained in place; see [references/schema-provenance.md](../../references/schema-provenance.md) | retained | complete |
| `schemas/oscal-1.1.2/poam-provenance.txt` | retained in place; see [references/schema-provenance.md](../../references/schema-provenance.md) | retained | complete |

## Source inventory

### `docs/fleet-cve-triage.md`

- Title: Fleet CVE Triage Operator Guide
- Purpose: Operator procedure for per-environment fleet CVE triage, batch triage, risk acceptance register, conflicts, verification, and bounds.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Purpose` | [cves/fleet-cve-triage.md#purpose](../../cves/fleet-cve-triage.md#purpose) |
  | `## Inventory Relations` | [cves/fleet-cve-triage.md#inventory-relations](../../cves/fleet-cve-triage.md#inventory-relations) |
  | `## Dispositions` | [cves/fleet-cve-triage.md#dispositions](../../cves/fleet-cve-triage.md#dispositions) |
  | `## Procedure` | [cves/fleet-cve-triage.md#procedure](../../cves/fleet-cve-triage.md#procedure) |
  | `## Batch triage` | [cves/fleet-cve-triage.md#batch-triage](../../cves/fleet-cve-triage.md#batch-triage) |
  | `## Risk acceptance register` | [cves/fleet-cve-triage.md#risk-acceptance-register](../../cves/fleet-cve-triage.md#risk-acceptance-register) |
  | `## Conflicts` | [cves/fleet-cve-triage.md#conflicts](../../cves/fleet-cve-triage.md#conflicts) |
  | `## Verification` | [poam/cve-poam-verification-and-closure.md#verification](../../poam/cve-poam-verification-and-closure.md#verification) |
  | `## Bounds` | [cves/fleet-cve-triage.md#bounds](../../cves/fleet-cve-triage.md#bounds) |
- Unmapped content: none

### `docs/stig-modules.md`

- Title: Crystal Forge STIG Module System
- Purpose: Describes the mkStigModule-based NixOS STIG control system and how to configure and extend it.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Overview` | [compliance/stig-modules.md#overview](../../compliance/stig-modules.md#overview) |
  | `## Architecture` | [compliance/stig-modules.md#architecture](../../compliance/stig-modules.md#architecture) |
  | `## How It Works` | [compliance/stig-modules.md#how-it-works](../../compliance/stig-modules.md#how-it-works) |
  | `## Configuration` | [compliance/stig-modules.md#configuration](../../compliance/stig-modules.md#configuration) |
  | `## Audit and Reporting` | [compliance/stig-modules.md#audit-and-reporting](../../compliance/stig-modules.md#audit-and-reporting) |
  | `## Adding New STIG Controls` | [compliance/stig-modules.md#adding-new-stig-controls](../../compliance/stig-modules.md#adding-new-stig-controls) |
  | `## Key Design Principles` | [compliance/stig-modules.md#key-design-principles](../../compliance/stig-modules.md#key-design-principles) |
  | `## Using Downstream` | [compliance/stig-modules.md#using-downstream](../../compliance/stig-modules.md#using-downstream) |
- Unmapped content: none

### `docs/operator/compliance-interchange.md`

- Title: CF-XCCDF Compliance Interchange Operator Guide
- Purpose: Operator guide for the implemented CF-XCCDF server/API interchange behavior.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Version and lineage semantics` | [compliance/cf-xccdf-interchange-operator-guide.md#version-and-lineage-semantics](../../compliance/cf-xccdf-interchange-operator-guide.md#version-and-lineage-semantics) |
  | `## Importing foreign STIG/XCCDF` | [compliance/cf-xccdf-interchange-operator-guide.md#importing-foreign-stigxccdf](../../compliance/cf-xccdf-interchange-operator-guide.md#importing-foreign-stigxccdf) |
  | `## Importing CF-XCCDF` | [compliance/cf-xccdf-interchange-operator-guide.md#importing-cf-xccdf](../../compliance/cf-xccdf-interchange-operator-guide.md#importing-cf-xccdf) |
  | `## Trust and publication` | [compliance/cf-xccdf-interchange-operator-guide.md#trust-and-publication](../../compliance/cf-xccdf-interchange-operator-guide.md#trust-and-publication) |
  | `## Assignments and overlays` | [compliance/assignments-and-report-only-enforcement.md#assignments-and-overlays](../../compliance/assignments-and-report-only-enforcement.md#assignments-and-overlays) |
  | `## XCCDF export` | [compliance/cf-xccdf-interchange-operator-guide.md#xccdf-export](../../compliance/cf-xccdf-interchange-operator-guide.md#xccdf-export) |
  | `## Policy JSON/TOML interchange` | [compliance/cf-xccdf-interchange-operator-guide.md#policy-jsontoml-interchange](../../compliance/cf-xccdf-interchange-operator-guide.md#policy-jsontoml-interchange) |
  | `## Compatibility and tested limits` | [compliance/cf-xccdf-interchange-operator-guide.md#compatibility-and-tested-limits](../../compliance/cf-xccdf-interchange-operator-guide.md#compatibility-and-tested-limits) |
- Unmapped content: none

### `docs/auth-session-security.md`

- Title: Auth Session Security Strategy
- Purpose: Describes server-authoritative browser sessions, cookies, lifetime, and CSRF protection.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Session Cookie` | [security/session-cookies-and-csrf.md#session-cookie](../../security/session-cookies-and-csrf.md#session-cookie) |
  | `## Session Lifecycle` | [security/session-cookies-and-csrf.md#session-lifecycle](../../security/session-cookies-and-csrf.md#session-lifecycle) |
  | `## CSRF Strategy` | [security/session-cookies-and-csrf.md#csrf-strategy](../../security/session-cookies-and-csrf.md#csrf-strategy) |
- Unmapped content: none

### `docs/auth-role-mapping.md`

- Title: OIDC Role Mapping Configuration
- Purpose: Describes mapping OIDC groups to local Admin, Operator, and Viewer roles.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Roles` | [security/oidc-role-mapping.md#roles](../../security/oidc-role-mapping.md#roles) |
  | `## Configuration` | [security/oidc-role-mapping.md#configuration](../../security/oidc-role-mapping.md#configuration) |
  | `## Role Selection Logic` | [security/oidc-role-mapping.md#what-happens-at-login](../../security/oidc-role-mapping.md#what-happens-at-login) |
  | `## Role Synchronization` | [security/oidc-role-mapping.md#role-synchronization](../../security/oidc-role-mapping.md#role-synchronization) |
  | `## Safe-Deny Behavior` | [security/oidc-role-mapping.md#what-happens-at-login](../../security/oidc-role-mapping.md#what-happens-at-login) |
  | `## Examples` | [security/oidc-role-mapping.md#examples](../../security/oidc-role-mapping.md#examples) |
  | `## Testing Role Mapping` | [security/oidc-role-mapping.md#testing-role-mapping](../../security/oidc-role-mapping.md#testing-role-mapping) |
  | `## Troubleshooting` | [security/oidc-role-mapping.md#troubleshooting](../../security/oidc-role-mapping.md#troubleshooting) |
- Unmapped content: none

### `docs/auth-provider-compatibility-validation.md`

- Title: TASK-65.7 Provider Compatibility and Security Validation
- Purpose: Records the OIDC provider claim-shape matrix, security regression coverage, and residual risks.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Validation Matrix` | [security/oidc-provider-compatibility-validation.md#validation-matrix](../../security/oidc-provider-compatibility-validation.md#validation-matrix) |
  | `## Security Regression Coverage` | [security/oidc-provider-compatibility-validation.md#security-regression-coverage](../../security/oidc-provider-compatibility-validation.md#security-regression-coverage) |
  | `## Residual Risks` | [security/oidc-provider-compatibility-validation.md#residual-risks](../../security/oidc-provider-compatibility-validation.md#residual-risks) |
- Unmapped content: none

### `docs/task-433-design-parity-review.md`

- Title: TASK-433 Design Parity Review
- Purpose: Records the TASK-433 design delta comparison and classification of product versus demo-only design mechanisms.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Evidence` | [historical/task-433-design-parity-review.md#evidence](../../historical/task-433-design-parity-review.md#evidence) |
  | `## Source and Contract Review` | [historical/task-433-design-parity-review.md#source-and-contract-review](../../historical/task-433-design-parity-review.md#source-and-contract-review) |
  | `## Design Delta Classification` | [historical/task-433-design-parity-review.md#design-delta-classification](../../historical/task-433-design-parity-review.md#design-delta-classification) |
- Unmapped content: none

### `docs/design/CrystalForge/docs/crystal-forge-compliance-design/README.md`

- Title: Crystal Forge Compliance architecture review, v0.1
- Purpose: Bundle contents, reading order, and status of the Compliance design review.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Contents` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Rendering and source links` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Status` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-compliance-design/compliance-view-design-v0.1.md`

- Title: Crystal Forge Compliance View
- Purpose: Review draft of the production /compliance route with gaps, decisions, and regression scenarios.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Architecture, data provenance, POA&M integration, and consistency contract` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 1. Purpose, evidence, and revision boundary` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 2. Existing design contracts and conflicts` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 3. Surface and component model` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 4. Identity and state model` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 5. UI-to-API contract inventory` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 6. Persistence and provenance map` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 7. Bundle versions, assignments, and requirement coverage` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 8. Evidence source selection and authority` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 9. Count units, scores, and misleading clean states` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 10. Navigation, request state, and refresh boundaries` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 11. Finding-origin POA&M creation and linking` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 12. Common POA&M detail, families, and entry-point consistency` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 13. Verification, closure, and reopening` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 14. Waivers, CVE acceptance, and assignment exceptions` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 15. Evidence export and report integrity` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 16. Import, bundle maintenance, and assignment side effects` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 17. Authorization, errors, loading, and unavailable states` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 18. Source-level design parity register` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 19. Performance and query behavior` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 20. Consolidated gap register` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 21. Proposed shared contracts` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 22. End-to-end workflow contracts for review` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 23. Verification strategy and regression matrix` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 24. Cross-view consistency ledger` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 25. Decision register` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## 26. Sources, verification limits, and artifact record` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-compliance-design/cross-view-contract-ledger-v0.1.md`

- Title: Crystal Forge: Cross-view Contract Ledger
- Purpose: Open worksheet of cross-view contracts for Systems, CVEs, and Compliance.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Contract review rows` | [ui/cross-view-contract-ledger.md#what-the-document-specifies](../../ui/cross-view-contract-ledger.md#what-the-document-specifies) |
  | `## Review order and disposition` | [ui/cross-view-contract-ledger.md#what-the-document-specifies](../../ui/cross-view-contract-ledger.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-compliance-design/verification.md`

- Title: Verification record
- Purpose: Verification record of the Compliance design review.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Source boundary` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Inspection performed` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Not performed` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Generated artifact checks` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Scope of changes` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
  | `## Review status` | [ui/compliance-view-design.md#what-the-document-specifies](../../ui/compliance-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-cves-design/README.md`

- Title: Crystal Forge CVEs View design review
- Purpose: Review order, bundle contents, and diagram table of the CVEs design review.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Main document` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## Review order` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## Bundle contents` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## Diagrams` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-cves-design/cves-view-design-v0.1.md`

- Title: Crystal Forge CVEs View
- Purpose: Review draft of the fleet /cves page, triage, and exact-CVE POA&M lifecycle.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Architecture, data provenance, and consistency contract` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 1. Purpose, evidence, and review boundary` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 2. Existing specifications and conflicts` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 3. Surface and component model` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 4. Terminology and independent state dimensions` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 5. Persistence and identity relationships` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 6. Data producers and source map` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 7. Inventory selection and authority` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 8. Count contract` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 9. HTTP contract and bounds` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 10. Page state, navigation, and refresh` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 11. Fleet drawer and navigation identity` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 12. Triage decisions and host-override precedence` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 13. Triage transaction, concurrency, and retry contract` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 14. Exact-CVE POA&M verification and lifecycle` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 15. Normative target: evidence continuity across deployments` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 16. Loading, error, empty, and stale-state contracts` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 17. Design-reference comparison` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 18. Performance and query behavior` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 19. Authorization, privacy, and export` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 20. Consolidated gap register` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 21. Proposed target contract` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 22. End-to-end workflow examples` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 23. Test coverage and regression matrix` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 24. Change boundaries and compatibility` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 25. Decision register` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## 26. Source index and verification record` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-cves-design/verification.md`

- Title: Verification record
- Purpose: Verification record of the CVEs design review.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Artifact checks performed` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## Application verification not performed` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
  | `## Repository safety` | [ui/cves-view-design.md#what-the-document-specifies](../../ui/cves-view-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/cve-poam-evidence-continuity-design-spec.md`

- Title: CVE POA&M and Risk-Disposition Continuity Across Revisions
- Purpose: Approved normative contract for CVE finding identity, evidence continuity, dispositions, POA&M, and the risk-acceptance register.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## 1. Purpose` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 2. Core design principles` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 3. Conceptual data model` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 4. Existing schema concepts to retain` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 5. Finding, evidence, disposition, and POA&M relationships` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 6. Current CVE authority` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 7. Explicit historical selections` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 8. POA&M baseline model` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 9. Database validation of a CVE POA&M baseline` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 10. Current evidence over time` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 11. Current finding resolution` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 12. POA&M continuity` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 13. System risk dispositions` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 14. Environment risk dispositions` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 15. Environment POA&M continuity` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 16. POA&M verification` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 17. POA&M closure` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 18. Environment POA&M closure` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 19. Environment scheduled-disposition coherence` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 20. Reconciliation` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 21. Configuration and commit relationship` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 22. Current and baseline evidence` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 23. UI requirements` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 24. Security and integrity invariants` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 25. Migration guidance` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 26. Implementation guidance` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 27. Behavior matrix` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 28. Required regressions` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 29. Acceptance criteria` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 30. Example end-to-end scenario` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 31. Summary` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
  | `## 32. Unified Risk-Acceptance Register Actions` | [poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies](../../poam/cve-poam-evidence-continuity-design.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md`

- Title: Crystal Forge XCCDF Interchange Profile
- Purpose: Design draft of the CF-XCCDF interchange profile.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## 1. Purpose` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 2. Normative language` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 3. Design principles` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 4. Scope` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 5. Terminology` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 6. Conformance classes` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 7. XML namespaces and checking systems` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 8. Portable artifact` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 9. Core XCCDF mapping` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 10. Benchmark representation` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 11. Policy representation` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 12. Current policy-type encodings` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 13. Policy phases and enforcement` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 14. Dependencies and non-global NixOS modules` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 15. Identifiers and framework mappings` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 16. Bundle assignment and tailoring` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 17. Round-trip requirements` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 18. Import behavior` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 19. Export behavior` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 20. Assessment result export` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 21. Trust and security` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 22. Compatibility promise` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 23. Validation and test suite` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 24. Required Crystal Forge data-model changes` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 25. Non-normative complete rule example` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 26. Decisions captured by this draft` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 27. Open decisions before version 0.2` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 28. Normative references` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
  | `## 29. Informative references` | [compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies](../../compliance/cf-xccdf-interchange-profile.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `backlog/docs/design/doc-12 - Compliance-implementation-roadmap.md`

- Title: Compliance implementation roadmap
- Purpose: Phased sequencing, overlap decisions, and readiness gates for compliance work.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Purpose` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
  | `## Recommended sequencing` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
  | `## Key overlap decisions` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
  | `## Readiness gates` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
  | `## Suggested milestone alignment` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
  | `## Related docs` | [compliance/compliance-implementation-roadmap.md#what-the-document-specifies](../../compliance/compliance-implementation-roadmap.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `backlog/docs/doc-20 - TASK-412-Slice-2-Implementation-Summary.md`

- Title: TASK-412 Slice 2 Implementation Summary
- Purpose: Task record of TASK-412 Slice 2 transactional trust and publication work.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## What Was Done` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Implementation Notes` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Commits` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Verification Status` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Known Caveats` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Next Steps for Reviewer` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## References` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `backlog/docs/doc-21 - TASK-412-Complete-Implementation-Slice-1-5-Verification-Summary.md`

- Title: TASK-412 Complete Implementation - Slices 1-5 Verification Summary
- Purpose: Task record and verification summary of TASK-412 Slices 1 to 5.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Work Completed` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Design Requirements Met` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Known Limitations & Future Work` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Commits & Push Timeline` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## Next Steps for Reviewers` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
  | `## References` | [historical/task-412-implementation-records.md#what-the-documents-record](../../historical/task-412-implementation-records.md#what-the-documents-record) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md`

- Title: Compliance UI Redesign Spec — design commit `23c88aba`
- Purpose: Visual and interaction specification for the production Compliance view.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## 1. Prerequisite — MR !315 must be merged first` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 2. Design mock → Crystal Forge domain mapping` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 3. Page layout (`ComplianceView.jsx:61-166`)` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 4. Bundle list table (`ComplianceView.jsx:184-283`)` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 5. Bundle detail drawer (`ComplianceView.jsx:285-394`)` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 6. CSS additions` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 7. Required backend changes` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 8. STIG import pause / resume (`ImportStigModal.jsx:195-260`, `ComplianceView.jsx:84-95`)` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 9. Reviewer verification endpoints` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 10. Required implementation sequence` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 11. Verification` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
  | `## 12. Out of scope` | [ui/compliance-ui-redesign-spec.md#what-the-document-specifies](../../ui/compliance-ui-redesign-spec.md#what-the-document-specifies) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `schemas/cf-xccdf-1/PROVENANCE.md`

- Title: CF-XCCDF v0.1 schema provenance
- Purpose: Provenance note of the CF-XCCDF v0.1 schema.
- Action: retained
- Sections:
  | (no H2 sections) | [references/schema-provenance.md](../../references/schema-provenance.md) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)

### `schemas/oscal-1.1.2/poam-provenance.txt`

- Title: OSCAL POA&M v1.1.2 schemas (unmodified NIST release assets)
- Purpose: Provenance and digests of the OSCAL POA&M v1.1.2 schemas.
- Action: retained
- Sections:
  | (no H2 sections) | [references/schema-provenance.md](../../references/schema-provenance.md) |
- Unmapped content: none (the retained file is the authoritative text; the pointer records scope, status, and evidence)
