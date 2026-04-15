---
id: TASK-271
title: Map system CVEs to affected NixOS modules
status: Backlog
assignee: []
created_date: '2026-04-15 02:24'
labels:
  - cve
  - nixos-modules
  - vulnerability-triage
  - backlog-capture
milestone: CVE Workflow Improvements
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/234'
priority: medium
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
System CVE views currently show vulnerable packages/versions but do not indicate which NixOS modules contributed those packages. This makes triage slower and less actionable because operators cannot quickly identify the module ownership/context behind a vulnerability.

## Desired Outcome
Add the ability to attribute CVEs to affected NixOS modules (or closest available provenance) so users can see module-level impact alongside package-level vulnerability data.

## Notes
- Keep existing migration 0110 unchanged; any schema evolution for this capability must use new migrations.
- Prefer implementation that works for both API consumers and web-ui views.
- Consider adding representative mock data so module attribution can be evaluated in server-stack-mock.
<!-- SECTION:DESCRIPTION:END -->
