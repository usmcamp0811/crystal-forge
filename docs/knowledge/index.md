---
okf_version: "0.2"
---

# Crystal Forge Knowledge Bundle

The canonical knowledge corpus for Crystal Forge, following the Open Knowledge Format v0.2.

## Overview
* [overview/](overview/index.md) - Product brief, system context, system overview, and project introduction. Open first for orientation on what Crystal Forge is and the problems it solves.

## API
* [api/](api/index.md) - REST API endpoints, authentication, error codes, and WebSocket streaming. Open when implementing or calling server APIs.

## Architecture
* [architecture/](architecture/index.md) - System components, event-driven queues, derivation processing loops, and backend workspace structure. Open when reasoning about system boundaries or data flows.

## Builders
* [builders/](builders/index.md) - Remote build worker architecture, job scheduling, session management, and trust boundaries. Open when working on builder-server protocols or build infrastructure.

## Caches
* [caches/](caches/index.md) - Artifact cache (Attic/S3), cache push workflows, and cache invalidation. Open when debugging cache misses or configuring cache storage.

## Compliance
* [compliance/](compliance/index.md) - STIG modules, CVE/POA&M evidence continuity, and compliance UI. Open when auditing security posture or implementing compliance features.

## Components
* [components/](components/index.md) - Core component model, crate boundaries, and dependency direction. Open when adding crates or refactoring crate graph.

## Concepts
* [concepts/](concepts/index.md) - Derivation status lifecycle, deployment flow, and domain models. Open for reference on domain entities and state machines.

## CVE
* [cves/](cves/index.md) - Fleet CVE triage, current-CVE resolver, and POA&M lifecycle. Open when triaging vulnerabilities or managing remediation.

## Data Model
* [data-model/](data-model/index.md) - SQL view definitions (derivations, builds, systems, CVE evidence), entity relationships, and query patterns. Open when writing or reviewing database queries.

## Decisions
* [decisions/](decisions/index.md) - Architecture Decision Records (ADRs) for major design choices. Open when proposing or reviewing significant changes.

## Deployment
* [deployment/](deployment/index.md) - Deployment policies, agent heartbeat/state persistence, and rollout automation. Open when configuring or troubleshooting deployments.

## Evaluation
* [evaluation/](evaluation/index.md) - Fast evaluation loop, flake snapshots, config inspector, and evaluation-flake isolation. Open when optimizing evaluation performance or debugging eval failures.

## Historical
* [historical/](historical/index.md) - Superseded designs and post-task follow-ups preserved for context. Open only for historical research; not current guidance.

## Meta
* [meta/](meta/index.md) - Bundle conventions, cleanup record, migration manifests, and OKF migration log. Open when contributing to the knowledge bundle itself.

## Operations
* [operations/](operations/index.md) - Adding API endpoints, NixOS module config, authentication modes, local development workflow, and onboarding. Open when performing operational tasks.

## POA&M
* [poam/](poam/index.md) - Plan of Action and Milestones for CVE remediation tracking. Open when creating or managing POA&M entries.

## References
* [references/](references/index.md) - External specifications, RFCs, and standards referenced by the bundle. Open for authoritative external context.

## Security
* [security/](security/index.md) - OIDC role mapping, authentication/authorization overview, session cookies, CSRF, and builder/API auth. Open when implementing or auditing auth flows.

## Testing
* [testing/](testing/index.md) - Test plan, fixture seeding, web UI checks, flake-check catalog, and coverage requirements. Open when writing tests or extending CI.

## UI
* [ui/](ui/index.md) - Design system (theming, typography, components), view specifications (dashboard, systems, flakes, builds, CVEs), and Figma extraction workflow. Open when designing or implementing web UI views.

## Workflows
* [workflows/](workflows/index.md) - Commit→eval→build→cache→deploy sequence, store path flow, and build invalidation graph. Open when tracing end-to-end processes or debugging workflow failures.
