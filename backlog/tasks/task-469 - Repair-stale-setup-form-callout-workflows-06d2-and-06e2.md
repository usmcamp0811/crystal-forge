---
id: TASK-469
title: Repair stale setup form-callout workflows 06d2 and 06e2
status: Backlog
assignee: []
created_date: '2026-10-01 23:57'
labels:
  - web-ui
  - tests
dependencies: []
type: chore
ordinal: 490000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while verifying TASK-326.2.2 onboarding coach work. Workflows 06d2-onboarding-builders-form-callouts and 06e2-onboarding-caches-form-callouts in checks/web-ui/tests/integration-test.js assert test ids, placeholders and button labels that do not exist in the production source at the TASK-326.2.2 base commit. Examples: setup-coach-builder-field-name, setup-coach-builder-field-public-key, setup-coach-builder-resource-guidance-callout, setup-coach-cache-field-name, setup-coach-cache-field-type, setup-coach-cache-field-endpoint, the "Add Destination" button, and the input placeholders "e.g., builder-01" and "main-cache". The 06b-06f workflows also need an empty setup state, so they only pass on a fresh database such as the CI VM. They cannot run against a populated local preview database. Update the assertions to the current builder and cache forms, or remove assertions for behavior that no longer exists.
<!-- SECTION:DESCRIPTION:END -->
