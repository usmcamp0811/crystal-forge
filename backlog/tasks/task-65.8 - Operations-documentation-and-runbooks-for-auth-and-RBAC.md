---
id: TASK-65.8
title: Operations documentation and runbooks for auth and RBAC
status: Backlog
assignee: ["GLM5.1"]
labels:
  - docs
  - security
  - auth
  - operations
milestone: m-14
dependencies:
  - TASK-65.7
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Operators need clear setup and troubleshooting guidance to run auth securely and maintainably.

Goal
Publish operator and developer docs for OIDC setup, RBAC mapping, dev mode behavior, and machine-auth carve-outs.

Non-Goals
- Public product marketing docs.
- Non-auth unrelated documentation cleanup.

Architectural Constraints
- Documentation must match implemented behavior and tests.
- Security-sensitive examples must avoid unsafe defaults.

Verification Plan
- Run repository documentation checks if configured.
- Manual: fresh-setup walkthrough from docs in devshell with `AUTH_MODE=dev`.
- Manual: OIDC mode setup validation with one provider.

Impact Areas
- Documentation, Operations, Security

Risk Level
- Low
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 OIDC setup guide includes required env and config for generic and named providers
- [ ] #2 RBAC mapping guide includes Admin, Operator, Viewer matrix and claim mapping defaults
- [ ] #3 Dev mode runbook includes selector flow and non-dev guardrails
- [ ] #4 `/api/agent/**` machine-auth carve-out is clearly documented
- [ ] #5 Troubleshooting section includes common failure modes and recovery steps
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: operator hardening checklist improvements.
<!-- SECTION:NOTES:END -->
