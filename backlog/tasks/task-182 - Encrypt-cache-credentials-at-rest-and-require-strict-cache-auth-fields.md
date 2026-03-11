---
id: TASK-182
title: Encrypt cache credentials at rest and require strict cache auth fields
status: To Do
assignee: []
created_date: '2026-03-11 12:47'
updated_date: '2026-03-11 12:47'
labels:
  - security
  - cache
  - backend
  - web-ui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem Statement:
Cache destination credentials are currently persisted as plaintext database fields, which is an unacceptable security risk. In addition, Attic token and key S3 auth fields must be treated as required for configured authenticated destinations.

Goal:
Implement encryption at rest for cache credential fields and enforce required credential semantics consistently in backend validation and web UI behavior.

Non-Goals:
- External secret manager integration.
- Rotating previously issued credentials outside this feature scope.
- Broader auth refactors unrelated to cache destination credential storage.

Architectural Constraints:
- Keep encryption/decryption logic out of UI; backend/domain/infrastructure layers own secret handling.
- Preserve existing API boundaries and admin authorization requirements.
- Avoid introducing global mutable state.

Verification Plan:
- nix develop -c bash -c "cd packages/default && SQLX_OFFLINE=true cargo check"
- nix develop -c bash -c "cd packages/web-ui && cargo check"
- nix develop -c bash -lc "nix run .#devScripts.db-only -- up -D && cd packages/default && cargo sqlx prepare" (if SQLx metadata changes)
- Add/adjust unit tests for encryption/decryption and validation behavior.

Impact Areas:
- cache destination schema + migrations
- backend cache models/queries/handlers
- runtime cache usage path (decrypt before use)
- web-ui cache modal validation text/required handling

Risk Level: high (security-sensitive storage and migration behavior)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Sensitive cache fields (Attic token, S3 secret/session credential material) are not stored as plaintext in persisted database columns.
- [ ] #2 Backend can decrypt and use credential fields at runtime for cache operations without exposing plaintext values in API responses.
- [ ] #3 Create and update paths enforce required Attic/S3 fields according to destination type.
- [ ] #4 Existing rows remain compatible through migration strategy (no runtime breakage).
- [ ] #5 Automated tests cover encryption/decryption behavior and required-field validation paths.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do per explicit maintainer request for immediate execution.
<!-- SECTION:NOTES:END -->
