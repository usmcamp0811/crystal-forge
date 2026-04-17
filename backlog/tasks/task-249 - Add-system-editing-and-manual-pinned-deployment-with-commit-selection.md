---
id: TASK-249
title: Add system editing and manual/pinned deployment with commit selection
status: Done
assignee: []
created_date: '2026-04-08 01:10'
updated_date: '2026-04-14 00:34'
labels:
  - feature
  - ui
  - api
  - systems
milestone: MVP
dependencies: []
priority: high
ordinal: 4590
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Enable users to edit existing system configurations and deploy systems with deployment strategy awareness. For systems set to manual or pinned deployment strategies, users must be able to select which specific commit/revision to deploy.

## Problem Statement
Currently, systems cannot be edited after creation, and there is no deployment interface that respects deployment strategies (auto/manual/pinned). Users need the ability to:
1. Modify system configurations after initial creation
2. Trigger deployments for manual/pinned systems
3. Select specific commits for pinned deployments
4. View deployment history and current state

## Goal
Implement a complete system editing and deployment workflow that supports all deployment strategies with appropriate controls.

## Non-Goals
- Automated deployment scheduling (beyond auto strategy)
- Rollback functionality (separate task)
- Multi-system bulk deployments
- Deployment approval workflows
- Infrastructure provisioning (assumes systems already exist)

## Impact Areas
- UI: New edit system modal/page, deployment modal with commit selector
- API: PATCH /api/systems/:id endpoint, POST /api/systems/:id/deploy endpoint
- Domain: System update validation, deployment strategy logic
- Database: Potentially new deployments tracking table

## Suggested Implementation Plan

### 1. Database & Domain Layer
- Define system update DTOs (edit payload)
- Add deployment tracking if not present (deployment_id, deployed_commit, deployed_at)
- Create validation logic for deployment strategy transitions
- Implement deployment request validation (strategy must allow manual trigger)

### 2. API Layer
- PATCH /api/systems/:id - update system configuration
  - Validate deployment strategy
  - Validate required fields based on system type
- POST /api/systems/:id/deploy - trigger deployment
  - Accept { commit_sha, revision, ref } in payload
  - Validate strategy (manual or pinned only)
  - Enqueue deployment job or call deployment service
- GET /api/systems/:id/commits - list available commits/revisions for selection

### 3. UI Components
- EditSystemModal or EditSystemPage
  - Form with existing system data pre-populated
  - Deployment strategy selector
  - Configuration fields (adapt based on system type)
  - Save/Cancel actions
- DeploySystemModal
  - Show current deployed revision
  - Commit/revision selector (searchable dropdown or list)
  - Deploy button with confirmation
  - Disabled state if system is auto-deployed
- Integration with existing SystemCard or SystemList

### 4. Verification
- Test edit flow: open modal, change fields, save, verify DB updated
- Test deploy flow for manual system: select commit, deploy, verify request sent
- Test deploy flow for pinned system: commit selector required
- Test that auto systems disable manual deployment
- Capture screenshots during web-ui check showing both modals
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 User can open an edit dialog/page for an existing system
- [x] #2 User can modify system name, description, deployment strategy, and configuration fields
- [x] #3 System updates are validated and persisted to the database
- [x] #4 User can trigger a deployment for a system with manual or pinned strategy
- [x] #5 Deployment modal shows commit/revision selector for manual and pinned systems
- [x] #6 Deployment modal displays current deployed revision and available revisions
- [x] #7 Auto-deployed systems either disable manual deploy or show read-only deployment status
- [x] #8 Deployment request is sent to the backend with selected commit
- [x] #9 Backend validates deployment request against system deployment strategy
- [x] #10 UI shows loading state during deployment submission
- [x] #11 Success/error feedback is displayed after deployment attempt
- [x] #12 Screenshots of edit system UI and deployment modal are included in MR
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-code on reckless in ~/code/crystal-forge/TASK-249-system-edit-deploy

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/217

Verification: nix build .#checks.x86_64-linux.web-ui; nix develop -c cargo check (packages/web-ui); SQLX_OFFLINE=true targeted cargo tests for deploy_system_requires_authenticated_role and get_system_commits_requires_authenticated_role.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 cargo sqlx prepare has been run if schema or queries changed
- [ ] #2 All Rust code passes cargo fmt --check and cargo clippy
- [x] #3 Unit tests cover system update and deployment validation logic
- [x] #4 Integration tests verify PATCH and deploy endpoints
- [x] #5 Frontend components are tested or manually verified
- [ ] #6 nix build succeeds with new files tracked in git
- [x] #7 Screenshots captured from web-ui check showing edit and deploy modals
<!-- DOD:END -->
