---
id: TASK-351
title: >-
  Fix OIDC external identity subject mismatch causing duplicate-user login
  failures
status: Backlog
assignee: []
created_date: '2026-06-12 21:02'
labels:
  - auth
  - oidc
  - bug
milestone: m-19
dependencies: []
references:
  - packages/default/src/handlers/api/auth_oidc.rs
  - packages/default/src/queries/auth_identity.rs
modified_files:
  - packages/default/src/handlers/api/auth_oidc.rs
  - packages/default/src/queries/auth_identity.rs
priority: high
ordinal: 1700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: OIDC login can authenticate the user successfully but then fail with `Database error during user lookup/creation` because Crystal Forge extracts an email-like subject at runtime while existing `external_identities` rows are keyed by the stable OIDC `sub` value. This causes external identity lookup to miss, then user creation collides on `users_email_key`.

Desired outcome: Crystal Forge consistently binds OIDC identities using the stable provider subject claim (`sub`) or performs a safe migration/link strategy so existing users can log in without duplicate-user failures after OIDC claim/config changes.
<!-- SECTION:DESCRIPTION:END -->
