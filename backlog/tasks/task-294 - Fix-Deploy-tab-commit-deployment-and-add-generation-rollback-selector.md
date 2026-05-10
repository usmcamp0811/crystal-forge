---
id: TASK-294
title: Fix Deploy tab commit deployment and add generation rollback selector
status: To Do
assignee: []
created_date: '2026-05-10 13:23'
updated_date: '2026-05-10 13:27'
labels:
  - bug
  - feature
  - ui
  - deployment
dependencies: []
priority: high
ordinal: 249000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Deploy tab in the system detail view has two critical issues:

1. **Commit deployment not working**: Selecting a commit and clicking "Deploy" does not actually deploy that specific commit. The system appears to deploy the HEAD of the flake instead of the selected commit.

2. **Missing generation rollback feature**: Users cannot select a previous NixOS generation to rollback to, which is a standard NixOS feature that should be available.

## Current Behavior

- Deploy tab shows commit list from the flake
- User can select a commit
- Clicking "Deploy {sha}" button does not deploy that specific commit
- The flake dropdown in the commit selector is not useful (only shows current flake)
- No way to select previous generations for rollback

## Desired Behavior

### Commit Deployment (Fix)
- When user selects a commit and clicks Deploy, that specific commit SHA must be sent to the backend
- Backend must update `systems.desired_target` to the store path corresponding to that commit
- Deployment agent must build and activate that specific commit
- Verify the correct commit SHA is passed through the entire chain: UI → API → DB → Agent

### Generation Rollback (New Feature)
- Repurpose the flake dropdown (or replace it) to show a list of previous NixOS generations
- Query `system_states` table for historical generation numbers associated with this system
- Display generations in descending order (e.g., "Generation 74", "Generation 73", "Generation 72")
- Show metadata for each generation: commit SHA, deployed timestamp, store path
- When user selects a generation, allow them to deploy/rollback to that generation
- Use appropriate NixOS mechanism (either `nix-env --rollback`/`--switch-generation` OR redeploy the store path from that generation)

## Technical Context

**Current Generation Tracking** (from codebase analysis):
- Generations are stored in `system_states.generation` (integer, nullable)
- Flag `system_states.generation_matches_current_store_path` tracks profile/current mismatches
- Generations are auto-detected by parsing `/nix/var/nix/profiles/system-{N}-link`
- Current generation is displayed in system detail view
- Historical generations are stored but not exposed in UI or API

**Current Rollback Implementation**:
- `POST /api/v1/systems/:id/rollback` endpoint exists
- Takes `SystemRollbackRequest { target_commit: String }`
- Sets `systems.desired_target` to deploy historical commit
- Creates a NEW generation (not true `nix-env --rollback`)

**Deploy Tab Location**:
- Component: `DeployTab` in `packages/web-ui/src/views/system_detail.rs` (lines 1506-1765)
- Props: `system`, `commits` (Vec<SystemCommitHistory>), `allow_mutations`, `on_deploy_commit` handler
- Current commit selector shows `SystemCommitHistory` entries from flake

## Design Consistency Requirement

The implementation MUST follow the established design system in this codebase:
- Use existing CSS classes from the design system (`sd-*`, `btn`, `chip`, `focus-ring`, etc.)
- Match the visual style of the current Deploy tab (card layout, kv-grid, callouts)
- Maintain the two-panel layout (left: selector, right: deployment plan)
- Use consistent spacing, colors, typography, and interaction patterns
- Follow the design patterns from surrounding components (Overview, History, Config tabs)

## Architecture Requirements

**Frontend (Dioxus)**:
- Component composition and reusability
- DTOs mirror server models
- State isolated from presentation

**Backend (Rust)**:
- Domain-oriented modules
- Explicit error types, Result-based error handling
- No unwrap in production paths
- Follow existing query and handler patterns

**Database**:
- Use existing `system_states` table for generation history
- Query historical states ordered by generation DESC
- May need new query function to fetch generation list for a system
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deploy tab commit selection actually deploys the selected commit (not HEAD)
- [ ] #2 Clicking Deploy button with commit selected triggers deployment of that exact commit SHA
- [ ] #3 Backend receives correct commit SHA in DeploySystemRequest
- [ ] #4 Deployment agent builds and activates the selected commit
- [ ] #5 Generation selector UI component displays list of previous generations for the system
- [ ] #6 Generation list shows: generation number, commit SHA (short), deployed timestamp, current indicator
- [ ] #7 Generation list is sorted descending (newest first)
- [ ] #8 User can select a generation from the list
- [ ] #9 Clicking Deploy with generation selected triggers rollback to that generation's store path
- [ ] #10 Deployment plan panel updates to show selected generation details
- [ ] #11 UI follows existing design system (sd-* classes, btn styles, focus-ring, etc.)
- [ ] #12 Generation selector replaces or repurposes the existing flake dropdown
- [ ] #13 Empty state handling when no historical generations exist
- [ ] #14 Loading state while fetching generation list
- [ ] #15 Error handling if generation list fetch fails
- [ ] #16 Verification: Manual test deploying specific commits works
- [ ] #17 Verification: Manual test rolling back to previous generation works
<!-- AC:END -->
