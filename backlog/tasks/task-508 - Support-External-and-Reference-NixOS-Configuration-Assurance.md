---
id: TASK-508
title: Support External and Reference NixOS Configuration Assurance
status: To Do
assignee: []
created_date: '2026-10-31T15:50:00Z'
labels:
  - management-mode
  - assurance
  - security
  - web-ui
  - server
dependencies: []
references:
  - >-
    git:18b47eed40620508e8b5725f1e7845c02146a122 (approved management-mode UI
    design on dev)
documentation:
  - docs/design/CrystalForge/components/MgmtParts.jsx
  - docs/design/CrystalForge/data-external.js
  - docs/design/CrystalForge/data-external-scan.js
  - docs/design/CrystalForge/components/AddSystemModal.jsx
  - docs/design/CrystalForge/components/EditSystemModal.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/Systems.jsx
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/components/ConfigExplorer.jsx
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/styles.css
  - docs/knowledge/index.md
  - docs/knowledge/testing/integration-test-contract.md
priority: high
type: feature
ordinal: 559000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge only models systems that it deploys through an enrolled agent. Operators also run NixOS systems deployed by other teams or tools (including MicroVM guests on CF-managed hosts), and they maintain reference/golden NixOS configurations (workstations, VM baselines, ISO-oriented configurations). Those configurations need the same evaluation, build, CVE-scan, hardening and compliance evidence, but registering them today requires an agent key and grants CF deployment authority it must not have.

## Goal
One complete feature that lets Crystal Forge evaluate, build, scan and retain evidence for a NixOS configuration without being authorized to deploy it. Three management modes exist:
- `managed`: CF-managed system; existing deployment policies apply.
- `external`: deployed independently of CF; CF has no deployment authority.
- `reference`: configuration tracked for assurance; deployment is not applicable; it is not a machine.

Backend field name: `management_mode`. Existing systems default to `managed`. `deployment_policy` stays separate and applies only with deployment authority. `is_active` stays separate from runtime health and mode.

Fundamental invariant: configuration evidence is independent of runtime observation. A successful scan does not establish that a guest runs the scanned configuration. A reference configuration is never an active or offline machine.

## Design authority
The UI design in commit 18b47eed (docs/design/CrystalForge, notably MgmtParts.jsx, data-external*.js, AddSystemModal, EditSystemModal, SystemDetail, Systems, FlakeExplorer, ConfigExplorer, ScanningView, DashboardView, styles.css, app.jsx, Shell.jsx, EnvironmentsView, DashboardWidgetsOps) is the authoritative presentation and interaction specification. Its fixture data, simulated Refresh and mock observations are NOT a backend data model and must not be copied into production.

## Non-goals
- Generic asset management framework, separate scanning pipeline, second deployment system, VM orchestrator.
- Automatic discovery of all guests, guest-agent enrollment, hypervisor orchestration, remote command execution, VM deployment.
- Synchronous request/response refresh channel to agents (transport stays pull-based).
- Full guest attestation or advanced MicroVM monitoring.
- Image distribution service; claiming that a NixOS closure scan equals scanning a generated ISO/VM image.

## Architectural constraints
- Server owns persistence, authorization, and deployment policy; enforcement is at backend boundaries, not UI.
- Schema change via new migration; SQLx offline metadata updated.
- Compatibility with deployed agents (older agents without guest reporting keep working).
- Reuse existing evaluation, build, builder API, cache verification, and derivation-based scan infrastructure.
- Exact-CVE / POA&M authority rules preserved; configuration-only evidence never authorizes runtime claims, remediation verification, POA&M closure or deployment attestations.
- Preserve RBAC, environment membership, secret handling; audit registration, mode transitions, parent-host changes.

## Impact areas
cf-server (models, migrations, systems API, deployment policy / desired target / agent responses / rollback, evaluation targets, build eligibility, scan selection & scheduling, dashboard/fleet metrics, audit), cf-agent (optional parent-host guest observation report), web-ui (Systems list/panel/detail all tabs, Add/Edit System, Flake Explorer, Config Explorer, Scanning, Dashboard, Environments, Shell), integration checks, docs/knowledge.

