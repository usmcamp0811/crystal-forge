---
id: TASK-462
title: Repair policy draft lifecycle server regression CSRF setup
status: Backlog
assignee: []
created_date: '2026-09-10 05:32'
updated_date: '2026-09-10 05:50'
labels:
  - server
  - tests
  - csrf
  - policy-draft
dependencies: []
references:
  - TASK-440
modified_files:
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
priority: medium
type: bug
ordinal: 475000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The ignored `policy_draft_derived_from_published` cf-server regression fails at its publication prerequisite with HTTP 403 before draft creation. The test sends only the authenticated session cookie, while `publish_policy_version` requires a matching CSRF cookie and `x-csrf-token` header after authentication and role checks. Update the test fixture/request to satisfy the production CSRF contract without weakening production enforcement, then prove the publication and policy-draft lifecycle assertions against isolated PostgreSQL.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The ignored `policy_draft_derived_from_published` regression sends a matching CSRF cookie and header for every protected mutation.
- [ ] #2 The regression reaches draft creation and passes against an isolated repository-managed PostgreSQL database.
- [ ] #3 Production CSRF enforcement and policy draft behavior remain unchanged.
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: openai-agent
created: 2026-09-10 05:50
---
Superseded by TASK-440 commit `1e986cd2351652e866f9b72bff956a312fc0827a`. The focused policy-draft lifecycle fixture now sends matching CSRF credentials and passes against isolated PostgreSQL. No separate implementation remains.
---
<!-- COMMENTS:END -->
