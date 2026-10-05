---
type: Reference
title: "Migration manifest: testing and operations"
description: Maps the test plan, check runbooks, fixture guides, sprint planning guide, check READMEs, and contribution documents to their OKF concept destinations.
tags:
  - crystal-forge
  - migration
---

# Migration manifest: testing and operations

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `docs/test_plan.md` | [testing/test-plan.md](../../testing/test-plan.md) | moved | complete |
| `docs/testing-prefetch.md` | [testing/offline-flake-prefetch.md](../../testing/offline-flake-prefetch.md) | moved | complete |
| `docs/web-ui-check.md` | [testing/web-ui-check.md](../../testing/web-ui-check.md) | moved | complete |
| `docs/fixture-seeding.md` | [testing/fixture-seeding.md](../../testing/fixture-seeding.md) | moved | complete |
| `TESTING.md` | [historical/multi-builder-api-testing-guide.md](../../historical/multi-builder-api-testing-guide.md) | moved | complete |
| `docs/ai-sprint-planning.md` | [operations/ai-sprint-planning.md](../../operations/ai-sprint-planning.md) | moved | complete |
| `docs/mock-execution-mode.md` | [operations/mock-execution-mode.md](../../operations/mock-execution-mode.md) | moved | complete |
| `checks/integration/README.md` | retained in place; see [integration section](../../testing/flake-checks.md#integration) | retained | complete |
| `checks/nixos-options-metadata/README.md` | retained in place; see [nixos-options-metadata section](../../testing/flake-checks.md#nixos-options-metadata) | retained | complete |
| `checks/oidc-auth/README.md` | retained in place; see [oidc-auth section](../../testing/flake-checks.md#oidc-auth) | retained | complete |
| `checks/oscal-export/README.md` | retained in place; see [oscal-export section](../../testing/flake-checks.md#oscal-export) | retained | complete |
| `checks/run-ui-dev-db-check/README.md` | retained in place; see [run-ui-dev-db-check section](../../testing/flake-checks.md#run-ui-dev-db-check) | retained | complete |
| `checks/server-regressions/README.md` | retained in place; see [server-regressions section](../../testing/flake-checks.md#server-regressions) | retained | complete |
| `checks/stig/README.md` | retained in place; see [stig section](../../testing/flake-checks.md#stig) | retained | complete |
| `checks/ui-screenshots/README.md` | retained in place; see [ui-screenshots section](../../testing/flake-checks.md#ui-screenshots) | retained | complete |
| `checks/web-ui/README.md` | retained in place; see [web-ui section](../../testing/flake-checks.md#web-ui) | retained | complete |
| `checks/web-ui/baselines/README.md` | retained in place; see [web-ui/baselines section](../../testing/flake-checks.md#web-uibaselines) | retained | complete |
| `checks/web-ui-reconciliation/README.md` | retained in place; see [web-ui-reconciliation section](../../testing/flake-checks.md#web-ui-reconciliation) | retained | complete |
| `checks/web-ui-test-runner/README.md` | retained in place; see [web-ui-test-runner section](../../testing/flake-checks.md#web-ui-test-runner) | retained | complete |
| `checks/xccdf-schema/README.md` | retained in place; see [xccdf-schema section](../../testing/flake-checks.md#xccdf-schema) | retained | complete |
| `packages/cf-test-suite/README.md` | retained in place; see [scenario runner section](../../testing/flake-checks.md#scenario-runner-packagescf-test-suite) | retained | complete |
| `docs/design/CrystalForge/fixtures/README.md` | retained in place; see [pointer](../../testing/design-golden-fixtures.md) | retained | complete |
| `backlog/docs/doc-1 - Task-Template.md` | retained in place; see [pointer](../../operations/backlog-process-documents.md) | retained | complete |
| `CONTRIBUTING.md` | retained in place; see [pointer](../../operations/contributing-guide.md) | retained | complete |

## Source inventory

### `docs/test_plan.md`

- Title: Crystal Forge Testing Plan
- Purpose: Defines the test levels, infrastructure, categories, scenario system, execution strategy, coverage targets, and maintenance goals for Crystal Forge.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Executive Summary` | [testing/test-plan.md#executive-summary](../../testing/test-plan.md#executive-summary) |
  | `## Testing Philosophy` | [testing/test-plan.md#testing-philosophy](../../testing/test-plan.md#testing-philosophy) |
  | `## Testing Architecture` | [testing/test-plan.md#testing-architecture](../../testing/test-plan.md#testing-architecture) |
  | `## File Structure & Organization` | [testing/test-plan.md#file-structure--organization](../../testing/test-plan.md#file-structure--organization) |
  | `## Test Categories` | [testing/test-plan.md#test-categories](../../testing/test-plan.md#test-categories) |
  | `## Test Data Management` | [testing/test-plan.md#test-data-management](../../testing/test-plan.md#test-data-management) |
  | `## Test Execution Strategy` | [testing/test-plan.md#test-execution-strategy](../../testing/test-plan.md#test-execution-strategy) |
  | `## Coverage Requirements` | [testing/test-plan.md#coverage-requirements](../../testing/test-plan.md#coverage-requirements) |
  | `## Test Documentation` | [testing/test-plan.md#test-documentation](../../testing/test-plan.md#test-documentation) |
  | `## Performance Testing` | [testing/test-plan.md#performance-testing](../../testing/test-plan.md#performance-testing) |
  | `## Security Testing` | [testing/test-plan.md#security-testing](../../testing/test-plan.md#security-testing) |
  | `## Test Output & Reporting` | [testing/test-plan.md#test-output--reporting](../../testing/test-plan.md#test-output--reporting) |
  | `## Test Maintenance` | [testing/test-plan.md#test-maintenance](../../testing/test-plan.md#test-maintenance) |
  | `## Development Workflow` | [testing/test-plan.md#development-workflow](../../testing/test-plan.md#development-workflow) |
  | `## Questions for Clarification` | [testing/test-plan.md#questions-for-clarification](../../testing/test-plan.md#questions-for-clarification) |
  | `## Success Metrics` | [testing/test-plan.md#success-metrics](../../testing/test-plan.md#success-metrics) |
  | `## Next Steps` | [testing/test-plan.md#next-steps](../../testing/test-plan.md#next-steps) |
- Unmapped content: none. The migration added `Status:` notes for stale paths, markers, and CI description.

### `docs/testing-prefetch.md`

- Title: none (the document starts at `## The Problem Being Solved`); the concept title is "Offline flake input prefetching for NixOS VM tests".
- Purpose: Explains how NixOS VM tests prefetch flake inputs and redirect the flake registry to local paths.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## The Problem Being Solved` | [testing/offline-flake-prefetch.md#the-problem-being-solved](../../testing/offline-flake-prefetch.md#the-problem-being-solved) |
  | `## Step-by-Step Breakdown` | [testing/offline-flake-prefetch.md#step-by-step-breakdown](../../testing/offline-flake-prefetch.md#step-by-step-breakdown) |
  | `## Why This Approach?` | [testing/offline-flake-prefetch.md#why-this-approach](../../testing/offline-flake-prefetch.md#why-this-approach) |
- Unmapped content: none. The migration added an H1 and a `Status:` note.

### `docs/web-ui-check.md`

- Title: Web UI Check — Runbook
- Purpose: Runbook for the `web-ui` Nix check and the host-side `web-ui-test` loop.
- Action: moved (kept whole because the sections are interdependent steps of one runbook)
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [testing/web-ui-check.md](../../testing/web-ui-check.md) |
  | `## Layout` | [testing/web-ui-check.md#layout](../../testing/web-ui-check.md#layout) |
  | `## Running locally` | [testing/web-ui-check.md#running-locally](../../testing/web-ui-check.md#running-locally) |
  | `## Phases and gates` | [testing/web-ui-check.md#phases-and-gates](../../testing/web-ui-check.md#phases-and-gates) |
  | `## Visual baselines` | [testing/web-ui-check.md#visual-baselines](../../testing/web-ui-check.md#visual-baselines) |
  | `## Design parity evidence` | [testing/web-ui-check.md#design-parity-evidence](../../testing/web-ui-check.md#design-parity-evidence) |
  | `## Adding a route/state to coverage` | [testing/web-ui-check.md#adding-a-routestate-to-coverage](../../testing/web-ui-check.md#adding-a-routestate-to-coverage) |
  | `## Debugging failures` | [testing/web-ui-check.md#debugging-failures](../../testing/web-ui-check.md#debugging-failures) |
  | `## CI integration` | [testing/web-ui-check.md#ci-integration](../../testing/web-ui-check.md#ci-integration) |
  | `## Known issues` | [testing/web-ui-check.md#known-issues](../../testing/web-ui-check.md#known-issues) |
- Unmapped content: none. The migration added `Status:` notes for `devStackWorkflows`, the `flake-check: [web-ui]` job name, and the known-issues tasks.

### `docs/fixture-seeding.md`

- Title: Fixture Seeding — Developer Guide
- Purpose: Explains fixture mode, the seeder, and how to extend it.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [testing/fixture-seeding.md](../../testing/fixture-seeding.md) |
  | `## Quick start` | [testing/fixture-seeding.md#quick-start](../../testing/fixture-seeding.md#quick-start) |
  | `## How the seeding works` | [testing/fixture-seeding.md#how-the-seeding-works](../../testing/fixture-seeding.md#how-the-seeding-works) |
  | `## What is seeded vs not yet implemented` | [testing/fixture-seeding.md#what-is-seeded-vs-not-yet-implemented](../../testing/fixture-seeding.md#what-is-seeded-vs-not-yet-implemented) |
  | `## Adding a new field to the seeder` | [testing/fixture-seeding.md#adding-a-new-field-to-the-seeder](../../testing/fixture-seeding.md#adding-a-new-field-to-the-seeder) |
  | `## Adding a new route to the screenshot check` | [testing/fixture-seeding.md#adding-a-new-route-to-the-screenshot-check](../../testing/fixture-seeding.md#adding-a-new-route-to-the-screenshot-check) |
  | `## FAQ` | [testing/fixture-seeding.md#faq](../../testing/fixture-seeding.md#faq) |
- Unmapped content: none. The migration added `Status:` notes for the screenshot check description, seeder path, unseeded sections, and route array.

### `TESTING.md`

- Title: TASK-140 Testing Guide
- Purpose: Manual UI, API, backend, and database test checklist for the TASK-140 Multi-Builder API feature.
- Action: moved (classified historical: task-bound, and its known issues no longer match the code)
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Overview` | [historical/multi-builder-api-testing-guide.md#overview](../../historical/multi-builder-api-testing-guide.md#overview) |
  | `## Prerequisites` | [historical/multi-builder-api-testing-guide.md#prerequisites](../../historical/multi-builder-api-testing-guide.md#prerequisites) |
  | `## Quick Start` | [historical/multi-builder-api-testing-guide.md#quick-start](../../historical/multi-builder-api-testing-guide.md#quick-start) |
  | `## UI Testing Checklist` | [historical/multi-builder-api-testing-guide.md#ui-testing-checklist](../../historical/multi-builder-api-testing-guide.md#ui-testing-checklist) |
  | `## API Testing` | [historical/multi-builder-api-testing-guide.md#api-testing](../../historical/multi-builder-api-testing-guide.md#api-testing) |
  | `## Backend Testing` | [historical/multi-builder-api-testing-guide.md#backend-testing](../../historical/multi-builder-api-testing-guide.md#backend-testing) |
  | `## Database Verification` | [historical/multi-builder-api-testing-guide.md#database-verification](../../historical/multi-builder-api-testing-guide.md#database-verification) |
  | `## Known Issues / Limitations` | [historical/multi-builder-api-testing-guide.md#known-issues--limitations](../../historical/multi-builder-api-testing-guide.md#known-issues--limitations) |
  | `## Success Criteria` | [historical/multi-builder-api-testing-guide.md#success-criteria](../../historical/multi-builder-api-testing-guide.md#success-criteria) |
  | `## Test Results` | [historical/multi-builder-api-testing-guide.md#test-results](../../historical/multi-builder-api-testing-guide.md#test-results) |
  | `## Feedback` | [historical/multi-builder-api-testing-guide.md#feedback](../../historical/multi-builder-api-testing-guide.md#feedback) |
- Unmapped content: none. The migration added a `Status:` note that lists differences from the code.

### `docs/ai-sprint-planning.md`

- Title: AI Sprint Planning & Backlog Grooming Guide
- Purpose: Defines the compressed AI-executed sprint process and the backlog grooming prompt.
- Action: moved (kept whole because the sections form one process)
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Title and introduction | [operations/ai-sprint-planning.md](../../operations/ai-sprint-planning.md) |
  | `# Sprint Model` | [operations/ai-sprint-planning.md#sprint-model](../../operations/ai-sprint-planning.md#sprint-model) |
  | `# Roles` | [operations/ai-sprint-planning.md#roles](../../operations/ai-sprint-planning.md#roles) |
  | `# Sprint Lifecycle` | [operations/ai-sprint-planning.md#sprint-lifecycle](../../operations/ai-sprint-planning.md#sprint-lifecycle) |
  | `# Backlog Grooming Prompt` | [operations/ai-sprint-planning.md#backlog-grooming-prompt](../../operations/ai-sprint-planning.md#backlog-grooming-prompt) |
  | `# Task Format Standard` | [operations/ai-sprint-planning.md#task-format-standard](../../operations/ai-sprint-planning.md#task-format-standard) |
  | `# AI-Safe Task Design Principles` | [operations/ai-sprint-planning.md#ai-safe-task-design-principles](../../operations/ai-sprint-planning.md#ai-safe-task-design-principles) |
  | `# Sprint Definition of Done` | [operations/ai-sprint-planning.md#sprint-definition-of-done](../../operations/ai-sprint-planning.md#sprint-definition-of-done) |
  | `# Risk Control Guidelines` | [operations/ai-sprint-planning.md#risk-control-guidelines](../../operations/ai-sprint-planning.md#risk-control-guidelines) |
  | `# Explicit Out-of-Scope Section` | [operations/ai-sprint-planning.md#explicit-out-of-scope-section](../../operations/ai-sprint-planning.md#explicit-out-of-scope-section) |
  | `# Recommended Documentation Pattern` | [operations/ai-sprint-planning.md#recommended-documentation-pattern](../../operations/ai-sprint-planning.md#recommended-documentation-pattern) |
  | `# Guiding Principle` | [operations/ai-sprint-planning.md#guiding-principle](../../operations/ai-sprint-planning.md#guiding-principle) |
  | `# Summary` | [operations/ai-sprint-planning.md#summary](../../operations/ai-sprint-planning.md#summary) |
- Unmapped content: none. The migration demoted source H1 headings to H2 so the concept has one H1, and added a `Status:` note.

### `docs/mock-execution-mode.md`

- Title: Mock Execution Mode (Dev Only)
- Purpose: Describes the deterministic dev-only mock execution mode.
- Action: moved
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Safety Model` | [operations/mock-execution-mode.md#safety-model](../../operations/mock-execution-mode.md#safety-model) |
  | `## Configuration` | [operations/mock-execution-mode.md#configuration](../../operations/mock-execution-mode.md#configuration) |
  | `## What Mock Mode Simulates` | [operations/mock-execution-mode.md#what-mock-mode-simulates](../../operations/mock-execution-mode.md#what-mock-mode-simulates) |
  | `## UI Indicator` | [operations/mock-execution-mode.md#ui-indicator](../../operations/mock-execution-mode.md#ui-indicator) |
  | `## Intended Use` | [operations/mock-execution-mode.md#intended-use](../../operations/mock-execution-mode.md#intended-use) |
- Unmapped content: none. The migration added `Status:` notes for the safety checks and the UI badge.

### Check READMEs (13 files under `checks/` plus `checks/web-ui/baselines/README.md`)

All READMEs are retained in place. The catalog concept
[testing/flake-checks.md](../../testing/flake-checks.md) has one section per
check, with what the check verifies, how to run it, and a link to the README.
Each README has the same section outline (`## What it verifies`,
`## Why it is a separate check`, `## Run it`, `## Out of scope`, `## CI`, and
optionally `## Related files`), which the catalog summarizes per check.

- `checks/integration/README.md`: Title "Integration Check". Action: retained. Destination: [integration](../../testing/flake-checks.md#integration).
- `checks/nixos-options-metadata/README.md`: Title "NixOS Options Metadata Check". Action: retained. Destination: [nixos-options-metadata](../../testing/flake-checks.md#nixos-options-metadata).
- `checks/oidc-auth/README.md`: Title "OIDC Authentication Check". Action: retained. Destination: [oidc-auth](../../testing/flake-checks.md#oidc-auth).
- `checks/oscal-export/README.md`: Title "OSCAL Export Check". Action: retained. Destination: [oscal-export](../../testing/flake-checks.md#oscal-export).
- `checks/run-ui-dev-db-check/README.md`: Title "run-ui-dev Database Behavior Check". Action: retained. Destination: [run-ui-dev-db-check](../../testing/flake-checks.md#run-ui-dev-db-check).
- `checks/server-regressions/README.md`: Title "Server Regressions Check". Action: retained. Destination: [server-regressions](../../testing/flake-checks.md#server-regressions).
- `checks/stig/README.md`: Title "STIG Module Unit Tests". Action: retained. Destination: [stig](../../testing/flake-checks.md#stig).
- `checks/ui-screenshots/README.md`: Title "UI Screenshots Check". Action: retained. Destination: [ui-screenshots](../../testing/flake-checks.md#ui-screenshots).
- `checks/web-ui/README.md`: Title "Web UI Check". Action: retained. Destination: [web-ui](../../testing/flake-checks.md#web-ui), with the procedure in [testing/web-ui-check.md](../../testing/web-ui-check.md).
- `checks/web-ui/baselines/README.md`: Title "Web UI Baselines". Action: retained. Destination: [web-ui/baselines](../../testing/flake-checks.md#web-uibaselines).
- `checks/web-ui-reconciliation/README.md`: Title "Web UI Reconciliation Check". Action: retained. Destination: [web-ui-reconciliation](../../testing/flake-checks.md#web-ui-reconciliation).
- `checks/web-ui-test-runner/README.md`: Title "Web UI Test Runner Check". Action: retained. Destination: [web-ui-test-runner](../../testing/flake-checks.md#web-ui-test-runner).
- `checks/xccdf-schema/README.md`: Title "XCCDF Schema Check". Action: retained. Destination: [xccdf-schema](../../testing/flake-checks.md#xccdf-schema).
- Unmapped content: none. The catalog adds `Status:` notes where a README differs from the check's `default.nix` or `.gitlab-ci.yml`.

### `packages/cf-test-suite/README.md`

- Title: cf-scenarios (Crystal Forge test data)
- Purpose: Shows how to run preset scenarios that populate a database.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Help`, `## Quick start`, `## Examples` | [testing/flake-checks.md#scenario-runner-packagescf-test-suite](../../testing/flake-checks.md#scenario-runner-packagescf-test-suite) |
- Unmapped content: none. The catalog records the current `cf-test-suite` command, scenario names, options, and connection variables.

### `docs/design/CrystalForge/fixtures/README.md`

- Title: Crystal Forge — golden fixtures
- Purpose: Defines the deterministic `crystal-forge.fixtures.json` snapshot shared by the design example and the Dioxus port.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Opening summary, `## The design example reads this file too`, `## Suggested CI use`, `## Regenerating`, `## Top-level shape`, `## Entity fields (the ones views assert on)`, `## Notes` | [testing/design-golden-fixtures.md](../../testing/design-golden-fixtures.md) |
- Unmapped content: none. The pointer summarizes each section, and the retained file stays authoritative.

### `backlog/docs/doc-1 - Task-Template.md`

- Title: Task Template
- Purpose: Template of sections for a Backlog.md task description.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `# Title` through `# Follow-Up Work` (13 sections) | [operations/backlog-process-documents.md](../../operations/backlog-process-documents.md) |
- Unmapped content: none. The pointer has a table of all 13 sections.

### `CONTRIBUTING.md`

- Title: Contributing to Crystal Forge
- Purpose: Contribution policy, process, testing requirements, and style expectations.
- Action: retained
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Project Status & Goals` through `## License` | [operations/contributing-guide.md](../../operations/contributing-guide.md) |
- Unmapped content: none. The pointer has one table row for each section and `Status:` notes for stale commands.
