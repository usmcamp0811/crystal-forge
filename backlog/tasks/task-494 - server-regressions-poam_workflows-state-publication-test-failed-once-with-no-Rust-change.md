---
id: TASK-494
title: >-
  server-regressions: poam_workflows state-publication test failed once with no
  Rust change
status: Backlog
assignee: []
created_date: '2026-10-18 17:44'
labels:
  - testing
  - ci
  - flaky
dependencies: []
references:
  - crates/cf-server/tests/poam_workflows.rs
  - TASK-490.24
priority: medium
type: bug
ordinal: 545000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`flake-check: [server-regressions]` failed in GitLab pipeline 2960470533 (MR !335, head `d28dfc9a`, job 17266273058): `crates/cf-server/tests/poam_workflows.rs` test `unlinked_environment_host_state_publication_precedes_cve_poam_closure` panicked at line 2680 with `assertion left == right failed: B must have no finding key for the state publisher to discover` (129 passed, 1 failed). The same check passed in pipeline 2960400212 at `9c6ce8ff`. No Rust, migration or module file changed between those heads (only CI selection, docs, task records, and integration-check files), so the failure looks non-deterministic or timing dependent. The failing assertion reads `poam_cve_findings` for system B immediately after `report_external_generation`.

Found while working on TASK-490.24; not investigated or fixed there. Next step: re-run to see whether it reproduces, then find the race between the generation report and the finding publication. Do not weaken the assertion.
<!-- SECTION:DESCRIPTION:END -->
