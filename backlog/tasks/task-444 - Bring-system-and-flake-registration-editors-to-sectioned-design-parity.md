---
id: TASK-444
title: Bring system and flake registration editors to sectioned design parity
status: To Do
assignee: []
created_date: '2026-08-31 02:21'
labels:
  - web-ui
  - systems
  - flakes
  - modals
  - design-parity
dependencies:
  - TASK-435
  - TASK-440
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/319'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - docs/design/CrystalForge/components/AddSystemModal.jsx
  - docs/design/CrystalForge/components/EditSystemModal.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/components/forms/add_system_form.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: enhancement
ordinal: 455000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the active system-key and configuration/flake work merges, update the add/edit system and add/edit flake workflows to the sectioned editor presentation introduced in design commit ac582592. Preserve every backend-backed field, validation rule, authorization check, onboarding hook, key-rotation state, prefilled registration context, and save/delete behavior while replacing the inconsistent flat and tabbed modal layouts with the current design hierarchy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add System uses the authoritative Host and environment Agent identity Flake assignment and Policy and tags sections with persistent header context section status indicators and footer state summary
- [ ] #2 Edit System uses the authoritative General Deployment Security and Danger zone sections and preserves the complete merged TASK-435 key-rotation workflow including in-flight locking and ambiguous-outcome recovery
- [ ] #3 System address entry uses one clearly defined FQDN-or-address value and reachability behavior without exposing a second contradictory server-address field
- [ ] #4 Add System preserves flake configuration prefill onboarding coach behavior server validation and registration success instructions
- [ ] #5 Add and Edit Flake use the authoritative Repository Credentials Sync and Danger zone sections while preserving real connection testing credential handling sync settings and removal safeguards
- [ ] #6 Each editor keeps primary actions available without losing entered state when switching sections and reports validation save loading success and error states in the section and footer context
- [ ] #7 Backdrop Escape close focus trap and focus restoration behavior is accessible and destructive confirmations remain separate guarded dialogs
- [ ] #8 Viewer and environment authorization behavior remains server-enforced and controls are hidden or disabled consistently with the merged product
- [ ] #9 Desktop narrow light and dark rendering matches the design without body clipping or nested modal scrolling regressions
- [ ] #10 Focused Rust tests and the authoritative web-ui check pass with assertion coverage and screenshots for create edit validation key rotation prefill error delete and keyboard workflows
<!-- AC:END -->
