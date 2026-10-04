---
type: Reference
title: "Migration manifest: builders, caches, and evaluation"
description: "Records where each builder, cache, and evaluation source document went during the OKF migration and the coverage result."
tags:
  - crystal-forge
  - migration
---

# Migration manifest: builders, caches, and evaluation

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/builder-security-architecture.md` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md), [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md), [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md), [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md), [security/builder-request-authentication-and-data-in-transit.md](../../security/builder-request-authentication-and-data-in-transit.md), [security/builder-threat-model.md](../../security/builder-threat-model.md), [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md), [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md) | split | complete |
| `docs/multi-builder-api.md` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md), [builders/verified-source-evaluator-contract.md](../../builders/verified-source-evaluator-contract.md), [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md), [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md), [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md), [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md), [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md), [api/builder-api-authentication-and-admin-endpoints.md](../../api/builder-api-authentication-and-admin-endpoints.md), [api/builder-job-lifecycle-api.md](../../api/builder-job-lifecycle-api.md), [api/builder-cve-scan-api.md](../../api/builder-cve-scan-api.md) | split | complete |
| `docs/nix-cli-invocations.md` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md) | moved | complete |
| `docs/s3-cache-notes.md` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md) | moved | complete |
| `docs/evaluation-flake-snapshots.md` | [evaluation/evaluation-flake-snapshot-architecture.md](../../evaluation/evaluation-flake-snapshot-architecture.md), [evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md](../../evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md), [evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md](../../evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md), [evaluation/evaluation-snapshot-retention-and-rollback.md](../../evaluation/evaluation-snapshot-retention-and-rollback.md), [cves/exact-cve-evidence-authority-and-inventory-reads.md](../../cves/exact-cve-evidence-authority-and-inventory-reads.md), [cves/exact-cve-writer-locking-and-fleet-triage.md](../../cves/exact-cve-writer-locking-and-fleet-triage.md), [evaluation/flake-outputs-and-count-authority.md](../../evaluation/flake-outputs-and-count-authority.md), [deployment/manual-deployment-queue-contract.md](../../deployment/manual-deployment-queue-contract.md), [api/evaluation-snapshot-api-and-url-state.md](../../api/evaluation-snapshot-api-and-url-state.md), [historical/task-440-config-explorer-design-audit.md](../../historical/task-440-config-explorer-design-audit.md), [testing/evaluation-snapshot-verification-expectations.md](../../testing/evaluation-snapshot-verification-expectations.md) | split | complete |
| `docs/config-explorer-architecture.md` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md), [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md), [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md), [evaluation/config-explorer-implementation-status.md](../../evaluation/config-explorer-implementation-status.md), [decisions/config-explorer-decisions.md](../../decisions/config-explorer-decisions.md) | split | complete |
| `docs/nixos-option-metadata.md` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md) | moved | complete |
| `backlog/docs/builders/remote-builder-architecture-status/doc-16 - Remote-Builder-Architecture-Status-and-Follow-up-Plan.md` | retained in place; see [pointer](../../builders/remote-builder-architecture-status-and-follow-up-plan.md) | retained | complete |
| `backlog/docs/build/build-invalidation-graph/doc-23 - Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md` | retained in place; see [pointer](../../operations/build-invalidation-graph-and-ci-latency-analysis.md) | retained | complete |
| `backlog/docs/doc-24 - Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md` | retained in place; see [pointer](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md) | retained | complete |

## Source inventory

### `docs/builder-security-architecture.md`

- Title: Crystal Forge Builder Security Architecture
- Purpose: Defines builder trust boundaries, network flows, signing protocol, threat model, firewall rules, failure phases, and key management for remote builders.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Crystal Forge Builder Security Architecture` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#crystal-forge-builder-security-architecture) |
  | `## 0. Recommended Default Strategy` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md#0-recommended-default-strategy) |
  | `## 1. Purpose and Scope` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#1-purpose-and-scope) |
  | `## 2. System Components and Trust Levels` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#2-system-components-and-trust-levels) |
  | `## 3. Component Definitions` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#3-component-definitions) |
  | `### 3.1 Crystal Forge Server` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#31-crystal-forge-server) |
  | `### 3.2 Crystal Forge Builder` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#32-crystal-forge-builder) |
  | `### 3.3 Crystal Forge Agent` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#33-crystal-forge-agent) |
  | `## 4. Network Flow Diagrams` | [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md#4-network-flow-diagrams) |
  | `### 4.1 Builder Job Lifecycle — Complete Network Picture` | [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md#41-builder-job-lifecycle--complete-network-picture) |
  | `### 4.2 ServerDerivation Strategy (No Source Access on Builder)` | [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md#42-serverderivation-strategy-no-source-access-on-builder) |
  | `### 4.3 SourceReEvaluateVerified + ServerBundledArchive Strategy` | [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md#43-sourcereevaluateverified--serverbundledarchive-strategy) |
  | `### 4.4 SourceReEvaluateVerified + LocalGitWorktree Strategy` | [builders/builder-network-flows-by-strategy.md](../../builders/builder-network-flows-by-strategy.md#44-sourcereevaluateverified--localgitworktree-strategy) |
  | `## 5. Authentication and Signing Protocol` | [security/builder-request-authentication-and-data-in-transit.md](../../security/builder-request-authentication-and-data-in-transit.md#5-authentication-and-signing-protocol) |
  | `### 5.1 Per-Request Ed25519 Signature` | [security/builder-request-authentication-and-data-in-transit.md](../../security/builder-request-authentication-and-data-in-transit.md#51-per-request-ed25519-signature) |
  | `### 5.2 What the Builder Private Key Controls` | [security/builder-request-authentication-and-data-in-transit.md](../../security/builder-request-authentication-and-data-in-transit.md#52-what-the-builder-private-key-controls) |
  | `## 6. Data in Transit — What Crosses the Wire` | [security/builder-request-authentication-and-data-in-transit.md](../../security/builder-request-authentication-and-data-in-transit.md#6-data-in-transit--what-crosses-the-wire) |
  | `## 7. Filesystem Layout on the Builder Host` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#7-filesystem-layout-on-the-builder-host) |
  | `## 8. Firewall Rules Required per Strategy` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#8-firewall-rules-required-per-strategy) |
  | `### 8.1 ServerDerivation (Recommended Default)` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#81-serverderivation-recommended-default) |
  | `### 8.2 SourceReEvaluateVerified + ServerBundledArchive` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#82-sourcereevaluateverified--serverbundledarchive) |
  | `### 8.3 SourceReEvaluateVerified + LocalGitWorktree` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#83-sourcereevaluateverified--localgitworktree) |
  | `### 8.4 CF Server Inbound Rules` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#84-cf-server-inbound-rules) |
  | `## 9. Threat Model — What Builders Can and Cannot Do` | [security/builder-threat-model.md](../../security/builder-threat-model.md#9-threat-model--what-builders-can-and-cannot-do) |
  | `### 9.1 Compromised Builder` | [security/builder-threat-model.md](../../security/builder-threat-model.md#91-compromised-builder) |
  | `### 9.2 Malicious Job Claim` | [security/builder-threat-model.md](../../security/builder-threat-model.md#92-malicious-job-claim) |
  | `### 9.3 Source Archive Tampering` | [security/builder-threat-model.md](../../security/builder-threat-model.md#93-source-archive-tampering) |
  | `### 9.4 Replay Attacks` | [security/builder-threat-model.md](../../security/builder-threat-model.md#94-replay-attacks) |
  | `## 10. Pre-Build Failure Phases` | [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md#10-pre-build-failure-phases) |
  | `## 11. Configuration Reference for Network-Constrained Environments` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#11-configuration-reference-for-network-constrained-environments) |
  | `### 11.1 Maximum Isolation (GovCloud / Air-Gap Adjacent)` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#111-maximum-isolation-govcloud--air-gap-adjacent) |
  | `### 11.2 Colocated / Internal Deployment (Relaxed)` | [operations/builder-network-and-filesystem-requirements.md](../../operations/builder-network-and-filesystem-requirements.md#112-colocated--internal-deployment-relaxed) |
  | `## 12. Key Management Lifecycle` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#12-key-management-lifecycle) |
  | `## 13. Logging and Auditability` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#13-logging-and-auditability) |
  | `## 14. Relationship to Other Crystal Forge Documentation` | [builders/builder-trust-boundaries-and-components.md](../../builders/builder-trust-boundaries-and-components.md#14-relationship-to-other-crystal-forge-documentation) |
- Unmapped content: none

### `docs/multi-builder-api.md`

- Title: Multi-Builder API Documentation
- Purpose: Describes the multi-builder architecture, execution strategies, database schema, builder and admin API reference, deployment, retry, security, and troubleshooting.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Multi-Builder API Documentation` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#multi-builder-api-documentation) |
  | `## Overview` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#overview) |
  | `## Remote Build Execution Strategies` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md#remote-build-execution-strategies) |
  | `### Recommended Default` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md#recommended-default) |
  | `### `server_derivation` — when to use it` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md#server_derivation--when-to-use-it) |
  | `### `server_derivation`` | [builders/remote-build-execution-strategies.md](../../builders/remote-build-execution-strategies.md#server_derivation) |
  | `### `source_re_evaluate_verified`` | [builders/verified-source-evaluator-contract.md](../../builders/verified-source-evaluator-contract.md#source_re_evaluate_verified) |
  | `## Architecture` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#architecture) |
  | `### Components` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#components) |
  | `### Key Features` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#key-features) |
  | `## Database Schema` | [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md#database-schema) |
  | `### `builders` Table` | [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md#builders-table) |
  | `### `builder_environment_assignments` Table` | [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md#builder_environment_assignments-table) |
  | `### `build_jobs` Table` | [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md#build_jobs-table) |
  | `### `builder_metrics` Table` | [data-model/builder-api-database-schema.md](../../data-model/builder-api-database-schema.md#builder_metrics-table) |
  | `## API Reference` | [api/builder-api-authentication-and-admin-endpoints.md](../../api/builder-api-authentication-and-admin-endpoints.md#api-reference) |
  | `### Authentication` | [api/builder-api-authentication-and-admin-endpoints.md](../../api/builder-api-authentication-and-admin-endpoints.md#authentication) |
  | `### Admin Endpoints` | [api/builder-api-authentication-and-admin-endpoints.md](../../api/builder-api-authentication-and-admin-endpoints.md#admin-endpoints) |
  | `### Builder Endpoints` | [api/builder-job-lifecycle-api.md](../../api/builder-job-lifecycle-api.md#builder-endpoints) |
  | `## Builder Deployment` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#builder-deployment) |
  | `### Prerequisites` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#prerequisites) |
  | `### Keypair Generation` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#keypair-generation) |
  | `### Configuration` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#configuration) |
  | `### Builder Polling Loop (Pseudocode)` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#builder-polling-loop-pseudocode) |
  | `## Environment Assignment` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#environment-assignment) |
  | `### Wildcard Builders` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#wildcard-builders) |
  | `### Environment-Specific Builders` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#environment-specific-builders) |
  | `## Retry Strategy` | [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md#retry-strategy) |
  | `### Priority Weighting` | [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md#priority-weighting) |
  | `### Max Retries` | [builders/builder-failure-phases-and-retry.md](../../builders/builder-failure-phases-and-retry.md#max-retries) |
  | `## Heartbeat and Offline Detection` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#heartbeat-and-offline-detection) |
  | `### Heartbeat Interval` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#heartbeat-interval) |
  | `### Offline Detection` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#offline-detection) |
  | `## Performance Considerations` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#performance-considerations) |
  | `### Indexes` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#indexes) |
  | `### Query Optimization` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#query-optimization) |
  | `### Metrics Retention` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#metrics-retention) |
  | `## Security` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#security) |
  | `### Authentication` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#authentication) |
  | `### Authorization` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#authorization) |
  | `### Network Boundaries` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#network-boundaries) |
  | `### Builder Credential Boundary` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#builder-credential-boundary) |
  | `### Key Management` | [security/builder-credential-boundary-key-management-and-audit-logging.md](../../security/builder-credential-boundary-key-management-and-audit-logging.md#key-management) |
  | `## Troubleshooting` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#troubleshooting) |
  | `### Builder Not Receiving Jobs` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#builder-not-receiving-jobs) |
  | `### Authentication Failures` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#authentication-failures) |
  | `### Jobs Not Retrying` | [operations/builder-deployment-and-troubleshooting.md](../../operations/builder-deployment-and-troubleshooting.md#jobs-not-retrying) |
  | `## Migration from Direct Database Access` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#migration-from-direct-database-access) |
  | `### Gradual Rollout` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#gradual-rollout) |
  | `### Backward Compatibility` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#backward-compatibility) |
  | `## Future Enhancements` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#future-enhancements) |
  | `## References` | [builders/builder-architecture-and-job-scheduling.md](../../builders/builder-architecture-and-job-scheduling.md#references) |
- Unmapped content: none

### `docs/nix-cli-invocations.md`

- Title: Nix CLI Invocations by Service
- Purpose: Lists every Nix CLI command that each Crystal Forge service runs.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Nix CLI Invocations by Service` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#nix-cli-invocations-by-service) |
  | `## Server (`crystal-forge server`)` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#server-crystal-forge-server) |
  | `### `handlers/api/builders.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#handlersapibuildersrs) |
  | `### `flake/eval.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#flakeevalrs) |
  | `### `flake/commits.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#flakecommitsrs) |
  | `### `models/evaluate_with_policies.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#modelsevaluate_with_policiesrs) |
  | `### `derivations/eval.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#derivationsevalrs) |
  | `### `derivations/build.rs` (shared with builder)` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#derivationsbuildrs-shared-with-builder) |
  | `### `derivations/utils.rs` (shared with builder)` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#derivationsutilsrs-shared-with-builder) |
  | `### `builder/worker.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#builderworkerrs) |
  | `## Builder (`crystal-forge builder --api`)` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#builder-crystal-forge-builder---api) |
  | `### `src/bin/builder.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#srcbinbuilderrs) |
  | `### `builder/cve_scanner.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#buildercve_scannerrs) |
  | `### `builder/api_client.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#builderapi_clientrs) |
  | `## Agent (`crystal-forge agent`)` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#agent-crystal-forge-agent) |
  | `### `src/bin/agent.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#srcbinagentrs) |
  | `### `deployment/agent.rs`` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#deploymentagentrs) |
  | `## Totals` | [references/nix-cli-invocations-by-service.md](../../references/nix-cli-invocations-by-service.md#totals) |
- Unmapped content: none

### `docs/s3-cache-notes.md`

- Title: Crystal Forge — S3 Cache (MinIO) Quickstart
- Purpose: Shows how to push to and read from a MinIO-backed S3 Nix binary cache.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Crystal Forge — S3 Cache (MinIO) Quickstart` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#crystal-forge--s3-cache-minio-quickstart) |
  | `## Requirements` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#requirements) |
  | `## Why this setup` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#why-this-setup) |
  | `## Environment (recommended)` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#environment-recommended) |
  | `## Push: two working forms` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#push-two-working-forms) |
  | `### A) All-in-URL (portable)` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#a-all-in-url-portable) |
  | `### B) Via env + short URL` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#b-via-env--short-url) |
  | `## Use as a substituter (reads)` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#use-as-a-substituter-reads) |
  | `## Crystal Forge (NixOS) snippet` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#crystal-forge-nixos-snippet) |
  | `## Troubleshooting` | [caches/s3-minio-cache-quickstart.md](../../caches/s3-minio-cache-quickstart.md#troubleshooting) |
- Unmapped content: none

### `docs/evaluation-flake-snapshots.md`

- Title: Evaluation and Flake Snapshot Architecture
- Purpose: Defines ownership, identity, lifecycle, persistence, retention, redaction, flake output counts, deployment queue, API, and verification contracts for evaluation and flake snapshots.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Evaluation and Flake Snapshot Architecture` | [evaluation/evaluation-flake-snapshot-architecture.md](../../evaluation/evaluation-flake-snapshot-architecture.md#evaluation-and-flake-snapshot-architecture) |
  | `## Purpose` | [evaluation/evaluation-flake-snapshot-architecture.md](../../evaluation/evaluation-flake-snapshot-architecture.md#purpose) |
  | `## Ownership and Data Flow` | [evaluation/evaluation-flake-snapshot-architecture.md](../../evaluation/evaluation-flake-snapshot-architecture.md#ownership-and-data-flow) |
  | `## Identity and Comparison` | [evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md](../../evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md#identity-and-comparison) |
  | `## Lifecycle` | [evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md](../../evaluation/evaluation-snapshot-identity-lifecycle-and-comparison.md#lifecycle) |
  | `## Persistence, Bounds, and Reclamation` | [evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md](../../evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md#persistence-bounds-and-reclamation) |
  | `## Retention` | [evaluation/evaluation-snapshot-retention-and-rollback.md](../../evaluation/evaluation-snapshot-retention-and-rollback.md#retention) |
  | `## Safe-Value and Redaction Policy` | [evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md](../../evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md#safe-value-and-redaction-policy) |
  | `## Flake Outputs and Count Authority` | [evaluation/flake-outputs-and-count-authority.md](../../evaluation/flake-outputs-and-count-authority.md#flake-outputs-and-count-authority) |
  | `## Deployment Queue Contract` | [deployment/manual-deployment-queue-contract.md](../../deployment/manual-deployment-queue-contract.md#deployment-queue-contract) |
  | `## API and URL State` | [api/evaluation-snapshot-api-and-url-state.md](../../api/evaluation-snapshot-api-and-url-state.md#api-and-url-state) |
  | `## TASK-440 Design Audit` | [historical/task-440-config-explorer-design-audit.md](../../historical/task-440-config-explorer-design-audit.md#task-440-design-audit) |
  | `## Verification Expectations` | [testing/evaluation-snapshot-verification-expectations.md](../../testing/evaluation-snapshot-verification-expectations.md#verification-expectations) |
- Unmapped content: none

### `docs/config-explorer-architecture.md`

- Title: Config Explorer Architecture
- Purpose: Specifies the Config Explorer design, its implementation map, and its decision record.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Config Explorer Architecture` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#config-explorer-architecture) |
  | `## Problem statement` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#problem-statement) |
  | `## Architecture invariants` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#architecture-invariants) |
  | `### Primary evaluator` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#primary-evaluator) |
  | `### Policy evaluator` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#policy-evaluator) |
  | `### Config Explorer` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#config-explorer) |
  | `## High-level Config path` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#high-level-config-path) |
  | `## Inspection phases` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#inspection-phases) |
  | `### Phase 0: exact target resolution` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#phase-0-exact-target-resolution) |
  | `### Phase 1: shallow root/bootstrap` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#phase-1-shallow-rootbootstrap) |
  | `### Phase 2: scoped prefix expansion` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#phase-2-scoped-prefix-expansion) |
  | `### Phase 3: exact option detail` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#phase-3-exact-option-detail) |
  | `### Phase 4: detailed provenance` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#phase-4-detailed-provenance) |
  | `## Configured options` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#configured-options) |
  | `### Benchmark record` | [evaluation/config-explorer-architecture.md](../../evaluation/config-explorer-architecture.md#benchmark-record) |
  | `## Target identity and cache contract` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#target-identity-and-cache-contract) |
  | `### Upgraded-fleet current revision recovery` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#upgraded-fleet-current-revision-recovery) |
  | `## Explorer observations and certified V2 snapshots` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#explorer-observations-and-certified-v2-snapshots) |
  | `### Explorer observations` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#explorer-observations) |
  | `### Certified V2 Config snapshot` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#certified-v2-config-snapshot) |
  | `## Changed, Drift, and search semantics` | [evaluation/config-explorer-target-identity-and-snapshot-semantics.md](../../evaluation/config-explorer-target-identity-and-snapshot-semantics.md#changed-drift-and-search-semantics) |
  | `## Failure containment` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#failure-containment) |
  | `## Resource scheduling` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#resource-scheduling) |
  | `## Lifecycle and progress` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#lifecycle-and-progress) |
  | `## Security model` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#security-model) |
  | `## Process and timeout model` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#process-and-timeout-model) |
  | `## Optional complete inventory` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#optional-complete-inventory) |
  | `## API principles` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#api-principles) |
  | `## Performance expectations` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#performance-expectations) |
  | `## Data flow` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#data-flow) |
  | `### Interactive Explorer` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#interactive-explorer) |
  | `### Authoritative separation` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#authoritative-separation) |
  | `## Non-goals` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#non-goals) |
  | `## Future evolution` | [evaluation/config-explorer-resource-security-and-api-model.md](../../evaluation/config-explorer-resource-security-and-api-model.md#future-evolution) |
  | `## Current implementation` | [evaluation/config-explorer-implementation-status.md](../../evaluation/config-explorer-implementation-status.md#current-implementation) |
  | `## Decision record` | [decisions/config-explorer-decisions.md](../../decisions/config-explorer-decisions.md#decision-record) |
- Unmapped content: none

### `docs/nixos-option-metadata.md`

- Title: NixOS Option Metadata Authority
- Purpose: States that packaged NixOS option metadata is an authoring aid and not authority for a target flake.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# NixOS Option Metadata Authority` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#nixos-option-metadata-authority) |
  | `## Architectural invariant` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#architectural-invariant) |
  | `## Source of the packaged catalog` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#source-of-the-packaged-catalog) |
  | `## Policy-authoring intelligence` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#policy-authoring-intelligence) |
  | `## Foreign flakes can have different schemas` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#foreign-flakes-can-have-different-schemas) |
  | `## Authority hierarchy` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#authority-hierarchy) |
  | `## Unknown/custom fallback` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#unknowncustom-fallback) |
  | `## Phase 3 and Phase 4 separation` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#phase-3-and-phase-4-separation) |
  | `## Future target-specific metadata` | [evaluation/nixos-option-metadata-authority.md](../../evaluation/nixos-option-metadata-authority.md#future-target-specific-metadata) |
- Unmapped content: none

### `backlog/docs/builders/remote-builder-architecture-status/doc-16 - Remote-Builder-Architecture-Status-and-Follow-up-Plan.md`

- Title: Remote Builder Architecture Status and Follow-up Plan
- Purpose: Retained Backlog document that holds authoritative design text; the pointer concept records its scope, status, and code evidence.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Current status after TASK-375` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Problem discovered during TASK-375 validation` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Recommended direction` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Strategy overview` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Follow-up tasks` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### TASK-375.3: pull-based store transport for `server_derivation`` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### TASK-375.4: verified source re-evaluation strategy` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Strategy comparison` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Recommended attempt phases` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Design principles going forward` | [builders/remote-builder-architecture-status-and-follow-up-plan.md](../../builders/remote-builder-architecture-status-and-follow-up-plan.md#what-the-document-specifies) (summarized; text stays in the retained file) |
- Unmapped content: none (the file is not moved)

### `backlog/docs/build/build-invalidation-graph/doc-23 - Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md`

- Title: Build Invalidation Graph and CI Feedback Latency Analysis
- Purpose: Retained Backlog document that holds authoritative design text; the pointer concept records its scope, status, and code evidence.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Purpose` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Problem statement` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Verified current state` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### The server derivation consumes the unfiltered workspace` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### The server derivation depends on the web UI derivation` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Internal consumers depend on aggregate packages` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Rust dependencies are rebuilt with application source` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### The devshell has no compiler cache` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### CI has no change-based gating and no cancellation` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Reporting jobs sit in the fast feedback path` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Target architecture` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Verification-level policy` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Implementation order and rationale` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## Constraints that MUST hold` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## References` | [operations/build-invalidation-graph-and-ci-latency-analysis.md](../../operations/build-invalidation-graph-and-ci-latency-analysis.md#what-the-document-specifies) (summarized; text stays in the retained file) |
- Unmapped content: none (the file is not moved)

### `backlog/docs/doc-24 - Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md`

- Title: Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture
- Purpose: Retained Backlog document that holds authoritative design text; the pointer concept records its scope, status, and code evidence.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## 1. Problem statement` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 2. User intent / product behavior` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 3. Core architectural principle: preserve the narrow fast evaluator` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Fast evaluator responsibilities` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Fast evaluator non-responsibilities` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 4. Three-tier evaluation model` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Tier 1 — Fast identity evaluation` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Tier 2 — Deep Config inspection` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Tier 3 — Evidence/policy evaluation` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 5. Parallel pipeline` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 6. Drift detection must stay fast` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Performance objective` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 7. ConfigArtifactV2 becomes canonical configuration evidence` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Candidate policy mappings` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Important distinction` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 8. Progressive Config evidence` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Stage A — policy-ready configuration facts` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Stage B — rich inspection/audit enrichment` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 9. Build admission is separate from deployment authorization` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Default behavior for already-running builds` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 10. Build admission modes` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Mode A — Build all` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Mode B — CF-enabled` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Mode C — Policy-gated` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 11. Progressive build-admission state machine` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 12. Build prioritization` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 13. PolicyAssessment becomes the decision source of truth` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 14. Policy phases` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Pre-build` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Build-priority` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Pre-deploy` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 15. Flake Explorer relationship` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Config artifact` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Flake artifact` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 16. Isolation requirements` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 17. Performance requirements / SLOs` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Required performance goals` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 18. Example timing` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 19. Failure semantics` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Fast evaluation failure` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### CF agent disabled` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Deep Config inspection unavailable` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Policy failure arrives after build starts` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Rich provenance Stage-B failure` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 20. Security and integrity requirements` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 21. Migration strategy after TASK-440` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Follow-up Task 1 — Canonical evaluation evidence architecture` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Follow-up Task 2 — Move config-derived policies to Config artifacts` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Follow-up Task 3 — Progressive build admission and scheduler priority` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Follow-up Task 4 — Reduce primary evaluator` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `### Follow-up Task 5 — Unify artifact/job infrastructure` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 22. Acceptance criteria for the consolidation initiative` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 23. Non-goals` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 24. Design invariants to carry into future tasks` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
  | `## 25. Short version` | [evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md](../../evaluation/evaluation-evidence-build-admission-and-deployment-gating-architecture.md#what-the-document-specifies) (summarized; text stays in the retained file) |
- Unmapped content: none (the file is not moved)

