---
type: Reference
title: "Migration manifest: API, views, and onboarding"
description: Maps the backend API specification, frontend views specification, onboarding guide, and TASK-215 frontend TODO to their OKF concept destinations.
tags:
  - crystal-forge
  - migration
---

# Migration manifest: API, views, and onboarding

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/specs/02-backend-api.md` | [api/api-overview-errors-and-streaming.md](../../api/api-overview-errors-and-streaming.md), [security/api-authentication-and-authorization.md](../../security/api-authentication-and-authorization.md), [api/systems-api.md](../../api/systems-api.md), [api/flakes-and-evaluation-snapshot-api.md](../../api/flakes-and-evaluation-snapshot-api.md), [api/builders-queues-environments-dashboard-admin-api.md](../../api/builders-queues-environments-dashboard-admin-api.md), [api/cve-scan-operations-api.md](../../api/cve-scan-operations-api.md), [api/fleet-cve-triage-api.md](../../api/fleet-cve-triage-api.md), [api/agent-and-cache-api.md](../../api/agent-and-cache-api.md), [operations/adding-a-backend-api-endpoint.md](../../operations/adding-a-backend-api-endpoint.md) | split | complete |
| `docs/specs/01-frontend-views.md` | [ui/frontend-navigation-and-shared-patterns.md](../../ui/frontend-navigation-and-shared-patterns.md), [ui/dashboard-and-systems-views.md](../../ui/dashboard-and-systems-views.md), [ui/flakes-environments-builds-and-evaluations-views.md](../../ui/flakes-environments-builds-and-evaluations-views.md), [ui/admin-console-and-login-views.md](../../ui/admin-console-and-login-views.md) | split | complete |
| `docs/onboarding-guide.md` | [operations/onboarding-first-time-setup-prerequisites.md](../../operations/onboarding-first-time-setup-prerequisites.md), [ui/guided-setup-coach.md](../../ui/guided-setup-coach.md), [operations/onboarding-step-1-environment.md](../../operations/onboarding-step-1-environment.md), [operations/onboarding-step-2-flake.md](../../operations/onboarding-step-2-flake.md), [operations/onboarding-step-3-builder.md](../../operations/onboarding-step-3-builder.md), [operations/onboarding-step-4-cache-destinations.md](../../operations/onboarding-step-4-cache-destinations.md), [operations/onboarding-step-5-system.md](../../operations/onboarding-step-5-system.md), [operations/onboarding-step-6-agent-deployment.md](../../operations/onboarding-step-6-agent-deployment.md), [operations/onboarding-troubleshooting.md](../../operations/onboarding-troubleshooting.md), [operations/onboarding-after-setup-and-next-steps.md](../../operations/onboarding-after-setup-and-next-steps.md) | split | complete |
| `FRONTEND_TODO.md` | [historical/task-215-frontend-remaining-work.md](../../historical/task-215-frontend-remaining-work.md) | moved | complete |

## Source inventory

### `docs/specs/02-backend-api.md`

- Title: Backend API Specification
- Purpose: Describes Crystal Forge's HTTP API, its endpoints, and how to add one.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [api/api-overview-errors-and-streaming.md](../../api/api-overview-errors-and-streaming.md) |
  | `## API Overview` | [../../api/api-overview-errors-and-streaming.md#api-overview](../../api/api-overview-errors-and-streaming.md#api-overview) |
  | `## Authentication` | [../../security/api-authentication-and-authorization.md#authentication](../../security/api-authentication-and-authorization.md#authentication) |
  | `## Authorization (RBAC)` | [../../security/api-authentication-and-authorization.md#authorization-rbac](../../security/api-authentication-and-authorization.md#authorization-rbac) |
  | `## Systems API` | [../../api/systems-api.md#endpoints](../../api/systems-api.md#endpoints) |
  | `## Flakes API` | [../../api/flakes-and-evaluation-snapshot-api.md#flakes-api](../../api/flakes-and-evaluation-snapshot-api.md#flakes-api) |
  | `## Evaluation and Flake Snapshot API` | [../../api/flakes-and-evaluation-snapshot-api.md#evaluation-and-flake-snapshot-api](../../api/flakes-and-evaluation-snapshot-api.md#evaluation-and-flake-snapshot-api) |
  | `## Builders API` | [../../api/builders-queues-environments-dashboard-admin-api.md#builders-api](../../api/builders-queues-environments-dashboard-admin-api.md#builders-api) |
  | `## Evaluation Queue API` | [../../api/builders-queues-environments-dashboard-admin-api.md#evaluation-queue-api](../../api/builders-queues-environments-dashboard-admin-api.md#evaluation-queue-api) |
  | `## Build Queue API` | [../../api/builders-queues-environments-dashboard-admin-api.md#build-queue-api](../../api/builders-queues-environments-dashboard-admin-api.md#build-queue-api) |
  | `## Environments API` | [../../api/builders-queues-environments-dashboard-admin-api.md#environments-api](../../api/builders-queues-environments-dashboard-admin-api.md#environments-api) |
  | `## Dashboard API` | [../../api/builders-queues-environments-dashboard-admin-api.md#dashboard-api](../../api/builders-queues-environments-dashboard-admin-api.md#dashboard-api) |
  | `## Admin API` | [../../api/builders-queues-environments-dashboard-admin-api.md#admin-api](../../api/builders-queues-environments-dashboard-admin-api.md#admin-api) |
  | `## CVE Scan Operations` | [../../api/cve-scan-operations-api.md#post-build-recovery-and-scanning-statistics](../../api/cve-scan-operations-api.md#post-build-recovery-and-scanning-statistics) |
  | `## Fleet CVE Triage` | [../../api/fleet-cve-triage-api.md#system-cve-inventory](../../api/fleet-cve-triage-api.md#system-cve-inventory) |
  | `## Agent API (Machine Auth)` | [../../api/agent-and-cache-api.md#agent-api-machine-authentication](../../api/agent-and-cache-api.md#agent-api-machine-authentication) |
  | `## Cache API (Future - TASK-141)` | [../../api/agent-and-cache-api.md#cache-administration-api](../../api/agent-and-cache-api.md#cache-administration-api) |
  | `## Common Error Codes` | [../../api/api-overview-errors-and-streaming.md#common-error-codes](../../api/api-overview-errors-and-streaming.md#common-error-codes) |
  | `## Adding a New API Endpoint` | [../../operations/adding-a-backend-api-endpoint.md#where-things-live](../../operations/adding-a-backend-api-endpoint.md#where-things-live) |
  | `## File Organization` | [../../ui/frontend-navigation-and-shared-patterns.md#file-organization](../../ui/frontend-navigation-and-shared-patterns.md#file-organization) |
  | `## WebSocket Streaming` | [../../api/api-overview-errors-and-streaming.md#websocket-streaming](../../api/api-overview-errors-and-streaming.md#websocket-streaming) |
  | `## Summary` | [../../api/api-overview-errors-and-streaming.md#summary](../../api/api-overview-errors-and-streaming.md#summary) |
- Unmapped content: none. Horizontal rules between sections were not carried over. Sections were reordered only at concept level (Common Error Codes, WebSocket Streaming, and Summary follow API Overview).

### `docs/specs/01-frontend-views.md`

- Title: Frontend Views Specification
- Purpose: Describes each web UI view, its data, interactions, and files.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title, introduction, related documentation | [ui/frontend-navigation-and-shared-patterns.md](../../ui/frontend-navigation-and-shared-patterns.md) |
  | `## Navigation Structure` | [../../ui/frontend-navigation-and-shared-patterns.md#navigation-structure](../../ui/frontend-navigation-and-shared-patterns.md#navigation-structure) |
  | `## Dashboard (`/`)` | [../../ui/dashboard-and-systems-views.md#dashboard-](../../ui/dashboard-and-systems-views.md#dashboard-) |
  | `## Systems List (`/systems`)` | [../../ui/dashboard-and-systems-views.md#systems-list-systems](../../ui/dashboard-and-systems-views.md#systems-list-systems) |
  | `## System Detail (`/systems/:id`)` | [../../ui/dashboard-and-systems-views.md#system-detail-systemsid](../../ui/dashboard-and-systems-views.md#system-detail-systemsid) |
  | `## Flakes List (`/flakes`)` | [../../ui/flakes-environments-builds-and-evaluations-views.md#flakes-list-flakes](../../ui/flakes-environments-builds-and-evaluations-views.md#flakes-list-flakes) |
  | `## Environments List (`/environments`)` | [../../ui/flakes-environments-builds-and-evaluations-views.md#environments-list-environments](../../ui/flakes-environments-builds-and-evaluations-views.md#environments-list-environments) |
  | `## Builds Queue (`/builds`)` | [../../ui/flakes-environments-builds-and-evaluations-views.md#builds-queue-builds](../../ui/flakes-environments-builds-and-evaluations-views.md#builds-queue-builds) |
  | `## Admin Console (`/admin`)` | [../../ui/admin-console-and-login-views.md#admin-console-admin](../../ui/admin-console-and-login-views.md#admin-console-admin) |
  | `## Login Views` | [../../ui/admin-console-and-login-views.md#login-views](../../ui/admin-console-and-login-views.md#login-views) |
  | `## Common UI Components` | [../../ui/frontend-navigation-and-shared-patterns.md#common-ui-components](../../ui/frontend-navigation-and-shared-patterns.md#common-ui-components) |
  | `## Responsive Behavior` | [../../ui/frontend-navigation-and-shared-patterns.md#responsive-behavior](../../ui/frontend-navigation-and-shared-patterns.md#responsive-behavior) |
  | `## File Organization` | [../../ui/frontend-navigation-and-shared-patterns.md#file-organization](../../ui/frontend-navigation-and-shared-patterns.md#file-organization) |
  
  | `## Adding a New View` | [../../ui/frontend-navigation-and-shared-patterns.md#adding-a-new-view](../../ui/frontend-navigation-and-shared-patterns.md#adding-a-new-view) |
  | `## Evaluations (`/evaluations`)` | [../../ui/flakes-environments-builds-and-evaluations-views.md#evaluations-evaluations](../../ui/flakes-environments-builds-and-evaluations-views.md#evaluations-evaluations) |
  | `## Summary Table` | [../../ui/frontend-navigation-and-shared-patterns.md#summary-table](../../ui/frontend-navigation-and-shared-patterns.md#summary-table) |
- Unmapped content: none.

### `docs/onboarding-guide.md`

- Title: Crystal Forge Onboarding Guide
- Purpose: Guides an administrator through first-time setup using the guided onboarding coach.
- Action: split
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title, `## Introduction` (What This Guide Covers, Prerequisites, Overview of the Setup Track) | [operations/onboarding-first-time-setup-prerequisites.md#what-this-guide-covers](../../operations/onboarding-first-time-setup-prerequisites.md#what-this-guide-covers) |
  | `## Introduction` | [operations/onboarding-first-time-setup-prerequisites.md#what-this-guide-covers](../../operations/onboarding-first-time-setup-prerequisites.md#what-this-guide-covers) |
  | `## Before You Begin` | [operations/onboarding-first-time-setup-prerequisites.md#before-you-begin](../../operations/onboarding-first-time-setup-prerequisites.md#before-you-begin) |
  | `## The Guided Setup Coach` | [ui/guided-setup-coach.md#the-guided-setup-coach](../../ui/guided-setup-coach.md#the-guided-setup-coach) |
  | `## POA&M Dashboard and Notifications` | [ui/guided-setup-coach.md#poam-dashboard-and-notifications](../../ui/guided-setup-coach.md#poam-dashboard-and-notifications) |
  | `## Step 1: Create Environment` | [operations/onboarding-step-1-environment.md#why-environments-matter](../../operations/onboarding-step-1-environment.md#why-environments-matter), [operations/onboarding-step-1-environment.md#create-the-environment](../../operations/onboarding-step-1-environment.md#create-the-environment) |
  | `## Step 2: Add Flake` | [operations/onboarding-step-2-flake.md#why-flakes-matter](../../operations/onboarding-step-2-flake.md#why-flakes-matter), [operations/onboarding-step-2-flake.md#guided-tour-add-flake-form](../../operations/onboarding-step-2-flake.md#guided-tour-add-flake-form) |
  | `## Step 3: Register Builder` | [operations/onboarding-step-3-builder.md#why-builders-matter](../../operations/onboarding-step-3-builder.md#why-builders-matter), [operations/onboarding-step-3-builder.md#guided-tour-add-builder-form](../../operations/onboarding-step-3-builder.md#guided-tour-add-builder-form), [operations/onboarding-step-3-builder.md#enabling-the-builder-in-nixos-config](../../operations/onboarding-step-3-builder.md#enabling-the-builder-in-nixos-config) |
  | `## Step 4: Configure Cache` | [operations/onboarding-step-4-cache-destinations.md#why-cache-destinations-matter](../../operations/onboarding-step-4-cache-destinations.md#why-cache-destinations-matter), [operations/onboarding-step-4-cache-destinations.md#guided-tour-add-cache-destination-form](../../operations/onboarding-step-4-cache-destinations.md#guided-tour-add-cache-destination-form) |
  | `## Step 5: Register System` | [operations/onboarding-step-5-system.md#why-systems-matter](../../operations/onboarding-step-5-system.md#why-systems-matter), [operations/onboarding-step-5-system.md#guided-tour-add-system-form](../../operations/onboarding-step-5-system.md#guided-tour-add-system-form) |
  | `## Step 6: Deploy Agent` (including `### Onboarding Complete!`) | [operations/onboarding-step-6-agent-deployment.md#enabling-the-agent-in-nixos-config](../../operations/onboarding-step-6-agent-deployment.md#enabling-the-agent-in-nixos-config), [operations/onboarding-step-6-agent-deployment.md#apply-and-rebuild-the-target-system](../../operations/onboarding-step-6-agent-deployment.md#apply-and-rebuild-the-target-system), [operations/onboarding-step-6-agent-deployment.md#onboarding-complete](../../operations/onboarding-step-6-agent-deployment.md#onboarding-complete) |
  | `### Steps 7–9: Policies, compliance bundles and POA&Ms` (inside Step 6) | [ui/guided-setup-coach.md#steps-79-policies-compliance-bundles-and-poams](../../ui/guided-setup-coach.md#steps-79-policies-compliance-bundles-and-poams) |
  | `## Security Workflows track` | [ui/guided-setup-coach.md#security-workflows-track](../../ui/guided-setup-coach.md#security-workflows-track) |
  | `## After Onboarding` | [operations/onboarding-after-setup-and-next-steps.md#after-onboarding](../../operations/onboarding-after-setup-and-next-steps.md#after-onboarding) |
  | `## Troubleshooting` | [operations/onboarding-troubleshooting.md#troubleshooting](../../operations/onboarding-troubleshooting.md#troubleshooting) |
  | `## Next Steps` | [operations/onboarding-after-setup-and-next-steps.md#next-steps](../../operations/onboarding-after-setup-and-next-steps.md#next-steps) |
- Unmapped content: none. Image links keep their original `./screenshots/` targets for the lead's link remapping.

### `FRONTEND_TODO.md`

- Title: Frontend Work Remaining for TASK-215
- Purpose: Lists the frontend phases 5-7 that remained after the TASK-215 backend caching work.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Phase 5: Fix Evaluation Status Chip` | [../../historical/task-215-frontend-remaining-work.md#phase-5-fix-evaluation-status-chip](../../historical/task-215-frontend-remaining-work.md#phase-5-fix-evaluation-status-chip) |
  | `## Phase 6: Fix System Status Chip Theming` | [../../historical/task-215-frontend-remaining-work.md#phase-6-fix-system-status-chip-theming](../../historical/task-215-frontend-remaining-work.md#phase-6-fix-system-status-chip-theming) |
  | `## Phase 7: Browser Timezone Display` | [../../historical/task-215-frontend-remaining-work.md#phase-7-browser-timezone-display](../../historical/task-215-frontend-remaining-work.md#phase-7-browser-timezone-display) |
  | `## Testing After Frontend Changes` | [../../historical/task-215-frontend-remaining-work.md#testing-after-frontend-changes](../../historical/task-215-frontend-remaining-work.md#testing-after-frontend-changes) |
  | `## Notes` | [../../historical/task-215-frontend-remaining-work.md#notes](../../historical/task-215-frontend-remaining-work.md#notes) |
- Unmapped content: none. A `Status` note was added after the introduction.
