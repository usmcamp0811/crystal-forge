---
id: TASK-187
title: First-Time Admin Setup Wizard — Guided Onboarding Flow
status: Backlog
assignee: []
created_date: '2026-03-13 01:16'
labels:
  - frontend
  - backend
  - admin
  - ux
  - onboarding
dependencies: []
references:
  - docs/eval-build-deploy-flow.md
  - packages/web-ui/src/views/register.rs
  - packages/default/src/handlers/api/auth_status.rs
  - modules/nixos/crystal-forge/default.nix
  - docs/specs/00-system-overview.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

After a new admin registers and logs into Crystal Forge for the first time, they land on an empty dashboard with no guidance. They must independently know the correct setup sequence to get a working deployment pipeline: create environments → add flakes → register builders → configure cache → add systems → deploy agents. Missing any step results in silent pipeline failures with no indication of what went wrong.

While Crystal Forge has a first-admin registration flow (auto-detected when zero users exist), there is no post-registration onboarding. The admin is dropped into an empty UI and left to figure out the required configuration order on their own.

## Desired Outcome

A guided setup wizard that activates after the first admin registers and logs in. The wizard walks the admin through the essential configuration steps in the correct order, ensuring the deployment pipeline is functional before they start using the system for real.

## Proposed Flow

The wizard should guide the admin through these steps in order (reflecting the pipeline dependency chain):

### Step 1: Create Your First Environment
- Explain what environments are (organizational units for systems, builders, caches)
- Inline form or link to Environments page to create one
- Minimum: one environment created

### Step 2: Add a Flake
- Explain what flakes represent (source of NixOS configurations)
- Inline form or link to Flakes page
- May require git authentication setup (SSH key or .netrc) — warn about this
- Minimum: one flake added and polling

### Step 3: Register a Builder
- Explain what builders do (process build jobs for evaluated derivations)
- Guide through keypair generation (builders view already has this)
- Assign builder to the environment created in Step 1
- Minimum: one builder registered and assigned

### Step 4: Configure a Cache Destination
- Explain the role of caches (how agents pull built store paths)
- Guide through S3/Attic/Nix cache configuration
- Assign cache to the environment created in Step 1
- Optional: configure signing key
- Minimum: one cache configured and assigned

### Step 5: Register a System
- Explain systems (managed NixOS hosts)
- Link system to the flake from Step 2 and environment from Step 1
- Generate or provide system public key
- Set initial deployment policy (explain manual vs auto_latest vs pinned)
- Minimum: one system registered

### Step 6: Deploy the Agent (informational)
- Explain that the CF agent NixOS module must be enabled on target hosts
- Show the NixOS module configuration snippet (`services.crystal-forge.client`)
- Explain agent heartbeat and how the server detects connected agents
- This step is informational — the admin completes it outside the UI

### Completion
- Summary of what was configured
- Link to the dashboard which should now show meaningful data
- Note that the first evaluation will begin automatically when the flake is polled

## Technical Notes

- The wizard state should be tracked server-side (e.g., a `setup_wizard_completed` flag on the admin user or a global setting) so it doesn't reappear after completion.
- The wizard should be skippable — an experienced admin may not need it. Add a "Skip Setup" option that dismisses it permanently.
- Consider making the wizard re-accessible from the admin settings (e.g., "Re-run Setup Wizard") for admins who skipped it.
- The wizard should integrate with the Configuration Health Warnings feature (separate task) — if the wizard is skipped, the health warnings serve as the fallback guidance.
- Each wizard step should validate completion before allowing the admin to proceed (can still skip individual steps, but with a warning).

## UX Considerations

- The wizard could be a modal overlay, a dedicated `/setup` route, or a sidebar panel — design TBD during sprint refinement.
- Steps should show progress (stepper component with checkmarks).
- Each step should have a brief explanation of *why* this configuration matters (not just *what* to do).
- The wizard should feel lightweight, not bureaucratic — the goal is to accelerate time-to-first-deploy, not slow it down.

## Dependencies

- This task is related to but independent of the Configuration Health Warnings task. The wizard is the proactive onboarding path; the health warnings are the reactive fallback.

## References

- First-admin registration flow: `packages/web-ui/src/views/register.rs`
- Setup status endpoint: `packages/default/src/handlers/api/auth_status.rs`
- NixOS client module: `modules/nixos/crystal-forge/default.nix`
- Deployment pipeline: `docs/eval-build-deploy-flow.md`
- Entity relationships: database migrations in `packages/default/migrations/`
<!-- SECTION:DESCRIPTION:END -->
