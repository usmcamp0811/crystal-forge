---
id: TASK-65.7
title: Provider compatibility and security validation
status: Done
assignee:
  - Claude Opus 4.5
created_date: ''
updated_date: '2026-03-13 01:24'
labels:
  - security
  - oidc
  - testing
  - qa
milestone: m-14
dependencies:
  - TASK-65.2
  - TASK-65.3
  - TASK-65.4
  - TASK-65.5
  - TASK-65.6
priority: medium
ordinal: 66000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Cross-provider behavior and security posture must be validated before rollout.

Goal
Establish compatibility validation and security regression coverage for supported providers and auth modes.

Non-Goals
- Building provider-specific feature forks.
- Replacing baseline automated tests with manual-only checks.

Architectural Constraints
- Keep validation deterministic and reproducible.
- Prefer targeted tests for changed areas.

Verification Plan
- `nix develop -c cargo test --package default auth::integration_matrix`
- `nix develop -c cargo test --package default auth::security_regression`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: execute provider smoke checks with documented setup profiles.

Impact Areas
- Security, API, Infrastructure, QA

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Validation matrix includes Authentik, Keycloak, Entra, Okta, and generic OIDC
- [ ] #2 Security checks cover token validation, role mapping failures, denied access, and session invalidation
- [ ] #3 Regression coverage confirms `/api/agent/**` key-auth path stability
- [ ] #4 Findings and residual risks are documented
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: expanded reliability and chaos testing for auth stack.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/128
Verification:
- nix develop -c cargo test --package crystal-forge auth::integration_matrix (pass)
- nix develop -c cargo test --package crystal-forge auth::security_regression (pass)
- nix develop -c cargo clippy -- -D warnings (fail: existing workspace lint debt and rustc artifact mismatch E0514)
Follow-up: TASK-80 created for clippy/toolchain stabilization.
<!-- SECTION:NOTES:END -->
