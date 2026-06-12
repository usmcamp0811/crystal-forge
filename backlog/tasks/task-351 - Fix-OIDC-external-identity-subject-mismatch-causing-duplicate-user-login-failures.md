---
id: TASK-351
title: >-
  Fix OIDC external identity subject mismatch causing duplicate-user login
  failures
status: Backlog
assignee: []
created_date: '2026-06-12 21:02'
updated_date: '2026-06-12 23:11'
labels:
  - auth
  - oidc
  - bug
  - blocker
milestone: m-19
dependencies: []
references:
  - packages/default/src/handlers/api/auth_oidc.rs
  - packages/default/src/queries/auth_identity.rs
modified_files:
  - packages/default/src/handlers/api/auth_oidc.rs
  - packages/default/src/queries/auth_identity.rs
priority: high
ordinal: 50
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: OIDC login can authenticate the user successfully but then fail with `Database error during user lookup/creation` because Crystal Forge extracts an email-like subject at runtime while existing `external_identities` rows are keyed by the stable OIDC `sub` value. This causes external identity lookup to miss, then user creation collides on `users_email_key`.

Desired outcome: Crystal Forge consistently binds OIDC identities using the stable provider subject claim (`sub`) or performs a safe migration/link strategy so existing users can log in without duplicate-user failures after OIDC claim/config changes.

Urgency: This is currently blocking validation on the test server and preventing review of the active Systems MR.

Immediate workaround discovered during review:
- Update the affected `external_identities.subject` row to the email-like value currently being extracted at runtime.
- Then implement the proper code fix so Crystal Forge uses a stable subject consistently and/or safely relinks existing users.

Suggested acceptance criteria:
- Existing OIDC-linked users can log in successfully after deploy without duplicate-user creation attempts.
- OIDC external identity lookup uses a stable subject value consistently across new and existing logins.
- If legacy rows already exist with a different subject shape, login safely relinks or migrates instead of failing on `users_email_key`.
- Regression coverage proves the app does not create a duplicate user when the email already exists.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Existing OIDC-linked users can log in successfully after deploy without duplicate-user creation attempts
- [ ] #2 OIDC external identity lookup uses a stable subject value consistently across new and existing logins
- [ ] #3 If legacy rows already exist with a different subject shape, login safely relinks or migrates instead of failing on users_email_key
- [ ] #4 Regression coverage proves the app does not create a duplicate user when the email already exists
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Raised urgency: this bug is actively blocking test-server login and review of MR !273.
<!-- SECTION:NOTES:END -->
