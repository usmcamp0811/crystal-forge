---
type: Reference
title: "Migration manifest: overview and architecture"
description: Maps the problem statement, constraints, context, roadmap, architecture ADR, system overview, flow documents, derivation status document, backend workspace document, Backlog flow copies, and pitch deck slides to their OKF destinations.
tags:
  - crystal-forge
  - migration
---

# Migration manifest: overview and architecture

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/problem_statement.md` | [overview/problem-statement.md](../../overview/problem-statement.md) | moved | complete |
| `docs/requirments.md` | [overview/constraints-and-policy.md](../../overview/constraints-and-policy.md) | moved | complete |
| `docs/context.md` | [overview/system-context-and-current-state.md](../../overview/system-context-and-current-state.md) | moved | complete |
| `ROADMAP.md` | [overview/roadmap.md](../../overview/roadmap.md) | moved | complete |
| `docs/architecture.md` | [components/core-components.md](../../components/core-components.md), [architecture/data-flows.md](../../architecture/data-flows.md), [architecture/event-driven-queues.md](../../architecture/event-driven-queues.md), [decisions/adr-000-architecture-overview.md](../../decisions/adr-000-architecture-overview.md), [operations/observability-and-troubleshooting.md](../../operations/observability-and-troubleshooting.md), [ui/frontend-development-overview.md](../../ui/frontend-development-overview.md) | split | complete |
| `docs/specs/00-system-overview.md` | [overview/system-overview.md](../../overview/system-overview.md), [data-model/core-entities-and-relationships.md](../../data-model/core-entities-and-relationships.md), [workflows/evaluation-and-build-queue-pipeline.md](../../workflows/evaluation-and-build-queue-pipeline.md), [deployment/deployment-flow.md](../../deployment/deployment-flow.md), [security/authentication-and-authorization-overview.md](../../security/authentication-and-authorization-overview.md), [operations/local-development-workflow.md](../../operations/local-development-workflow.md), [operations/server-configuration-reference.md](../../operations/server-configuration-reference.md) | split | complete |
| `docs/eval-build-deploy-flow.md` | [workflows/commit-eval-build-cache-deploy-flow.md](../../workflows/commit-eval-build-cache-deploy-flow.md) | moved | complete |
| `docs/eval-build-deploy-sequence.md` | [workflows/commit-eval-build-cache-deploy-sequence.md](../../workflows/commit-eval-build-cache-deploy-sequence.md) | moved | complete |
| `docs/store-path-flow.md` | [workflows/store-path-flow.md](../../workflows/store-path-flow.md) | moved | complete |
| `docs/derivation-status.md` | [concepts/derivation-status-lifecycle.md](../../concepts/derivation-status-lifecycle.md), [architecture/derivation-processing-loops.md](../../architecture/derivation-processing-loops.md), [caches/cache-push-process.md](../../caches/cache-push-process.md), [deployment/deployment-flow.md](../../deployment/deployment-flow.md), [operations/observability-and-troubleshooting.md](../../operations/observability-and-troubleshooting.md) | split | complete |
| `packages/default/WORKSPACE.md` | [architecture/backend-cargo-workspace.md](../../architecture/backend-cargo-workspace.md); the original path holds a short pointer | replaced | complete |
| `backlog/docs/doc-2 - Commit-Eval-Build-Cache-Deploy-Flow.md` | [workflows/commit-eval-build-cache-deploy-flow.md](../../workflows/commit-eval-build-cache-deploy-flow.md); the Backlog document keeps its frontmatter and holds a pointer | replaced | complete |
| `backlog/docs/doc-3 - Commit-Eval-Build-Cache-Deploy-Sequence.md` | [workflows/commit-eval-build-cache-deploy-sequence.md](../../workflows/commit-eval-build-cache-deploy-sequence.md); the Backlog document keeps its frontmatter and holds a pointer | replaced | complete |
| `packages/slides/slides.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/00-title.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/01-intro.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/02-compliance-burden.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/03-current-approach.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/04-traditional-tools.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/05-nix-changes-the-game.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/06-nix-single-source.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/07-cf-what-if-we-made-this-simple.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/08-cf-who-benefits.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/09-cf-how-it-works.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/10-cf-agent-lifecycle.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/11-cf-build-coordination.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/12-cf-beyond-config-mgmt.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/13-cf-immutable-by-desding.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/14-cf-built-for-audits.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/15-cf-reporting.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/16-scope-boundaries.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/17-vision.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |
| `packages/slides/slides/18-get-involved.md` | retained in place; see [pointer](../../overview/product-vision-pitch-deck.md) | retained | complete |

## Source inventory

### `docs/problem_statement.md`

