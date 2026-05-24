---
id: TASK-316
title: 'Add waiver lifecycle API with scope, expiration, and auditability'
status: Backlog
assignee: []
created_date: '2026-05-24 02:33'
labels:
  - compliance
  - waiver
  - backend
  - api
milestone: Compliance Foundations
dependencies:
  - TASK-309
  - TASK-314
priority: high
ordinal: 3150
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Compliance gates require formal, auditable exceptions. Existing policy behavior does not provide a first-class waiver lifecycle object with governance metadata.

## Goal
Implement waiver records and APIs that support scoped exceptions with expiration, approver identity, reason, and risk acceptance while preserving audit history.

## Non-Goals
- No advanced approval workflow orchestration beyond required fields and validation.
- No external GRC system integration in this task.

## Architectural Constraints
- Waivers must be scoped to bundle/control and system or system-group target.
- Expired waivers must automatically stop affecting effective compliance status.
- Waiver state must be represented distinctly from normal PASS/FAIL.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::waiver
- nix develop -c cargo test --package default handlers::api::compliance

## Impact Areas
- packages/default/src/domain/**
- packages/default/src/queries/**
- packages/default/src/handlers/api/**
- packages/default/src/services/**
- migrations/**

## Risk Level
Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Waiver model supports bundle_id, control_id, target scope (system/system-group), reason, approver, created_at, expires_at, and risk acceptance fields.
- [ ] #2 Waiver APIs support create/list/revoke and include audit metadata in responses.
- [ ] #3 Compliance evaluation logic can treat active waiver as a distinct waived status.
- [ ] #4 Expired/revoked waivers no longer alter effective status.
- [ ] #5 Tests cover scope validation, expiration behavior, and revocation semantics.
<!-- AC:END -->
