---
id: TASK-440.4
title: Complete system-scoped compliance assignment workflows
status: Backlog
assignee: []
created_date: '2026-09-18 03:51'
labels:
  - compliance
  - poam
  - environments
  - follow-up
  - TASK-440
dependencies: []
references:
  - TASK-440
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
modified_files:
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/environments/adapter.rs
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
parent_task_id: TASK-440
priority: high
type: feature
ordinal: 477000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The accepted Compliance design supports assignments scoped to environments and individual systems. TASK-440's integrated Compliance and environment flows expose environment replacement but do not provide the full system-scoped assignment interaction, and a multi-step environment save can leave metadata, gate policy, and assignments partially reconciled after a failure. Operators need complete assignment coverage and retry-safe persistence before these workflows can be treated as atomic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Authorized users can create, edit, and remove system-scoped compliance bundle assignments from the supported Compliance workflow.
- [ ] #2 Environment-scoped and system-scoped assignments remain visibly distinct and use the accepted design terminology and assignment model.
- [ ] #3 Saving environment metadata, deployment gate policy, and assignment changes cannot leave an undocumented partial result; failures expose the persisted state and a retry cannot replay stale differences.
- [ ] #4 Concurrent edits and retries use an explicit conflict contract and do not silently overwrite newer assignment state.
- [ ] #5 Authorization and hidden-environment behavior remain non-disclosing, and unauthorized users cannot mutate assignments.
- [ ] #6 Loading, empty, error, conflict, and success states are covered at wide, dark, light, and narrow viewports with keyboard and focus behavior.
- [ ] #7 Focused backend, persistence, frontend, and authoritative browser tests cover system scope and partial-failure recovery.
- [ ] #8 Compliance and environment documentation states transaction boundaries, retry semantics, and assignment scope behavior.
<!-- AC:END -->
