---
id: TASK-429
title: Fix policy catalog loading after bulk compliance import
status: Backlog
assignee: []
created_date: '2026-08-21 00:53'
labels:
  - web-ui
  - compliance
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/views/policies_api.rs
priority: medium
type: bug
ordinal: 424000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the real 103-policy Anduril V1R2 draft import, the Deployment Policies page rendered no policy cards even after 60 seconds, while the deployment-policy API and compliance coverage API returned the imported records. Diagnose the paginated policy loader or catalog rendering so imported policies remain manageable in the UI.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Deployment Policies page renders policy cards after importing the 103-policy Anduril V1R2 fixture
- [ ] #2 Imported draft policies can be located in the appropriate policy domain
- [ ] #3 A browser regression covers policy catalog loading after a bulk compliance import
<!-- AC:END -->