- Title: Crystal Forge Problem Brief
- Purpose: States who is hurt, the problem, why now, scope limits, and the success signal.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Who Is Hurt` | [../../overview/problem-statement.md#who-is-hurt](../../overview/problem-statement.md#who-is-hurt) |
  | `## The Problem` | [../../overview/problem-statement.md#the-problem](../../overview/problem-statement.md#the-problem) |
  | `## Why Now` | [../../overview/problem-statement.md#why-now](../../overview/problem-statement.md#why-now) |
  | `## Out of Scope (This Phase)` | [../../overview/problem-statement.md#out-of-scope-this-phase](../../overview/problem-statement.md#out-of-scope-this-phase) |
  | `## One Success Signal` | [../../overview/problem-statement.md#one-success-signal](../../overview/problem-statement.md#one-success-signal) |
- Unmapped content: none

### `docs/requirments.md`

- Title: Crystal Forge Constraints & Policy
- Purpose: Lists non-negotiable constraints, flexible elements, and policy requirements.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Non-Negotiable Constraints` (Security & Memory Safety, Data Handling & Privacy, Platform Requirements, Regulatory Compliance) | [../../overview/constraints-and-policy.md#non-negotiable-constraints](../../overview/constraints-and-policy.md#non-negotiable-constraints) |
  | `## Flexible Elements` | [../../overview/constraints-and-policy.md#flexible-elements](../../overview/constraints-and-policy.md#flexible-elements) |
  | `## Policy Requirements` | [../../overview/constraints-and-policy.md#policy-requirements](../../overview/constraints-and-policy.md#policy-requirements) |
- Unmapped content: none

### `docs/context.md`

- Title: Crystal Forge Context & Current State
- Purpose: Describes system context, components, a point-in-time current state, communication patterns, and the scaling model.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## System Context` (Upstream Dependencies, System Components) | [../../overview/system-context-and-current-state.md](../../overview/system-context-and-current-state.md), [../cleanup-record.md](../cleanup-record.md) |
  | `### Current State` | [overview/system-context-and-current-state.md](../../overview/system-context-and-current-state.md) (flagged as stale-risk) (section rewritten during cleanup; see [cleanup record](../cleanup-record.md)) |
  | `### Communication Patterns` | [overview/system-context-and-current-state.md#communication-patterns](../../overview/system-context-and-current-state.md#communication-patterns) |
  | `### Scaling Model` | [overview/system-context-and-current-state.md#scaling-model](../../overview/system-context-and-current-state.md#scaling-model) |
- Unmapped content: none

### `ROADMAP.md`

- Title: Crystal Forge Roadmap
- Purpose: Lists current capabilities and planned work with six roadmap items and future possibilities.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Where We Are` | [../../overview/roadmap.md](../../overview/roadmap.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Where We're Going` (items 1 to 6) | [../../overview/roadmap.md](../../overview/roadmap.md) |
  | `## Future Possibilities` | [../../overview/roadmap.md#future-possibilities](../../overview/roadmap.md#future-possibilities) |
- Unmapped content: none

### `docs/architecture.md`

- Title: ADR-000: Crystal Forge Architecture Overview
- Purpose: Records the accepted architecture decision, core components, data flows, queue architecture, decisions, consequences, and future evolution.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Status`, `## Context`, `## Decision` | [../../decisions/adr-000-architecture-overview.md#status](../../decisions/adr-000-architecture-overview.md#status) |
  | `### Core Components` (diagram, Agent, Server, Builder) | [components/core-components.md](../../components/core-components.md) |
  | `### Data Flows` | [architecture/data-flows.md](../../architecture/data-flows.md) |
  | `### Event-Driven Queue Architecture` | [architecture/event-driven-queues.md](../../architecture/event-driven-queues.md) |
  | `### Key Architectural Decisions` | [decisions/adr-000-architecture-overview.md](../../decisions/adr-000-architecture-overview.md) (section rewritten during cleanup; see [cleanup record](../cleanup-record.md)) |
  | `### Observability Points` | [operations/observability-and-troubleshooting.md#observability-points](../../operations/observability-and-troubleshooting.md#observability-points) |
  | `## Consequences` | [../../decisions/adr-000-architecture-overview.md#consequences](../../decisions/adr-000-architecture-overview.md#consequences) |
  | `## Frontend Development` | [../../ui/frontend-development-overview.md](../../ui/frontend-development-overview.md) |
  | `## Future Evolution` | [../../decisions/adr-000-architecture-overview.md](../../decisions/adr-000-architecture-overview.md), [../cleanup-record.md](../cleanup-record.md) |
- Unmapped content: none

### `docs/specs/00-system-overview.md`

- Title: Crystal Forge - System Overview
- Purpose: Gives the product overview, architecture, data model, walkthroughs, development workflow, and configuration.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## What is Crystal Forge?` | [../../overview/system-overview.md#what-is-crystal-forge](../../overview/system-overview.md#what-is-crystal-forge) |
  | `## High-Level Architecture` (including `### Key Components`) | [../../overview/system-overview.md](../../overview/system-overview.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Data Model` | [../../data-model/core-entities-and-relationships.md](../../data-model/core-entities-and-relationships.md) |
  | `## How It Works` (heading) | [../../overview/system-overview.md#how-it-works](../../overview/system-overview.md#how-it-works) |
  | `### 1. Registering a System` | [security/authentication-and-authorization-overview.md#1-registering-a-system](../../security/authentication-and-authorization-overview.md#1-registering-a-system) |
  | `### 2. Evaluation and Build Queue Pipeline` | [workflows/evaluation-and-build-queue-pipeline.md](../../workflows/evaluation-and-build-queue-pipeline.md) |
  | `### 3. Deploying to a System` | [deployment/deployment-flow.md#automatic-deployment](../../deployment/deployment-flow.md#automatic-deployment) |
  | `### 4. Authentication`, `### 5. Authorization (RBAC)` | [security/authentication-and-authorization-overview.md](../../security/authentication-and-authorization-overview.md) |
  | `## Development Workflow` | [../../operations/local-development-workflow.md#development-workflow](../../operations/local-development-workflow.md#development-workflow) |
  | `## Configuration` | [../../operations/server-configuration-reference.md#configuration](../../operations/server-configuration-reference.md#configuration) |
  | `## Important Patterns` | [../../operations/local-development-workflow.md#important-patterns](../../operations/local-development-workflow.md#important-patterns) |
  | `## Common Tasks` | [../../operations/local-development-workflow.md#common-tasks](../../operations/local-development-workflow.md#common-tasks) |
  | `## Key Files Reference` | [../../operations/local-development-workflow.md#key-files-reference](../../operations/local-development-workflow.md#key-files-reference) |
  | `## Next Steps` | [../../overview/system-overview.md](../../overview/system-overview.md), [../cleanup-record.md](../cleanup-record.md) |
- Unmapped content: none

### `docs/eval-build-deploy-flow.md`

- Title: Commit -> Eval -> Build -> Cache -> Deploy Flow
- Purpose: Shows the commit-to-deployment flow chart.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Mermaid Flowchart` | [../../workflows/commit-eval-build-cache-deploy-flow.md](../../workflows/commit-eval-build-cache-deploy-flow.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Quick Explanation` | [../../workflows/commit-eval-build-cache-deploy-flow.md](../../workflows/commit-eval-build-cache-deploy-flow.md), [../cleanup-record.md](../cleanup-record.md) |
- Unmapped content: none

### `docs/eval-build-deploy-sequence.md`

- Title: Commit -> Eval -> Build -> Cache -> Deploy Sequence
- Purpose: Shows the commit-to-deployment sequence diagram and a reading guide.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Mermaid Sequence Diagram` | [../../workflows/commit-eval-build-cache-deploy-sequence.md](../../workflows/commit-eval-build-cache-deploy-sequence.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Reading Guide` (Key Architectural Decisions) | [../../workflows/commit-eval-build-cache-deploy-sequence.md#reading-guide](../../workflows/commit-eval-build-cache-deploy-sequence.md#reading-guide) |
- Unmapped content: none

### `docs/store-path-flow.md`

- Title: Crystal Forge Store Path Flow
- Purpose: Shows per-system store path tracking, the LIFO build queue, and agent states.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## The Complete Flow` | [../../workflows/store-path-flow.md#the-complete-flow](../../workflows/store-path-flow.md#the-complete-flow) |
  | `## Key Points for Dumb Interns` (Parallel Operations, LIFO Build Queue, System States, The Loop, Why This Design?) | [../../workflows/store-path-flow.md#key-points](../../workflows/store-path-flow.md#key-points), [../cleanup-record.md](../cleanup-record.md) |
- Unmapped content: none

### `docs/derivation-status.md`

- Title: Crystal Forge Derivation Status Flow
- Purpose: Explains derivation statuses, transitions, loops, deployment, cache push, retries, and monitoring.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [concepts/derivation-status-lifecycle.md](../../concepts/derivation-status-lifecycle.md) |
  | `## Combined Lifecycle (Sequence)` | [../../concepts/derivation-status-lifecycle.md#current-lifecycle](../../concepts/derivation-status-lifecycle.md#current-lifecycle) |
  | `## Status Table` | [../../concepts/derivation-status-lifecycle.md#derivation-statuses](../../concepts/derivation-status-lifecycle.md#derivation-statuses) |
  | `## Processing Loops` | [../../architecture/derivation-processing-loops.md](../../architecture/derivation-processing-loops.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Deployment Flow` | [../../deployment/deployment-flow.md](../../deployment/deployment-flow.md) |
  | `## Cache Push Process` | [../../caches/cache-push-process.md](../../caches/cache-push-process.md) |
  | `## Retry Logic` | [../../concepts/derivation-status-lifecycle.md#retry-rules](../../concepts/derivation-status-lifecycle.md#retry-rules) |
  | `## Terminal States` | [../../concepts/derivation-status-lifecycle.md#terminal-states](../../concepts/derivation-status-lifecycle.md#terminal-states) |
  | `## Integration Points` | [../../architecture/derivation-processing-loops.md](../../architecture/derivation-processing-loops.md), [../cleanup-record.md](../cleanup-record.md) |
  | `## Common Issues` | [../../operations/observability-and-troubleshooting.md#common-issues](../../operations/observability-and-troubleshooting.md#common-issues) |
  | `## Monitoring Recommendations` | [../../operations/observability-and-troubleshooting.md#monitoring-recommendations](../../operations/observability-and-troubleshooting.md#monitoring-recommendations) |
- Unmapped content: none

### `packages/default/WORKSPACE.md`

- Title: Crystal Forge Backend — Workspace Architecture
- Purpose: Developer documentation for the Cargo workspace crate split.
- Action: replaced (the original path holds a short pointer)
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Workspace layout`, `## Crate boundaries` | [../../architecture/backend-cargo-workspace.md#workspace-layout](../../architecture/backend-cargo-workspace.md#workspace-layout) |
  | `## Targeted Cargo checks`, `## Targeted Nix builds` | [../../architecture/backend-cargo-workspace.md#targeted-cargo-checks](../../architecture/backend-cargo-workspace.md#targeted-cargo-checks) |
  | `## Forbidden dependency boundaries` | [../../architecture/backend-cargo-workspace.md#forbidden-dependency-boundaries](../../architecture/backend-cargo-workspace.md#forbidden-dependency-boundaries) |
  | `## Timing evidence` | [../../architecture/backend-cargo-workspace.md#timing-evidence](../../architecture/backend-cargo-workspace.md#timing-evidence) |
  | `## Known follow-ups (outside this MR)` | [../../architecture/backend-cargo-workspace.md#known-follow-ups](../../architecture/backend-cargo-workspace.md#known-follow-ups) |
  | `## SQLx offline metadata` | [../../architecture/backend-cargo-workspace.md#sqlx-offline-metadata](../../architecture/backend-cargo-workspace.md#sqlx-offline-metadata) |
- Unmapped content: none

### `backlog/docs/doc-2 - Commit-Eval-Build-Cache-Deploy-Flow.md`

- Title: Commit -> Eval -> Build -> Cache -> Deploy Flow
- Purpose: Backlog.md copy of `docs/eval-build-deploy-flow.md`.
- Action: replaced
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Whole body (content identical to the canonical document) | [workflows/commit-eval-build-cache-deploy-flow.md](../../workflows/commit-eval-build-cache-deploy-flow.md) |
- Unmapped content: none. The body now holds a two-line pointer and the frontmatter is unchanged.

### `backlog/docs/doc-3 - Commit-Eval-Build-Cache-Deploy-Sequence.md`

- Title: Commit -> Eval -> Build -> Cache -> Deploy Sequence
- Purpose: Backlog.md copy of `docs/eval-build-deploy-sequence.md`, older than the canonical document.
- Action: replaced
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Whole body, identical except for reading-guide item 6 | [workflows/commit-eval-build-cache-deploy-sequence.md](../../workflows/commit-eval-build-cache-deploy-sequence.md) |
  | Reading-guide item 6 (older "Deployable" definition, unique to this copy) | [workflows/commit-eval-build-cache-deploy-sequence.md#earlier-definition-of-deployable](../../workflows/commit-eval-build-cache-deploy-sequence.md#earlier-definition-of-deployable) |
- Unmapped content: none. The body now holds a two-line pointer and the frontmatter is unchanged.

### `packages/slides/slides.md` and `packages/slides/slides/*.md`

- Title: Crystal Forge Slidev deck
- Purpose: Presentation source for the Crystal Forge product pitch.
- Action: retained (all 20 files stay at their paths; the pointer concept lists each slide with a one-line description)
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `slides.md` (cover, eight section dividers, slide includes) | [overview/product-vision-pitch-deck.md#deck-structure](../../overview/product-vision-pitch-deck.md#deck-structure) |
  | `slides/00-title.md` to `slides/18-get-involved.md` | [overview/product-vision-pitch-deck.md#slide-files](../../overview/product-vision-pitch-deck.md#slide-files) |
- Unmapped content: none