## Risk
High: security-sensitive deployment authority, persistence migration, evidence integrity.

## Dependencies
Approved design commit 18b47eed on dev (present on origin/dev). No blocking backlog tasks.

## Verification plan
- cargo tests for domain/API/deployment gating/scan selection/metrics in the Nix dev shell; SQLx prepare check.
- Migration applied to an isolated database containing pre-existing systems.
- Live task-owned preview with isolated DB; browser comparison against design mock (dark/light, responsive).
- Owning integration scenarios (integration-access / integration-delivery / integration-assurance) extended per docs/knowledge/testing/integration-coverage-map.md.
- nix build .#okf-knowledge -L for docs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems persist a typed management_mode (managed|external|reference); a migration defaults every existing system to managed and invalid modes are rejected by the server
- [ ] #2 external and reference systems register without agent credentials and no fake key is generated; managed registration keeps existing agent-key requirements
- [ ] #3 Reference configurations use a descriptive identity without requiring a runtime hostname; multiple registered systems may share one NixOS configuration with unique resource identity
- [ ] #4 Create/edit/list/detail/filter APIs expose management_mode and parent-host association while preserving environment membership and RBAC
- [ ] #5 For external and reference systems the server never initiates, authorizes, queues or returns a deployment instruction across automatic policy, manual deploy, desired-target update, pending requests, rollback, rollout and agent responses
- [ ] #6 managed->external/reference transition atomically revokes deployment authority, invalidates outstanding deployment requests, disables the previous agent identity and preserves history; concurrent processes cannot issue a forbidden deployment
- [ ] #7 external/reference->managed requires explicit authorization, valid agent enrollment credentials and a valid deployment policy and never deploys on conversion; external<->reference transitions keep configuration evidence
- [ ] #8 External and reference configurations are evaluated, built (without an agent-enabled configuration) and CVE-scanned through the existing pipelines using exact configuration and derivation identity, including scheduled and manual scans, with no fabricated running store path and no duplicate builds for identical derivations
- [ ] #9 Removed or renamed configurations, failed evaluations, failed builds and failed or unavailable scans are reported explicitly while historical evidence is retained
- [ ] #10 Configuration-only scan evidence never authorizes current-runtime vulnerability claims, remediation verification, POA&M closure or deployment attestations; existing exact-CVE authority rules still pass
- [ ] #11 A CF-managed parent host can report authenticated guest observations (guest identity, unit, unit state, optional runner store path, timestamp); the server rejects unauthorized or cross-environment reports; older agents keep working; observations are stored and shown separately from scan evidence and never claimed as a configuration match without evidence
- [ ] #12 Dashboard, fleet health, deployment, environment, scan-coverage, hardening and compliance metrics count reference configurations and unobserved external systems according to documented definitions (no reference offline, no reference in managed deployment denominators, external included in scan coverage)
- [ ] #13 Registration, management-mode transitions and parent-host changes are audited and require existing authorization
- [ ] #14 Web UI implements the approved design for Add System, Edit System (mode change confirmation), Systems list filters/labels/statuses and side panel, every System Detail tab, Flake Explorer registration with preselection, Config Explorer, Scanning, Dashboard and supporting views, with real data, loading/empty/error/unavailable states and dark/light parity
- [ ] #15 Automated tests cover domain/API, deployment safety (including concurrency and bypass attempts), evaluation/build/scan eligibility, CVE/POA&M authority, host observations, fleet metrics, and owning browser integration scenarios
- [ ] #16 OKF knowledge pages document the management-mode model, deployment authority and transitions, external/reference registration, configuration vs runtime assurance, CVE evidence authority, MicroVM observation limits, scan scheduling, metric semantics, API and migration changes, and troubleshooting; nix build .#okf-knowledge passes
<!-- AC:END -->
