---
id: TASK-446
title: Bring environment and cache destination editors to sectioned design parity
status: To Do
assignee: []
created_date: '2026-08-31 02:21'
labels:
  - web-ui
  - environments
  - caches
  - modals
  - design-parity
dependencies:
  - TASK-433
  - TASK-440
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/components/CachesView.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/components/environments/environment_form_modal.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/views/caches.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: enhancement
ordinal: 457000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the policy/POA&M and configuration/flake merge requests land, update environment and cache destination create/edit workflows to the sectioned editor presentation in design commit ac582592. The change is presentation and interaction parity around existing real mutations: it must retain policy assignments, bundle enforcement, cache credentials, connection testing, authorization, validation, and deletion safeguards.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Environment create/edit uses Basics Binary cache Deployment Policy enforcement and edit-only Danger zone sections with authoritative header chips section counts and footer state summary
- [ ] #2 Environment editing preserves color production classification cache assignment deployment defaults approval and auto-sync behavior gate policies compliance-bundle enforcement and assignment readiness
- [ ] #3 Cache destination create/edit uses Destination Credentials and Environments sections with authoritative missing-field indicators and footer state summary
- [ ] #4 Cache editing preserves S3-compatible Attic and Nix HTTPS types authentication selection credential creation connection testing environment assignment and real save errors
- [ ] #5 Switching sections never discards unsaved values and validation errors identify the affected section before submission
- [ ] #6 Deletion safeguards preserve current system-assignment and environment/cache dependency semantics and use separate confirmation dialogs
- [ ] #7 Mutation authorization and hidden-environment behavior remain enforced at the API boundary and the UI does not reveal inaccessible assignments
- [ ] #8 Editors provide accessible focus trapping Escape/backdrop behavior focus restoration keyboard navigation and in-flight duplicate-submit prevention
- [ ] #9 Desktop narrow light and dark layouts match the design without clipping overlap or unusable horizontal rail behavior
- [ ] #10 Focused Rust tests and the authoritative web-ui check pass with assertion coverage and screenshots for add edit validation testing error assignment deletion and keyboard states
<!-- AC:END -->
