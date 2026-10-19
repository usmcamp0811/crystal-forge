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
| `docs/mock-execution-mode.md` | [operations/mock-execution-mode.md](../../operations/mock-execution-mode.md) | moved | complete |
| `checks/integration/README.md` | historical scope in [integration section](../../testing/flake-checks.md#integration); source retired by TASK-490.24 | merged | complete |
| `checks/integration-access/README.md` | new TASK-490.1 runbook retained beside the native check; see [coverage map](../../testing/integration-coverage-map.md) | retained | complete |
| `checks/integration-delivery/README.md` | behavior-oriented source/build lifecycle runbook retained beside its native owner; see [coverage map](../../testing/integration-coverage-map.md) | retained | complete |
| `checks/integration-assurance/fixtures/xccdf/README.md` | retained fixture provenance for planned Assurance import coverage; [retirement map](../../testing/integration-coverage-map.md#legacy-retirement-map) | retained | complete |
| `checks/builder-evaluator-packaging/README.md` | relocated source cataloged in [builder-evaluator-packaging section](../../testing/flake-checks.md#builder-evaluator-packaging) | merged | complete |
| `checks/niks3-cache/README.md` | historical scope in [niks3-cache section](../../testing/flake-checks.md#niks3-cache); source retired by TASK-490.24 | merged | complete |
| `checks/niks3-write-probe/README.md` | source retired by TASK-490.24; [retirement disposition](../../testing/integration-coverage-map.md#legacy-retirement-map) | merged | complete |
| `checks/niks3-ca-bundle/README.md` | relocated source cataloged in [CA compatibility operator contract](../../caches/niks3-ca-bundle-compatibility.md) | merged | complete |
| `checks/attic-metrics/README.md` | source retired by TASK-490.24; [retirement disposition](../../testing/integration-coverage-map.md#legacy-retirement-map) and [storage observation contract](../../caches/niks3-cache.md#response-only-storage-observations) | merged | complete |
| `checks/attic-publication/README.md` | source retired by TASK-490.24; [retirement disposition](../../testing/integration-coverage-map.md#legacy-retirement-map) | merged | complete |
| `packages/attic-metrics/README.md` | retained in place at [the adapter operator runbook](../../../../packages/attic-metrics/README.md); see [storage observation contract](../../caches/niks3-cache.md#response-only-storage-observations) | retained | complete |
| `checks/nixos-options-metadata/README.md` | relocated source cataloged in [nixos-options-metadata section](../../testing/flake-checks.md#nixos-options-metadata) | merged | complete |
| `checks/nixos-discovery-reliability/README.md` | relocated source cataloged in [cold-input evidence and limits](../../evaluation/bulk-evaluator-resource-planning.md#cold-input-evidence-and-limits) | merged | complete |
| `checks/oidc-auth/README.md` | historical scope in [oidc-auth section](../../testing/flake-checks.md#oidc-auth); source retired by TASK-490.24 | merged | complete |
| `checks/oscal-export/README.md` | relocated source cataloged in [oscal-export section](../../testing/flake-checks.md#oscal-export) | merged | complete |
| `checks/run-ui-dev-db-check/README.md` | relocated source cataloged in [run-ui-dev-db-check section](../../testing/flake-checks.md#run-ui-dev-db-check) | merged | complete |
| `checks/server-regressions/README.md` | relocated source cataloged in [server-regressions section](../../testing/flake-checks.md#server-regressions) | merged | complete |
| `checks/stig/README.md` | relocated source cataloged in [stig section](../../testing/flake-checks.md#stig) | merged | complete |
| `checks/ui-screenshots/README.md` | relocated source cataloged in [ui-screenshots section](../../testing/flake-checks.md#ui-screenshots) | merged | complete |
| `checks/web-ui/README.md` | historical scope in [web-ui section](../../testing/flake-checks.md#web-ui); source retired by TASK-490.24 | merged | complete |
| `checks/web-ui/baselines/README.md` | historical [baseline process](../../testing/web-ui-check.md#visual-baselines); source retired by TASK-490.24 | merged | complete |
| `checks/web-ui/design-parity/eval-drawer.md` | relocation recorded in [testing index](../../testing/index.md) | merged | complete |
| `checks/web-ui-reconciliation/README.md` | historical scope in [web-ui-reconciliation section](../../testing/flake-checks.md#web-ui-reconciliation); source retired by TASK-490.24 | merged | complete |
| `checks/web-ui-test-runner/README.md` | relocated source cataloged in [web-ui-test-runner section](../../testing/flake-checks.md#web-ui-test-runner) | merged | complete |
| `checks/xccdf-schema/README.md` | relocated source cataloged in [xccdf-schema section](../../testing/flake-checks.md#xccdf-schema) | merged | complete |
| `packages/verify-builder-evaluator-packaging/README.md` | consolidated module and installed-wrapper contract in [catalog](../../testing/flake-checks.md#current-inventory-and-commands); former package removed | merged | complete |
| `packages/verify-niks3-ca-bundle/README.md` | [operator contract](../../caches/niks3-ca-bundle-compatibility.md) and [core CA ownership](../../testing/flake-checks.md#current-inventory-and-commands); former package removed | merged | complete |
| `packages/verify-nixos-options-metadata/README.md` | ordinary metadata package assertions in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/verify-nixos-discovery-reliability/README.md` | direct runtime test in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) and [cold-input evidence](../../evaluation/bulk-evaluator-resource-planning.md#cold-input-evidence-and-limits) | merged | complete |
| `packages/verify-oscal-export/README.md` | ordinary OSCAL fixture validation in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/verify-run-ui-dev-db-check/README.md` | direct tool selftests in [component catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/verify-server-regressions/README.md` | consolidated isolated state-machine workflow and selection audit in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/verify-stig/README.md` | eleven pure module assertion groups in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/ui-screenshots/README.md` | retained at relocated package source; [catalog](../../testing/flake-checks.md#ui-screenshots) | retained | complete |
| `packages/devScripts/web-ui/design-parity/eval-drawer.md` | retained at relocated developer-tool source; [testing index](../../testing/index.md) | retained | complete |
| `packages/verify-web-ui-test-runner/README.md` | direct host runner selftest in [component catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/verify-xccdf-schema/README.md` | actual installed writer/schema validation and static-example disposition in [catalog](../../testing/flake-checks.md#current-inventory-and-commands) | merged | complete |
| `packages/cf-test-suite/README.md` | retained in place; see [scenario runner section](../../testing/flake-checks.md#scenario-runner-packagescf-test-suite) | retained | complete |
| `docs/design/CrystalForge/fixtures/README.md` | retained in place; see [pointer](../../testing/design-golden-fixtures.md) | retained | complete |
| `backlog/docs/doc-1 - Task-Template.md` | retained in place; see [pointer](../../operations/backlog-process-documents.md) | retained | complete |
| `CONTRIBUTING.md` | retained in place; see [pointer](../../operations/contributing-guide.md) | retained | complete |

## Source inventory

### TASK-490.24 source-retirement disposition

The table preserves original source identities and migration-base provenance.
`merged` records the current catalog or retirement disposition after the original
path is removed; `retained` rows identify live Markdown sources. The interim
verification-package README rows are now `merged`: those files are removed,
and the component catalog and verification guide own their current contracts.
The historical source inventory below describes the earlier migration, not the
current filesystem. Its original titles, purposes and source paths remain as
provenance. Source retirement is explicitly authorized before replacement
coverage is complete; `Coverage: complete` here records a document disposition,
not completed integration coverage. No original baseline blob or inventory hash
is rewritten. Retired suite source remains available in Git history at `5fa1f4fa`.

### `checks/web-ui/design-parity/eval-drawer.md`

- Title: Evaluation drawer comparison
- Purpose: Defines the task-owned drawer shell comparison inputs, commands, assertions, structured matrix workflow, and visual acceptance limits.
- Action: retained at `packages/devScripts/web-ui/design-parity/eval-drawer.md` and cataloged in the [testing index](../../testing/index.md); original path retained here as provenance.
- Sections:
  | Source section | Destination |
  | --- | --- |
  | `## Inputs` | [Inputs](../../../../packages/devScripts/web-ui/design-parity/eval-drawer.md#inputs) |
  | `## Current contract` | [Current contract](../../../../packages/devScripts/web-ui/design-parity/eval-drawer.md#current-contract) |
  | `## Structured matrix workflow` | [Structured matrix workflow](../../../../packages/devScripts/web-ui/design-parity/eval-drawer.md#structured-matrix-workflow) |

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

### TASK-470 additions after rebase

These retained README mappings were added for TASK-470 after rebase onto the
OKF knowledge bundle. Neither README existed at the original migration base
`3b23d36f`; the historical source inventory remains unchanged.

- `checks/builder-evaluator-packaging/README.md`: Title "Builder and server runtime packaging". Action: retained. Destination: [builder-evaluator-packaging](../../testing/flake-checks.md#builder-evaluator-packaging).
- `checks/niks3-cache/README.md`: Title "Niks3 cache integration". Action: retained. Destination: [niks3-cache](../../testing/flake-checks.md#niks3-cache).
- Unmapped content: none. Both READMEs remain authoritative in place; the catalog summarizes scope, commands, limits, and CI membership.

### `checks/niks3-write-probe/README.md` (new TASK-470 runbook)

- Title: Native Niks3 write-route proof.
- Purpose: Defines the v1.6.0/v1.8.0 protected-handler compatibility gate,
  scoped token-free public metadata, protected-route authentication controls,
  retained-credential API matrix, TLS boundaries,
  complete CF/native/S3 non-mutation assertions, and separate CLI publication
  controls. Native execution and compilation remain pending CI.
- Action: retained. The new runbook remains authoritative beside its check. It
  did not exist at the original migration base `3b23d36f`; no historical baseline
  blob is claimed or added to `source-inventory.tsv`.
- Destination: [native write-probe runbook](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/niks3-write-probe/README.md) (historical; retired by TASK-490.24).
- Unmapped content: none. All source sections remain in place, including the
  distinction between protected-handler evidence and actual publication proof.

### `checks/niks3-ca-bundle/README.md` (new TASK-470 runbook)

- Title: Niks3 onboarding CA bundle regression
- Purpose: Operational runbook for the sandbox CA bundle fixture and compatibility check, including commands, public provenance, measured structure, and verification history.
- Action: retained. The newly authored README remains authoritative beside the check; it was not present at the original migration base `3b23d36f` and has no historical baseline blob.
- Destination: [CA compatibility operator contract](../../caches/niks3-ca-bundle-compatibility.md).
- Unmapped content: none. The runbook retains all operational sections in place and links to the canonical compatibility contract.

### `checks/attic-metrics/README.md` and `packages/attic-metrics/README.md` (new TASK-470 runbooks)

- Titles: Authoritative Attic inventory check; Optional Attic inventory adapter.
- Purpose: The check runbook describes native fixtures, restricted-role checks,
  accounting, credential isolation, module lifecycle verification, encrypted
  retained-credential API assertions, and the database-selected publication driver.
  The new lifecycle and publication assertions describe source coverage; their
  final execution remains pending CI. The package
  runbook defines the adapter API, logical inventory semantics, manual operator
  setup, token handling, and failure limits.
- Action: retained. Both newly authored runbooks remain authoritative at their
  original paths. Neither existed at the original migration base `3b23d36f`.
  No historical baseline blob is claimed or added to `source-inventory.tsv`.
- Destinations: [native inventory runbook](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/attic-metrics/README.md) (historical; retired by TASK-490.24), [adapter operator runbook](../../../../packages/attic-metrics/README.md), and [storage observation contract](../../caches/niks3-cache.md#response-only-storage-observations).
- Unmapped content: none. All source sections remain in place. Coverage records
  document retention, not a claim that a verification command passed.

### `checks/attic-publication/README.md` (new TASK-470 runbook)

- Title: Native Attic publication regression.
- Purpose: Defines the pinned native builder/server publication fixture,
  selected-token versus ambient/stale-credential matrix, inventory assertions,
  process ownership and cleanup, test-first evidence, and upstream contracts.
- Action: retained. The newly authored runbook remains authoritative beside the
  check. It did not exist at the original migration base `3b23d36f`; no historical
  baseline blob is claimed or added to `source-inventory.tsv`.
- Destination: [native publication runbook](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/attic-publication/README.md) (historical; retired by TASK-490.24).
- Unmapped content: none. Every source section remains in place. The fixture's
  documented limits remain explicit; final-head CI execution is pending.

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
