---
id: TASK-335
title: >-
  Create User Profile view with exact CrystalForgelatest parity and
  backend-backed account data
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - user-profile
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
  - TASK-332
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
modified_files:
  - packages/web-ui/src/views/profile.rs
  - packages/web-ui/src/state/auth.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
  - checks/web-ui
priority: high
ordinal: 1680
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: User profile/account surface is missing or not aligned to latest design standards and lacks complete parity verification.

Goal: Create/refine User Profile view with exact visual and interaction parity to CrystalForgelatest and real backend-backed account/session data.

Non-goals: Reworking global auth architecture beyond profile surface requirements.

Replan note: this is a missing-surface m-20 task. Deliver it as a vertical slice with real data rather than placeholder-first UI.

Scope details:
- Implement User Profile page sections and controls matching reference design.
- Ensure profile/account data, preferences, and security-related states are sourced from backend APIs.
- Match interaction details for edit/save/cancel/validation/feedback states.
- Align loading/empty/error/success states with design standards.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 User Profile view is pixel-aligned with CrystalForgelatest references across supported breakpoints
- [ ] #2 Profile interactions (edit/save/cancel/validation feedback) match design behavior exactly
- [ ] #3 Profile/account content is backend-driven in production path
- [ ] #4 web-ui check includes assertion-based validation for critical profile workflows and state transitions
- [ ] #5 web-ui check captures screenshots for profile loading, empty, error, populated, and editing/confirmation states
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Prefer UI + backend account data together to avoid rework from placeholder states.
<!-- SECTION:NOTES:END -->
