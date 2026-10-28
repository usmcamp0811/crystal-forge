---
id: TASK-486
title: Classify full browser fixture failures against a current dev baseline
status: Backlog
assignee: []
created_date: '2026-10-13 18:01'
labels: []
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17127883247'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/14617703162'
priority: medium
type: task
ordinal: 508000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Full optional Web UI job17127883247 at269b58ad has93passes/51failures of144, including cache legacy expectations, evaluations, compliance/POA&M stale-finding/assignment contracts, builds/CVEs/scanning, registration/profile/systems/flakes, and one harness-runtime-fatal from two route.fulfill rejections (integration-test.js1842/1851). TASK-470's focused type-switch failure is being repaired in parent; do not duplicate it. Relevant noncache assertions/components are source-equal to current dev ec71ca0c, but prior e274/f8/c3 traces stop at builds or4MiB cap. Historical dev job14617703162 reports22passes/22failures of44 despite success status, not a current green baseline. Obtain current-dev disposable full-suite evidence, classify each remaining workflow by actual terminal cause and repair in relevant scope without weakening security or visual assertions. Complete full trace retained /tmp/opencode/task470-job17127883247-full.trace; no production data needed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Obtain a current dev baseline with full workflow result artifacts and uncapped terminal evidence
- [ ] #2 Separate fixture/contract/source regressions using exact workflow names and terminal causes
- [ ] #3 Resolve route-handler lifecycle rejections without discarding failures or lowering assertion requirements
- [ ] #4 Record actual full-suite counts and keep optional-job status distinct from required gates
<!-- AC:END -->
