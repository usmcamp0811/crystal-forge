---
type: Reference
title: "Migration manifest: deployment and agent"
description: Maps the agent heartbeat and history document, deployment design proposal, deployment policy documents, database view documents, and developer notes to their OKF concept destinations.
tags:
  - crystal-forge
  - migration
---

# Migration manifest: deployment and agent

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/agent-heartbeat-state-history.md` | [deployment/agent-heartbeat-vs-state-persistence.md](../../deployment/agent-heartbeat-vs-state-persistence.md), [data-model/system-events-timeline.md](../../data-model/system-events-timeline.md), [api/agent-post-types-and-deployment-response.md](../../api/agent-post-types-and-deployment-response.md), [concepts/restart-and-activation-classification.md](../../concepts/restart-and-activation-classification.md), [ui/deployment-history-rendering-rules.md](../../ui/deployment-history-rendering-rules.md), [operations/deployment-history-debugging-and-tests.md](../../operations/deployment-history-debugging-and-tests.md) | split | complete |
| `docs/deployments_design_doc.md` | [historical/agent-deployment-design-proposal.md](../../historical/agent-deployment-design-proposal.md) | moved | complete |
| `docs/deployment-policies.md` | [deployment/deployment-policies.md](../../deployment/deployment-policies.md), [deployment/built-in-policy-types.md](../../deployment/built-in-policy-types.md), [deployment/advanced-policy-types.md](../../deployment/advanced-policy-types.md), [deployment/policy-use-cases-and-best-practices.md](../../deployment/policy-use-cases-and-best-practices.md) | split | complete |
| `docs/deployment-policy-checks.md` | [deployment/deployment-policy-checks.md](../../deployment/deployment-policy-checks.md), [deployment/composite-policy-enforcement.md](../../deployment/composite-policy-enforcement.md) | split | complete |
| `docs/heartbeat-queries.md` | [data-model/database-relationships-and-views-guide.md](../../data-model/database-relationships-and-views-guide.md) | moved | complete |
| `docs/dev-notes.md` | [data-model/systems-status-view-technical-notes.md](../../data-model/systems-status-view-technical-notes.md), [data-model/commit-deployment-timeline-developer-notes.md](../../data-model/commit-deployment-timeline-developer-notes.md), [concepts/legacy-system-status-determination.md](../../concepts/legacy-system-status-determination.md), [evaluation/systemd-run-evaluation-isolation.md](../../evaluation/systemd-run-evaluation-isolation.md) | split | complete |
| `docs/views/view_buildable_derivations.md` | [data-model/views/view-buildable-derivations.md](../../data-model/views/view-buildable-derivations.md) | moved | complete |
| `docs/views/view_build_queue_status.md` | [data-model/views/view-build-queue-status.md](../../data-model/views/view-build-queue-status.md) | moved | complete |
| `docs/views/view_commit_build_status.md` | [data-model/views/view-commit-build-status.md](../../data-model/views/view-commit-build-status.md) | moved | complete |
| `docs/views/view_commit_deployment_timeline.md` | [data-model/views/view-commit-deployment-timeline.md](../../data-model/views/view-commit-deployment-timeline.md) | moved | complete |
| `docs/views/view_commit_nixos_table.md` | [data-model/views/view-commit-nixos-table.md](../../data-model/views/view-commit-nixos-table.md) | moved | complete |
| `docs/views/view_config_timeline.md` | [data-model/views/view-config-timeline.md](../../data-model/views/view-config-timeline.md) | moved | complete |
| `docs/views/view_derivation_status_breakdown.md` | [data-model/views/view-derivation-status-breakdown.md](../../data-model/views/view-derivation-status-breakdown.md) | moved | complete |
| `docs/views/view_flake_recent_commits.md` | [data-model/views/view-flake-recent-commits.md](../../data-model/views/view-flake-recent-commits.md) | moved | complete |
| `docs/views/view_nixos_derivation_build_queue.md` | [data-model/views/view-nixos-derivation-build-queue.md](../../data-model/views/view-nixos-derivation-build-queue.md) | moved | complete |
| `docs/views/view_system_deployment_status.md` | [data-model/views/view-system-deployment-status.md](../../data-model/views/view-system-deployment-status.md) | moved | complete |
| `docs/views/view_system_heartbeat_status.md` | [data-model/views/view-system-heartbeat-status.md](../../data-model/views/view-system-heartbeat-status.md) | moved | complete |

## Source inventory

### `docs/agent-heartbeat-state-history.md`

- Title: Agent heartbeat, state, deployment, and history logic
- Purpose: Describes agent state reporting, heartbeat versus full-state persistence, the system_events timeline, and history classification.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [deployment/agent-heartbeat-vs-state-persistence.md](../../deployment/agent-heartbeat-vs-state-persistence.md) |
  | `## Source files` | [../../deployment/agent-heartbeat-vs-state-persistence.md#source-files](../../deployment/agent-heartbeat-vs-state-persistence.md#source-files) |
  | `## High-level flow` | [../../deployment/agent-heartbeat-vs-state-persistence.md#high-level-flow](../../deployment/agent-heartbeat-vs-state-persistence.md#high-level-flow) |
  | `## Authoritative `system_events` timeline` | [../../data-model/system-events-timeline.md#authoritative-system_events-timeline](../../data-model/system-events-timeline.md#authoritative-system_events-timeline) |
  | `### Pending deployment context` | [data-model/system-events-timeline.md#pending-deployment-context](../../data-model/system-events-timeline.md#pending-deployment-context) |
  | `## Agent POST types` | [../../api/agent-post-types-and-deployment-response.md#agent-post-types](../../api/agent-post-types-and-deployment-response.md#agent-post-types) |
  | `## Server heartbeat-vs-state decision` | [../../deployment/agent-heartbeat-vs-state-persistence.md#server-heartbeat-vs-state-decision](../../deployment/agent-heartbeat-vs-state-persistence.md#server-heartbeat-vs-state-decision) |
  | `### Equivalence check` | [deployment/agent-heartbeat-vs-state-persistence.md#equivalence-check](../../deployment/agent-heartbeat-vs-state-persistence.md#equivalence-check) |
  | `## Deployment command response` | [../../api/agent-post-types-and-deployment-response.md#deployment-command-response](../../api/agent-post-types-and-deployment-response.md#deployment-command-response) |
  | `### Current limitation: detached CF deployment attribution` | [api/agent-post-types-and-deployment-response.md#current-limitation-detached-cf-deployment-attribution](../../api/agent-post-types-and-deployment-response.md#current-limitation-detached-cf-deployment-attribution) |
  | `## Restart and activation classification` | [../../concepts/restart-and-activation-classification.md#restart-and-activation-classification](../../concepts/restart-and-activation-classification.md#restart-and-activation-classification) |
  | `### Why startup can be a local rebuild` | [concepts/restart-and-activation-classification.md#why-startup-can-be-a-local-rebuild](../../concepts/restart-and-activation-classification.md#why-startup-can-be-a-local-rebuild) |
  | `### Why unchanged periodic rows are not local rebuilds` | [concepts/restart-and-activation-classification.md#why-unchanged-periodic-rows-are-not-local-rebuilds](../../concepts/restart-and-activation-classification.md#why-unchanged-periodic-rows-are-not-local-rebuilds) |
  | `## Web UI history rendering rules` | [../../ui/deployment-history-rendering-rules.md#web-ui-history-rendering-rules](../../ui/deployment-history-rendering-rules.md#web-ui-history-rendering-rules) |
  | `### Current limitation: failed deployment history` | [ui/deployment-history-rendering-rules.md#current-limitation-failed-deployment-history](../../ui/deployment-history-rendering-rules.md#current-limitation-failed-deployment-history) |
  | `## Correct tests to keep` | [../../operations/deployment-history-debugging-and-tests.md#correct-tests-to-keep](../../operations/deployment-history-debugging-and-tests.md#correct-tests-to-keep) |
  | `## Debug checklist` | [../../operations/deployment-history-debugging-and-tests.md#debug-checklist](../../operations/deployment-history-debugging-and-tests.md#debug-checklist) |
  | `## Known pitfalls` | [../../operations/deployment-history-debugging-and-tests.md#known-pitfalls](../../operations/deployment-history-debugging-and-tests.md#known-pitfalls) |
- Unmapped content: none

### `docs/deployments_design_doc.md`

- Title: Crystal Forge Agent Deployment Design Document
- Purpose: Historical proposal for server-to-agent deployment commands and the manual, auto_latest, and pinned rollout modes.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [historical/agent-deployment-design-proposal.md](../../historical/agent-deployment-design-proposal.md) |
  | `## Overview` | [../../historical/agent-deployment-design-proposal.md#overview](../../historical/agent-deployment-design-proposal.md#overview) |
  | `## Current State` | [../../historical/agent-deployment-design-proposal.md#current-state](../../historical/agent-deployment-design-proposal.md#current-state) |
  | `## Proposed Architecture` | [../../historical/agent-deployment-design-proposal.md#proposed-architecture](../../historical/agent-deployment-design-proposal.md#proposed-architecture) |
  | `## Risks & Mitigations` | [../../historical/agent-deployment-design-proposal.md#risks--mitigations](../../historical/agent-deployment-design-proposal.md#risks--mitigations) |
  | `## Success Criteria` | [../../historical/agent-deployment-design-proposal.md#success-criteria](../../historical/agent-deployment-design-proposal.md#success-criteria) |
- Unmapped content: none

### `docs/deployment-policies.md`

- Title: Deployment Policies
- Purpose: Defines deployment policy architecture, enforcement semantics, built-in and advanced policy types, assignment, and evaluation flow.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [deployment/deployment-policies.md](../../deployment/deployment-policies.md) |
  | `## Policy Architecture` | [../../deployment/deployment-policies.md#policy-architecture](../../deployment/deployment-policies.md#policy-architecture) |
  | `## Deployment-Manager Enforcement Semantics` | [../../deployment/deployment-policies.md#deployment-manager-enforcement-semantics](../../deployment/deployment-policies.md#deployment-manager-enforcement-semantics) |
  | `### Advanced Policy Behavior in Deployment Manager` | [deployment/deployment-policies.md#advanced-policy-behavior-in-deployment-manager](../../deployment/deployment-policies.md#advanced-policy-behavior-in-deployment-manager) |
  | `## Built-in Policy Types` | [../../deployment/built-in-policy-types.md#built-in-policy-types](../../deployment/built-in-policy-types.md#built-in-policy-types) |
  | `### require_cf_agent (Core)` | [deployment/built-in-policy-types.md#require_cf_agent-core](../../deployment/built-in-policy-types.md#require_cf_agent-core) |
  | `### require_packages` | [deployment/built-in-policy-types.md#require_packages](../../deployment/built-in-policy-types.md#require_packages) |
  | `### custom_check` | [deployment/built-in-policy-types.md#custom_check](../../deployment/built-in-policy-types.md#custom_check) |
  | `## Advanced Policy Types` | [../../deployment/advanced-policy-types.md#advanced-policy-types](../../deployment/advanced-policy-types.md#advanced-policy-types) |
  | `### time_window` | [deployment/advanced-policy-types.md#time_window](../../deployment/advanced-policy-types.md#time_window) |
  | `### require_approvals` | [deployment/advanced-policy-types.md#require_approvals](../../deployment/advanced-policy-types.md#require_approvals) |
  | `### canary_rollout` | [deployment/advanced-policy-types.md#canary_rollout](../../deployment/advanced-policy-types.md#canary_rollout) |
  | `### cve_threshold` | [deployment/advanced-policy-types.md#cve_threshold](../../deployment/advanced-policy-types.md#cve_threshold) |
  | `## Policy Assignment` | [../../deployment/deployment-policies.md#policy-assignment](../../deployment/deployment-policies.md#policy-assignment) |
  | `### Environment Baseline (Mandatory)` | [deployment/deployment-policies.md#environment-baseline-mandatory](../../deployment/deployment-policies.md#environment-baseline-mandatory) |
  | `### System-Specific (Optional)` | [deployment/deployment-policies.md#system-specific-optional](../../deployment/deployment-policies.md#system-specific-optional) |
  | `## Policy Evaluation Flow` | [../../deployment/deployment-policies.md#policy-evaluation-flow](../../deployment/deployment-policies.md#policy-evaluation-flow) |
  | `### Build-Time (Nix-Evaluated)` | [deployment/deployment-policies.md#build-time-nix-evaluated](../../deployment/deployment-policies.md#build-time-nix-evaluated) |
  | `### Deployment-Time` | [deployment/deployment-policies.md#deployment-time](../../deployment/deployment-policies.md#deployment-time) |
  | `## Example Use Cases` | [../../deployment/policy-use-cases-and-best-practices.md#example-use-cases](../../deployment/policy-use-cases-and-best-practices.md#example-use-cases) |
  | `### Production Safety Gate` | [deployment/policy-use-cases-and-best-practices.md#production-safety-gate](../../deployment/policy-use-cases-and-best-practices.md#production-safety-gate) |
  | `### Change Window Enforcement` | [deployment/policy-use-cases-and-best-practices.md#change-window-enforcement](../../deployment/policy-use-cases-and-best-practices.md#change-window-enforcement) |
  | `### Gradual Rollout with Safety Checks` | [deployment/policy-use-cases-and-best-practices.md#gradual-rollout-with-safety-checks](../../deployment/policy-use-cases-and-best-practices.md#gradual-rollout-with-safety-checks) |
  | `### Zero-Tolerance CVE Policy` | [deployment/policy-use-cases-and-best-practices.md#zero-tolerance-cve-policy](../../deployment/policy-use-cases-and-best-practices.md#zero-tolerance-cve-policy) |
  | `## Configuration Best Practices` | [../../deployment/policy-use-cases-and-best-practices.md#configuration-best-practices](../../deployment/policy-use-cases-and-best-practices.md#configuration-best-practices) |
  | `## Future Enhancements` | [../../deployment/policy-use-cases-and-best-practices.md#future-enhancements](../../deployment/policy-use-cases-and-best-practices.md#future-enhancements) |
- Unmapped content: none

### `docs/deployment-policy-checks.md`

- Title: Deployment Policy Checks
- Purpose: Defines the check policy types, composite enforcement, custom_check, require_cve_check, and seeded CVE policies.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [deployment/deployment-policy-checks.md](../../deployment/deployment-policy-checks.md) |
  | `## Supported policy types` | [../../deployment/deployment-policy-checks.md#supported-policy-types](../../deployment/deployment-policy-checks.md#supported-policy-types) |
  | `## Composite enforcement` | [../../deployment/composite-policy-enforcement.md#composite-enforcement](../../deployment/composite-policy-enforcement.md#composite-enforcement) |
  | `## `custom_check`` | [../../deployment/deployment-policy-checks.md#custom_check](../../deployment/deployment-policy-checks.md#custom_check) |
  | `### 1) Legacy single-expression shape (backward compatible)` | [deployment/deployment-policy-checks.md#1-legacy-single-expression-shape-backward-compatible](../../deployment/deployment-policy-checks.md#1-legacy-single-expression-shape-backward-compatible) |
  | `### 2) Multi-rule shape (`rules[]` + `mode`)` | [deployment/deployment-policy-checks.md#2-multi-rule-shape-rules--mode](../../deployment/deployment-policy-checks.md#2-multi-rule-shape-rules--mode) |
  | `## `require_cve_check`` | [../../deployment/deployment-policy-checks.md#require_cve_check](../../deployment/deployment-policy-checks.md#require_cve_check) |
  | `### Applicability` | [deployment/deployment-policy-checks.md#applicability](../../deployment/deployment-policy-checks.md#applicability) |
  | `## Seeded canonical CVE policies` | [../../deployment/deployment-policy-checks.md#seeded-canonical-cve-policies](../../deployment/deployment-policy-checks.md#seeded-canonical-cve-policies) |
- Unmapped content: none

### `docs/heartbeat-queries.md`

- Title: Database Relationships and Views Guide
- Purpose: Gives generic SQL patterns and design principles for views over related tables.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/database-relationships-and-views-guide.md](../../data-model/database-relationships-and-views-guide.md) |
  | `## Understanding Multi-Table Relationships` | [../../data-model/database-relationships-and-views-guide.md#understanding-multi-table-relationships](../../data-model/database-relationships-and-views-guide.md#understanding-multi-table-relationships) |
  | `## Core Relationship Patterns` | [../../data-model/database-relationships-and-views-guide.md#core-relationship-patterns](../../data-model/database-relationships-and-views-guide.md#core-relationship-patterns) |
  | `### 1. One-to-Many with Latest Record Pattern` | [data-model/database-relationships-and-views-guide.md#1-one-to-many-with-latest-record-pattern](../../data-model/database-relationships-and-views-guide.md#1-one-to-many-with-latest-record-pattern) |
  | `### 2. Multiple Activity Streams Pattern` | [data-model/database-relationships-and-views-guide.md#2-multiple-activity-streams-pattern](../../data-model/database-relationships-and-views-guide.md#2-multiple-activity-streams-pattern) |
  | `### 3. Status Derivation from Multiple Sources` | [data-model/database-relationships-and-views-guide.md#3-status-derivation-from-multiple-sources](../../data-model/database-relationships-and-views-guide.md#3-status-derivation-from-multiple-sources) |
  | `## Advanced Patterns` | [../../data-model/database-relationships-and-views-guide.md#advanced-patterns](../../data-model/database-relationships-and-views-guide.md#advanced-patterns) |
  | `### 4. Hierarchical Status Rollup` | [data-model/database-relationships-and-views-guide.md#4-hierarchical-status-rollup](../../data-model/database-relationships-and-views-guide.md#4-hierarchical-status-rollup) |
  | `### 5. Time-Based Status Windows` | [data-model/database-relationships-and-views-guide.md#5-time-based-status-windows](../../data-model/database-relationships-and-views-guide.md#5-time-based-status-windows) |
  | `## View Design Principles` | [../../data-model/database-relationships-and-views-guide.md#view-design-principles](../../data-model/database-relationships-and-views-guide.md#view-design-principles) |
  | `### Separation of Concerns` | [data-model/database-relationships-and-views-guide.md#separation-of-concerns](../../data-model/database-relationships-and-views-guide.md#separation-of-concerns) |
  | `### Performance Considerations` | [data-model/database-relationships-and-views-guide.md#performance-considerations](../../data-model/database-relationships-and-views-guide.md#performance-considerations) |
  | `### Maintainability` | [data-model/database-relationships-and-views-guide.md#maintainability](../../data-model/database-relationships-and-views-guide.md#maintainability) |
  | `## Common Pitfalls` | [../../data-model/database-relationships-and-views-guide.md#common-pitfalls](../../data-model/database-relationships-and-views-guide.md#common-pitfalls) |
  | `## Testing Relationship Views` | [../../data-model/database-relationships-and-views-guide.md#testing-relationship-views](../../data-model/database-relationships-and-views-guide.md#testing-relationship-views) |
- Unmapped content: none

### `docs/dev-notes.md`

- Title: Systems Status View - Technical Notes
- Purpose: Collects developer notes for the systems status view, the deployment timeline view, the legacy system status logic, and systemd-run evaluation isolation.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/systems-status-view-technical-notes.md](../../data-model/systems-status-view-technical-notes.md) |
  | `## Problem Statement` | [../../data-model/systems-status-view-technical-notes.md#problem-statement](../../data-model/systems-status-view-technical-notes.md#problem-statement) |
  | `## Root Cause Analysis` | [../../data-model/systems-status-view-technical-notes.md#root-cause-analysis](../../data-model/systems-status-view-technical-notes.md#root-cause-analysis) |
  | `### Initial Approach (Broken)` | [data-model/systems-status-view-technical-notes.md#initial-approach-broken](../../data-model/systems-status-view-technical-notes.md#initial-approach-broken) |
  | `### Hash Extraction Attempt (Failed)` | [data-model/systems-status-view-technical-notes.md#hash-extraction-attempt-failed](../../data-model/systems-status-view-technical-notes.md#hash-extraction-attempt-failed) |
  | `## Solution` | [../../data-model/systems-status-view-technical-notes.md#solution](../../data-model/systems-status-view-technical-notes.md#solution) |
  | `### Correct Approach` | [data-model/systems-status-view-technical-notes.md#correct-approach](../../data-model/systems-status-view-technical-notes.md#correct-approach) |
  | `### Key Insight` | [data-model/systems-status-view-technical-notes.md#key-insight](../../data-model/systems-status-view-technical-notes.md#key-insight) |
  | `## Implementation Notes` | [../../data-model/systems-status-view-technical-notes.md#implementation-notes](../../data-model/systems-status-view-technical-notes.md#implementation-notes) |
  | `## Database Schema Dependencies` | [../../data-model/systems-status-view-technical-notes.md#database-schema-dependencies](../../data-model/systems-status-view-technical-notes.md#database-schema-dependencies) |
  | `## Future Considerations` | [../../data-model/systems-status-view-technical-notes.md#future-considerations](../../data-model/systems-status-view-technical-notes.md#future-considerations) |
  | `# Deployment Timeline View - Developer Notes` (part heading) | [data-model/commit-deployment-timeline-developer-notes.md#deployment-timeline-view---developer-notes](../../data-model/commit-deployment-timeline-developer-notes.md#deployment-timeline-view---developer-notes) |
  | `## Problem Statement` | [../../data-model/commit-deployment-timeline-developer-notes.md#problem-statement](../../data-model/commit-deployment-timeline-developer-notes.md#problem-statement) |
  | `## Root Cause Analysis` | [../../data-model/commit-deployment-timeline-developer-notes.md#root-cause-analysis](../../data-model/commit-deployment-timeline-developer-notes.md#root-cause-analysis) |
  | `### Original Broken Approach` | [data-model/commit-deployment-timeline-developer-notes.md#original-broken-approach](../../data-model/commit-deployment-timeline-developer-notes.md#original-broken-approach) |
  | `### Data Reality Check` | [data-model/commit-deployment-timeline-developer-notes.md#data-reality-check](../../data-model/commit-deployment-timeline-developer-notes.md#data-reality-check) |
  | `## Architecture Understanding` | [../../data-model/commit-deployment-timeline-developer-notes.md#architecture-understanding](../../data-model/commit-deployment-timeline-developer-notes.md#architecture-understanding) |
  | `### What Crystal Forge Actually Tracks` | [data-model/commit-deployment-timeline-developer-notes.md#what-crystal-forge-actually-tracks](../../data-model/commit-deployment-timeline-developer-notes.md#what-crystal-forge-actually-tracks) |
  | `### The "Deployment" Inference Problem` | [data-model/commit-deployment-timeline-developer-notes.md#the-deployment-inference-problem](../../data-model/commit-deployment-timeline-developer-notes.md#the-deployment-inference-problem) |
  | `## Solution Approach` | [../../data-model/commit-deployment-timeline-developer-notes.md#solution-approach](../../data-model/commit-deployment-timeline-developer-notes.md#solution-approach) |
  | `### Key Insights` | [data-model/commit-deployment-timeline-developer-notes.md#key-insights](../../data-model/commit-deployment-timeline-developer-notes.md#key-insights) |
  | `### Implementation Strategy` | [data-model/commit-deployment-timeline-developer-notes.md#implementation-strategy](../../data-model/commit-deployment-timeline-developer-notes.md#implementation-strategy) |
  | `### What the View Now Shows` | [data-model/commit-deployment-timeline-developer-notes.md#what-the-view-now-shows](../../data-model/commit-deployment-timeline-developer-notes.md#what-the-view-now-shows) |
  | `## Limitations and Caveats` | [../../data-model/commit-deployment-timeline-developer-notes.md#limitations-and-caveats](../../data-model/commit-deployment-timeline-developer-notes.md#limitations-and-caveats) |
  | `### Data Quality Issues` | [data-model/commit-deployment-timeline-developer-notes.md#data-quality-issues](../../data-model/commit-deployment-timeline-developer-notes.md#data-quality-issues) |
  | `### Conceptual Limitations` | [data-model/commit-deployment-timeline-developer-notes.md#conceptual-limitations](../../data-model/commit-deployment-timeline-developer-notes.md#conceptual-limitations) |
  | `### Query Performance` | [data-model/commit-deployment-timeline-developer-notes.md#query-performance](../../data-model/commit-deployment-timeline-developer-notes.md#query-performance) |
  | `## Alternative Approaches Considered` | [../../data-model/commit-deployment-timeline-developer-notes.md#alternative-approaches-considered](../../data-model/commit-deployment-timeline-developer-notes.md#alternative-approaches-considered) |
  | `### Direct Deployment Tracking` | [data-model/commit-deployment-timeline-developer-notes.md#direct-deployment-tracking](../../data-model/commit-deployment-timeline-developer-notes.md#direct-deployment-tracking) |
  | `### Derivation Path Correlation` | [data-model/commit-deployment-timeline-developer-notes.md#derivation-path-correlation](../../data-model/commit-deployment-timeline-developer-notes.md#derivation-path-correlation) |
  | `### Agent-Reported Commit Hash` | [data-model/commit-deployment-timeline-developer-notes.md#agent-reported-commit-hash](../../data-model/commit-deployment-timeline-developer-notes.md#agent-reported-commit-hash) |
  | `## Recommendations` | [../../data-model/commit-deployment-timeline-developer-notes.md#recommendations](../../data-model/commit-deployment-timeline-developer-notes.md#recommendations) |
  | `### Short Term` | [data-model/commit-deployment-timeline-developer-notes.md#short-term](../../data-model/commit-deployment-timeline-developer-notes.md#short-term) |
  | `### Long Term` | [data-model/commit-deployment-timeline-developer-notes.md#long-term](../../data-model/commit-deployment-timeline-developer-notes.md#long-term) |
  | `### Database Optimizations` | [data-model/commit-deployment-timeline-developer-notes.md#database-optimizations](../../data-model/commit-deployment-timeline-developer-notes.md#database-optimizations) |
  | `## Usage Notes` | [../../data-model/commit-deployment-timeline-developer-notes.md#usage-notes](../../data-model/commit-deployment-timeline-developer-notes.md#usage-notes) |
  | `### Expected Behavior` | [data-model/commit-deployment-timeline-developer-notes.md#expected-behavior](../../data-model/commit-deployment-timeline-developer-notes.md#expected-behavior) |
  | `### Debugging Steps` | [data-model/commit-deployment-timeline-developer-notes.md#debugging-steps](../../data-model/commit-deployment-timeline-developer-notes.md#debugging-steps) |
  | `### Monitoring Queries` | [data-model/commit-deployment-timeline-developer-notes.md#monitoring-queries](../../data-model/commit-deployment-timeline-developer-notes.md#monitoring-queries) |
  | `# Systems Status Logic - Developer Documentation` (part heading) | [concepts/legacy-system-status-determination.md#systems-status-logic---developer-documentation](../../concepts/legacy-system-status-determination.md#systems-status-logic---developer-documentation) |
  | `## Status Determination Logic` | [../../concepts/legacy-system-status-determination.md#status-determination-logic](../../concepts/legacy-system-status-determination.md#status-determination-logic) |
  | `### Key Principle: Dry-Run Success = Deployable Configuration` | [concepts/legacy-system-status-determination.md#key-principle-dry-run-success--deployable-configuration](../../concepts/legacy-system-status-determination.md#key-principle-dry-run-success--deployable-configuration) |
  | `### Status Progression and Meaning` | [concepts/legacy-system-status-determination.md#status-progression-and-meaning](../../concepts/legacy-system-status-determination.md#status-progression-and-meaning) |
  | `### Statuses That Count as "System Has This Commit"` | [concepts/legacy-system-status-determination.md#statuses-that-count-as-system-has-this-commit](../../concepts/legacy-system-status-determination.md#statuses-that-count-as-system-has-this-commit) |
  | `### Statuses That DON'T Count` | [concepts/legacy-system-status-determination.md#statuses-that-dont-count](../../concepts/legacy-system-status-determination.md#statuses-that-dont-count) |
  | `## System Status Categories` | [../../concepts/legacy-system-status-determination.md#system-status-categories](../../concepts/legacy-system-status-determination.md#system-status-categories) |
  | `### 🟢 Up to Date` | [concepts/legacy-system-status-determination.md#-up-to-date](../../concepts/legacy-system-status-determination.md#-up-to-date) |
  | `### 🔴 Outdated` | [concepts/legacy-system-status-determination.md#-outdated](../../concepts/legacy-system-status-determination.md#-outdated) |
  | `### 🟤 Unknown State` | [concepts/legacy-system-status-determination.md#-unknown-state](../../concepts/legacy-system-status-determination.md#-unknown-state) |
  | `### ⚫ Offline` | [concepts/legacy-system-status-determination.md#-offline](../../concepts/legacy-system-status-determination.md#-offline) |
  | `## Implementation Details` | [../../concepts/legacy-system-status-determination.md#implementation-details](../../concepts/legacy-system-status-determination.md#implementation-details) |
  | `### Latest Deployable Configuration Query` | [concepts/legacy-system-status-determination.md#latest-deployable-configuration-query](../../concepts/legacy-system-status-determination.md#latest-deployable-configuration-query) |
  | `### Key Join Logic` | [concepts/legacy-system-status-determination.md#key-join-logic](../../concepts/legacy-system-status-determination.md#key-join-logic) |
  | `## Architectural Context` | [../../concepts/legacy-system-status-determination.md#architectural-context](../../concepts/legacy-system-status-determination.md#architectural-context) |
  | `### Why "Dry-Run Success" Matters` | [concepts/legacy-system-status-determination.md#why-dry-run-success-matters](../../concepts/legacy-system-status-determination.md#why-dry-run-success-matters) |
  | `### Data Model Limitations` | [concepts/legacy-system-status-determination.md#data-model-limitations](../../concepts/legacy-system-status-determination.md#data-model-limitations) |
  | `## Common Scenarios` | [../../concepts/legacy-system-status-determination.md#common-scenarios](../../concepts/legacy-system-status-determination.md#common-scenarios) |
  | `### Scenario 1: Normal Progression` | [concepts/legacy-system-status-determination.md#scenario-1-normal-progression](../../concepts/legacy-system-status-determination.md#scenario-1-normal-progression) |
  | `### Scenario 2: Build Failure` | [concepts/legacy-system-status-determination.md#scenario-2-build-failure](../../concepts/legacy-system-status-determination.md#scenario-2-build-failure) |
  | `### Scenario 3: Evaluation Failure` | [concepts/legacy-system-status-determination.md#scenario-3-evaluation-failure](../../concepts/legacy-system-status-determination.md#scenario-3-evaluation-failure) |
  | `### Scenario 4: Multiple Commits` | [concepts/legacy-system-status-determination.md#scenario-4-multiple-commits](../../concepts/legacy-system-status-determination.md#scenario-4-multiple-commits) |
  | `## Troubleshooting` | [../../concepts/legacy-system-status-determination.md#troubleshooting](../../concepts/legacy-system-status-determination.md#troubleshooting) |
  | `### System Shows "Unknown State"` | [concepts/legacy-system-status-determination.md#system-shows-unknown-state](../../concepts/legacy-system-status-determination.md#system-shows-unknown-state) |
  | `### System Shows "Outdated" Unexpectedly` | [concepts/legacy-system-status-determination.md#system-shows-outdated-unexpectedly](../../concepts/legacy-system-status-determination.md#system-shows-outdated-unexpectedly) |
  | `### System Shows Wrong Status` | [concepts/legacy-system-status-determination.md#system-shows-wrong-status](../../concepts/legacy-system-status-determination.md#system-shows-wrong-status) |
  | `## Future Improvements` | [../../concepts/legacy-system-status-determination.md#future-improvements](../../concepts/legacy-system-status-determination.md#future-improvements) |
  | `### Explicit Deployment Tracking` | [concepts/legacy-system-status-determination.md#explicit-deployment-tracking](../../concepts/legacy-system-status-determination.md#explicit-deployment-tracking) |
  | `### Agent Enhancements` | [concepts/legacy-system-status-determination.md#agent-enhancements](../../concepts/legacy-system-status-determination.md#agent-enhancements) |
  | `### Status Refinements` | [concepts/legacy-system-status-determination.md#status-refinements](../../concepts/legacy-system-status-determination.md#status-refinements) |
  | `# SystemD-Run Evaluation Isolation in Crystal Forge` (part heading) | [evaluation/systemd-run-evaluation-isolation.md#systemd-run-evaluation-isolation-in-crystal-forge](../../evaluation/systemd-run-evaluation-isolation.md#systemd-run-evaluation-isolation-in-crystal-forge) |
  | `## Background` | [../../evaluation/systemd-run-evaluation-isolation.md#background](../../evaluation/systemd-run-evaluation-isolation.md#background) |
  | `## The Problem` | [../../evaluation/systemd-run-evaluation-isolation.md#the-problem](../../evaluation/systemd-run-evaluation-isolation.md#the-problem) |
  | `### OOM Killer Behavior` | [evaluation/systemd-run-evaluation-isolation.md#oom-killer-behavior](../../evaluation/systemd-run-evaluation-isolation.md#oom-killer-behavior) |
  | `### Previous Architecture Limitations` | [evaluation/systemd-run-evaluation-isolation.md#previous-architecture-limitations](../../evaluation/systemd-run-evaluation-isolation.md#previous-architecture-limitations) |
  | `## The Solution: SystemD-Run Scope Isolation` | [../../evaluation/systemd-run-evaluation-isolation.md#the-solution-systemd-run-scope-isolation](../../evaluation/systemd-run-evaluation-isolation.md#the-solution-systemd-run-scope-isolation) |
  | `### Implementation Strategy` | [evaluation/systemd-run-evaluation-isolation.md#implementation-strategy](../../evaluation/systemd-run-evaluation-isolation.md#implementation-strategy) |
  | `### Fallback Architecture` | [evaluation/systemd-run-evaluation-isolation.md#fallback-architecture](../../evaluation/systemd-run-evaluation-isolation.md#fallback-architecture) |
  | `## Technical Implementation` | [../../evaluation/systemd-run-evaluation-isolation.md#technical-implementation](../../evaluation/systemd-run-evaluation-isolation.md#technical-implementation) |
  | `### Three-Tier Execution Strategy` | [evaluation/systemd-run-evaluation-isolation.md#three-tier-execution-strategy](../../evaluation/systemd-run-evaluation-isolation.md#three-tier-execution-strategy) |
  | `### Resource Limit Configuration` | [evaluation/systemd-run-evaluation-isolation.md#resource-limit-configuration](../../evaluation/systemd-run-evaluation-isolation.md#resource-limit-configuration) |
  | `## Fallback Scenarios` | [../../evaluation/systemd-run-evaluation-isolation.md#fallback-scenarios](../../evaluation/systemd-run-evaluation-isolation.md#fallback-scenarios) |
  | `### When Fallback Occurs` | [evaluation/systemd-run-evaluation-isolation.md#when-fallback-occurs](../../evaluation/systemd-run-evaluation-isolation.md#when-fallback-occurs) |
  | `### Fallback Detection Logic` | [evaluation/systemd-run-evaluation-isolation.md#fallback-detection-logic](../../evaluation/systemd-run-evaluation-isolation.md#fallback-detection-logic) |
  | `### Error Categorization` | [evaluation/systemd-run-evaluation-isolation.md#error-categorization](../../evaluation/systemd-run-evaluation-isolation.md#error-categorization) |
  | `## Benefits and Trade-offs` | [../../evaluation/systemd-run-evaluation-isolation.md#benefits-and-trade-offs](../../evaluation/systemd-run-evaluation-isolation.md#benefits-and-trade-offs) |
  | `### Benefits` | [evaluation/systemd-run-evaluation-isolation.md#benefits](../../evaluation/systemd-run-evaluation-isolation.md#benefits) |
  | `### Trade-offs` | [evaluation/systemd-run-evaluation-isolation.md#trade-offs](../../evaluation/systemd-run-evaluation-isolation.md#trade-offs) |
  | `### Performance Impact` | [evaluation/systemd-run-evaluation-isolation.md#performance-impact](../../evaluation/systemd-run-evaluation-isolation.md#performance-impact) |
  | `## Operational Considerations` | [../../evaluation/systemd-run-evaluation-isolation.md#operational-considerations](../../evaluation/systemd-run-evaluation-isolation.md#operational-considerations) |
  | `### Monitoring Points` | [evaluation/systemd-run-evaluation-isolation.md#monitoring-points](../../evaluation/systemd-run-evaluation-isolation.md#monitoring-points) |
  | `### Logging Strategy` | [evaluation/systemd-run-evaluation-isolation.md#logging-strategy](../../evaluation/systemd-run-evaluation-isolation.md#logging-strategy) |
  | `### Failure Modes` | [evaluation/systemd-run-evaluation-isolation.md#failure-modes](../../evaluation/systemd-run-evaluation-isolation.md#failure-modes) |
  | `## Architecture Evolution` | [../../evaluation/systemd-run-evaluation-isolation.md#architecture-evolution](../../evaluation/systemd-run-evaluation-isolation.md#architecture-evolution) |
  | `### Before: Monolithic Evaluation` | [evaluation/systemd-run-evaluation-isolation.md#before-monolithic-evaluation](../../evaluation/systemd-run-evaluation-isolation.md#before-monolithic-evaluation) |
  | `### After: Isolated Evaluation` | [evaluation/systemd-run-evaluation-isolation.md#after-isolated-evaluation](../../evaluation/systemd-run-evaluation-isolation.md#after-isolated-evaluation) |
  | `## Future Improvements` | [../../evaluation/systemd-run-evaluation-isolation.md#future-improvements](../../evaluation/systemd-run-evaluation-isolation.md#future-improvements) |
  | `### Enhanced Resource Management` | [evaluation/systemd-run-evaluation-isolation.md#enhanced-resource-management](../../evaluation/systemd-run-evaluation-isolation.md#enhanced-resource-management) |
  | `### Evaluation Caching` | [evaluation/systemd-run-evaluation-isolation.md#evaluation-caching](../../evaluation/systemd-run-evaluation-isolation.md#evaluation-caching) |
  | `### Advanced Isolation` | [evaluation/systemd-run-evaluation-isolation.md#advanced-isolation](../../evaluation/systemd-run-evaluation-isolation.md#advanced-isolation) |
- Unmapped content: none

### `docs/views/view_buildable_derivations.md`

- Title: Buildable Derivations View (`view_buildable_derivations`)
- Purpose: Describes the system-aware build queue view that workers claim work from.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-buildable-derivations.md](../../data-model/views/view-buildable-derivations.md) |
  | `## Overview` | [../../data-model/views/view-buildable-derivations.md#status](../../data-model/views/view-buildable-derivations.md#status) |
  | `## Example Output` | [../../data-model/views/view-buildable-derivations.md#current-definition](../../data-model/views/view-buildable-derivations.md#current-definition) |
  | `## Purpose` | [../../data-model/views/view-buildable-derivations.md#status](../../data-model/views/view-buildable-derivations.md#status) |
  | `## Core Logic` | [../../data-model/views/view-buildable-derivations.md#current-definition](../../data-model/views/view-buildable-derivations.md#current-definition) |
  | `## Key Fields` | [../../data-model/views/view-buildable-derivations.md#current-definition](../../data-model/views/view-buildable-derivations.md#current-definition) |
  | `## Example Queries` | [../../data-model/views/view-buildable-derivations.md#example-queries](../../data-model/views/view-buildable-derivations.md#example-queries) |
  | `## Operational Context` | [../../data-model/views/view-buildable-derivations.md#legacy-claim-sequence](../../data-model/views/view-buildable-derivations.md#legacy-claim-sequence) |
  | `## Performance Notes` | [../../data-model/views/view-buildable-derivations.md#current-definition](../../data-model/views/view-buildable-derivations.md#current-definition) |
  | `## Related Tables and Views` | [../../data-model/views/view-buildable-derivations.md#relationship-to-other-views](../../data-model/views/view-buildable-derivations.md#relationship-to-other-views) |
  | `## Migration Notes` | [../../data-model/views/view-buildable-derivations.md#status](../../data-model/views/view-buildable-derivations.md#status) |
- Unmapped content: none

### `docs/views/view_build_queue_status.md`

- Title: Build Queue Status View (`view_build_queue_status`)
- Purpose: Describes the system-level build queue monitoring view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-build-queue-status.md](../../data-model/views/view-build-queue-status.md) |
  | `## Overview` | [../../data-model/views/view-build-queue-status.md#overview](../../data-model/views/view-build-queue-status.md#overview) |
  | `## Example Output` | [../../data-model/views/view-build-queue-status.md#example-output](../../data-model/views/view-build-queue-status.md#example-output) |
  | `## Purpose` | [../../data-model/views/view-build-queue-status.md#purpose](../../data-model/views/view-build-queue-status.md#purpose) |
  | `## Core Logic` | [../../data-model/views/view-build-queue-status.md#core-logic](../../data-model/views/view-build-queue-status.md#core-logic) |
  | `## Key Fields` | [../../data-model/views/view-build-queue-status.md#key-fields](../../data-model/views/view-build-queue-status.md#key-fields) |
  | `## Example Queries` | [../../data-model/views/view-build-queue-status.md#example-queries](../../data-model/views/view-build-queue-status.md#example-queries) |
  | `## Operational Context` | [../../data-model/views/view-build-queue-status.md](../../data-model/views/view-build-queue-status.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Performance Notes` | [../../data-model/views/view-build-queue-status.md](../../data-model/views/view-build-queue-status.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Related Tables and Views` | [../../data-model/views/view-build-queue-status.md#related-tables-and-views](../../data-model/views/view-build-queue-status.md#related-tables-and-views) |
  | `## Migration Notes` | [../../data-model/views/view-build-queue-status.md](../../data-model/views/view-build-queue-status.md), [../cleanup-record.md](../cleanup-record.md) |
- Unmapped content: none

### `docs/views/view_commit_build_status.md`

- Title: Commit Build Status View
- Purpose: Describes per-commit and per-derivation build status.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-commit-build-status.md](../../data-model/views/view-commit-build-status.md) |
  | `## Overview` | [../../data-model/views/view-commit-build-status.md#overview](../../data-model/views/view-commit-build-status.md#overview) |
  | `## Build Status Categories` | [../../data-model/views/view-commit-build-status.md#build-status-categories](../../data-model/views/view-commit-build-status.md#build-status-categories) |
  | `## Key Information Levels` | [../../data-model/views/view-commit-build-status.md#key-information-levels](../../data-model/views/view-commit-build-status.md#key-information-levels) |
  | `## Important Fields` | [../../data-model/views/view-commit-build-status.md#important-fields](../../data-model/views/view-commit-build-status.md#important-fields) |
  | `## Data Organization` | [../../data-model/views/view-commit-build-status.md#data-organization](../../data-model/views/view-commit-build-status.md#data-organization) |
  | `## Primary Use Cases` | [../../data-model/views/view-commit-build-status.md#primary-use-cases](../../data-model/views/view-commit-build-status.md#primary-use-cases) |
  | `## Query Examples` | [../../data-model/views/view-commit-build-status.md#query-examples](../../data-model/views/view-commit-build-status.md#query-examples) |
  | `## Integration Notes` | [../../data-model/views/view-commit-build-status.md#integration-notes](../../data-model/views/view-commit-build-status.md#integration-notes) |
- Unmapped content: none

### `docs/views/view_commit_deployment_timeline.md`

- Title: Commit Deployment Timeline View
- Purpose: Describes the commit-centric deployment timeline view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-commit-deployment-timeline.md](../../data-model/views/view-commit-deployment-timeline.md) |
  | `## Overview` | [../../data-model/views/view-commit-deployment-timeline.md#overview](../../data-model/views/view-commit-deployment-timeline.md#overview) |
  | `## Time Scope` | [../../data-model/views/view-commit-deployment-timeline.md#time-scope](../../data-model/views/view-commit-deployment-timeline.md#time-scope) |
  | `## Key Concepts` | [../../data-model/views/view-commit-deployment-timeline.md#key-concepts](../../data-model/views/view-commit-deployment-timeline.md#key-concepts) |
  | `## Important Fields` | [../../data-model/views/view-commit-deployment-timeline.md#important-fields](../../data-model/views/view-commit-deployment-timeline.md#important-fields) |
  | `## Data Ordering` | [../../data-model/views/view-commit-deployment-timeline.md#data-ordering](../../data-model/views/view-commit-deployment-timeline.md#data-ordering) |
  | `## Use Cases` | [../../data-model/views/view-commit-deployment-timeline.md#use-cases](../../data-model/views/view-commit-deployment-timeline.md#use-cases) |
  | `## Operational Workflows` | [../../data-model/views/view-commit-deployment-timeline.md#operational-workflows](../../data-model/views/view-commit-deployment-timeline.md#operational-workflows) |
  | `## Query Examples` | [../../data-model/views/view-commit-deployment-timeline.md#query-examples](../../data-model/views/view-commit-deployment-timeline.md#query-examples) |
  | `## Integration Notes` | [../../data-model/views/view-commit-deployment-timeline.md#integration-notes](../../data-model/views/view-commit-deployment-timeline.md#integration-notes) |
  | `## Limitations` | [../../data-model/views/view-commit-deployment-timeline.md#limitations](../../data-model/views/view-commit-deployment-timeline.md#limitations) |
- Unmapped content: none

### `docs/views/view_commit_nixos_table.md`

- Title: NixOS Commit Table View (`view_commit_nixos_table`)
- Purpose: Describes the compact commit-centric NixOS derivation table view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-commit-nixos-table.md](../../data-model/views/view-commit-nixos-table.md) |
  | `## Overview` | [../../data-model/views/view-commit-nixos-table.md#overview](../../data-model/views/view-commit-nixos-table.md#overview) |
  | `## Columns` | [../../data-model/views/view-commit-nixos-table.md#columns](../../data-model/views/view-commit-nixos-table.md#columns) |
  | `## Typical Use` | [../../data-model/views/view-commit-nixos-table.md#typical-use](../../data-model/views/view-commit-nixos-table.md#typical-use) |
  | `## Example Queries` | [../../data-model/views/view-commit-nixos-table.md#example-queries](../../data-model/views/view-commit-nixos-table.md#example-queries) |
  | `## Panel Tips (Grafana)` | [../../data-model/views/view-commit-nixos-table.md](../../data-model/views/view-commit-nixos-table.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Related` | [../../data-model/views/view-commit-nixos-table.md#related](../../data-model/views/view-commit-nixos-table.md#related) |
- Unmapped content: none

### `docs/views/view_config_timeline.md`

- Title: Config Timeline View (`view_config_timeline`)
- Purpose: Describes the Grafana config timeline view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-config-timeline.md](../../data-model/views/view-config-timeline.md) |
  | `## Overview` | [../../data-model/views/view-config-timeline.md#overview](../../data-model/views/view-config-timeline.md#overview) |
  | `## What it Shows` | [../../data-model/views/view-config-timeline.md#what-it-shows](../../data-model/views/view-config-timeline.md#what-it-shows) |
  | `## Data Sources & Logic` | [../../data-model/views/view-config-timeline.md#data-sources--logic](../../data-model/views/view-config-timeline.md#data-sources--logic) |
  | `## Columns` | [../../data-model/views/view-config-timeline.md#columns](../../data-model/views/view-config-timeline.md#columns) |
  | `## Time Scope & Ordering` | [../../data-model/views/view-config-timeline.md#time-scope--ordering](../../data-model/views/view-config-timeline.md#time-scope--ordering) |
  | `## Typical Grafana Query` | [../../data-model/views/view-config-timeline.md](../../data-model/views/view-config-timeline.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Handy Parse Helpers (Postgres)` | [../../data-model/views/view-config-timeline.md#handy-parse-helpers-postgres](../../data-model/views/view-config-timeline.md#handy-parse-helpers-postgres) |
  | `## Operational Notes` | [../../data-model/views/view-config-timeline.md#operational-notes](../../data-model/views/view-config-timeline.md#operational-notes) |
  | `## Performance Hints` | [../../data-model/views/view-config-timeline.md](../../data-model/views/view-config-timeline.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Related Views` | [../../data-model/views/view-config-timeline.md#related-views](../../data-model/views/view-config-timeline.md#related-views) |
- Unmapped content: none

### `docs/views/view_derivation_status_breakdown.md`

- Title: Derivation Status Breakdown View
- Purpose: Describes derivation counts and metrics per status.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-derivation-status-breakdown.md](../../data-model/views/view-derivation-status-breakdown.md) |
  | `## Overview` | [../../data-model/views/view-derivation-status-breakdown.md#overview](../../data-model/views/view-derivation-status-breakdown.md#overview) |
  | `## Status Analysis` | [../../data-model/views/view-derivation-status-breakdown.md#status-analysis](../../data-model/views/view-derivation-status-breakdown.md#status-analysis) |
  | `## Key Fields` | [../../data-model/views/view-derivation-status-breakdown.md#key-fields](../../data-model/views/view-derivation-status-breakdown.md#key-fields) |
  | `## Status Categories` | [../../data-model/views/view-derivation-status-breakdown.md#status-categories](../../data-model/views/view-derivation-status-breakdown.md#status-categories) |
  | `## Metrics Interpretation` | [../../data-model/views/view-derivation-status-breakdown.md#metrics-interpretation](../../data-model/views/view-derivation-status-breakdown.md#metrics-interpretation) |
  | `## Operational Insights` | [../../data-model/views/view-derivation-status-breakdown.md#operational-insights](../../data-model/views/view-derivation-status-breakdown.md#operational-insights) |
  | `## Use Cases` | [../../data-model/views/view-derivation-status-breakdown.md#use-cases](../../data-model/views/view-derivation-status-breakdown.md#use-cases) |
  | `## Query Examples` | [../../data-model/views/view-derivation-status-breakdown.md#query-examples](../../data-model/views/view-derivation-status-breakdown.md#query-examples) |
  | `## Notes` | [../../data-model/views/view-derivation-status-breakdown.md#notes](../../data-model/views/view-derivation-status-breakdown.md#notes) |
- Unmapped content: none

### `docs/views/view_flake_recent_commits.md`

- Title: Flake Recent Commits View (`view_flake_recent_commits`)
- Purpose: Describes the per-flake recent commit snapshot view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-flake-recent-commits.md](../../data-model/views/view-flake-recent-commits.md) |
  | `## Overview` | [../../data-model/views/view-flake-recent-commits.md#overview](../../data-model/views/view-flake-recent-commits.md#overview) |
  | `## Key Behavior` | [../../data-model/views/view-flake-recent-commits.md#key-behavior](../../data-model/views/view-flake-recent-commits.md#key-behavior) |
  | `## Important Fields` | [../../data-model/views/view-flake-recent-commits.md#important-fields](../../data-model/views/view-flake-recent-commits.md#important-fields) |
  | `## Data Ordering` | [../../data-model/views/view-flake-recent-commits.md#data-ordering](../../data-model/views/view-flake-recent-commits.md#data-ordering) |
  | `## Primary Use Cases` | [../../data-model/views/view-flake-recent-commits.md#primary-use-cases](../../data-model/views/view-flake-recent-commits.md#primary-use-cases) |
  | `## Example Queries` | [../../data-model/views/view-flake-recent-commits.md#example-queries](../../data-model/views/view-flake-recent-commits.md#example-queries) |
  | `## Integration Notes` | [../../data-model/views/view-flake-recent-commits.md#integration-notes](../../data-model/views/view-flake-recent-commits.md#integration-notes) |
- Unmapped content: none

### `docs/views/view_nixos_derivation_build_queue.md`

- Title: NixOS Derivation Build Queue View (`view_nixos_derivation_build_queue`)
- Purpose: Describes the ordered NixOS derivation build queue view.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-nixos-derivation-build-queue.md](../../data-model/views/view-nixos-derivation-build-queue.md) |
  | `## Overview` | [../../data-model/views/view-nixos-derivation-build-queue.md#status](../../data-model/views/view-nixos-derivation-build-queue.md#status) |
  | `## Example Output` | [../../data-model/views/view-nixos-derivation-build-queue.md#example-output](../../data-model/views/view-nixos-derivation-build-queue.md#example-output) |
  | `## Purpose` | [../../data-model/views/view-nixos-derivation-build-queue.md#status](../../data-model/views/view-nixos-derivation-build-queue.md#status) |
  | `## Core Logic` | [../../data-model/views/view-nixos-derivation-build-queue.md#definition](../../data-model/views/view-nixos-derivation-build-queue.md#definition) |
  | `## Key Fields` | [../../data-model/views/view-nixos-derivation-build-queue.md#definition](../../data-model/views/view-nixos-derivation-build-queue.md#definition) |
  | `## Example Queries` | [../../data-model/views/view-nixos-derivation-build-queue.md#example-queries](../../data-model/views/view-nixos-derivation-build-queue.md#example-queries) |
  | `## Operational Context` | [../../data-model/views/view-nixos-derivation-build-queue.md#definition](../../data-model/views/view-nixos-derivation-build-queue.md#definition) |
  | `## Performance Notes` | [../../data-model/views/view-nixos-derivation-build-queue.md#definition](../../data-model/views/view-nixos-derivation-build-queue.md#definition) |
  | `## Related Views` | [../../data-model/views/view-nixos-derivation-build-queue.md#related-views](../../data-model/views/view-nixos-derivation-build-queue.md#related-views) |
- Unmapped content: none

### `docs/views/view_system_deployment_status.md`

- Title: System Deployment Status View
- Purpose: Describes the current deployment status view for systems.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-system-deployment-status.md](../../data-model/views/view-system-deployment-status.md) |
  | `## Overview` | [../../data-model/views/view-system-deployment-status.md#overview](../../data-model/views/view-system-deployment-status.md#overview) |
  | `## Status Categories` | [../../data-model/views/view-system-deployment-status.md#status-categories](../../data-model/views/view-system-deployment-status.md#status-categories) |
  | `## Key Relationships` | [../../data-model/views/view-system-deployment-status.md#key-relationships](../../data-model/views/view-system-deployment-status.md#key-relationships) |
  | `## Important Fields` | [../../data-model/views/view-system-deployment-status.md#important-fields](../../data-model/views/view-system-deployment-status.md#important-fields) |
  | `## Commit Counting Logic` | [../../data-model/views/view-system-deployment-status.md#commit-counting-logic](../../data-model/views/view-system-deployment-status.md#commit-counting-logic) |
  | `## Operational Implications` | [../../data-model/views/view-system-deployment-status.md#operational-implications](../../data-model/views/view-system-deployment-status.md#operational-implications) |
  | `## Use Cases` | [../../data-model/views/view-system-deployment-status.md#use-cases](../../data-model/views/view-system-deployment-status.md#use-cases) |
  | `## Query Examples` | [../../data-model/views/view-system-deployment-status.md#query-examples](../../data-model/views/view-system-deployment-status.md#query-examples) |
  | `## Notes` | [../../data-model/views/view-system-deployment-status.md#notes](../../data-model/views/view-system-deployment-status.md#notes) |
- Unmapped content: none

### `docs/views/view_system_heartbeat_status.md`

- Title: System Heartbeat Status View
- Purpose: Describes the connectivity status view for systems.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [data-model/views/view-system-heartbeat-status.md](../../data-model/views/view-system-heartbeat-status.md) |
  | `## Overview` | [../../data-model/views/view-system-heartbeat-status.md#overview](../../data-model/views/view-system-heartbeat-status.md#overview) |
  | `## Status Categories` | [../../data-model/views/view-system-heartbeat-status.md#status-categories](../../data-model/views/view-system-heartbeat-status.md#status-categories) |
  | `## Logic` | [../../data-model/views/view-system-heartbeat-status.md#logic](../../data-model/views/view-system-heartbeat-status.md#logic) |
  | `## Key Fields` | [../../data-model/views/view-system-heartbeat-status.md#key-fields](../../data-model/views/view-system-heartbeat-status.md#key-fields) |
  | `## Time Boundaries` | [../../data-model/views/view-system-heartbeat-status.md#time-boundaries](../../data-model/views/view-system-heartbeat-status.md#time-boundaries) |
  | `## Use Cases` | [../../data-model/views/view-system-heartbeat-status.md#use-cases](../../data-model/views/view-system-heartbeat-status.md#use-cases) |
  | `## Related Views` | [../../data-model/views/view-system-heartbeat-status.md#related-views](../../data-model/views/view-system-heartbeat-status.md#related-views) |
  | `## Query Examples` | [../../data-model/views/view-system-heartbeat-status.md#query-examples](../../data-model/views/view-system-heartbeat-status.md#query-examples) |
- Unmapped content: none

## Notes for the lead

- `docs/views/view_config_timeline.png` is retained in place (image asset). [data-model/views/view-config-timeline.md](../../data-model/views/view-config-timeline.md) links to it with a corrected relative path.
- `docs/dev-notes.md` contains a fourth part, "SystemD-Run Evaluation Isolation in Crystal Forge", beyond the three parts named in the assignment. It moved to [evaluation/systemd-run-evaluation-isolation.md](../../evaluation/systemd-run-evaluation-isolation.md).
- Status notes in the destination concepts record disagreements between sources and between sources and code. The migration did not resolve them.
