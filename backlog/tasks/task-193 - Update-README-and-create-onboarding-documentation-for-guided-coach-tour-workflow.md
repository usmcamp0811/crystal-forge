---
id: TASK-193
title: >-
  Update README and create onboarding documentation for guided coach tour
  workflow
status: To Do
assignee: []
created_date: '2026-03-16 01:02'
updated_date: '2026-03-16 01:03'
labels:
  - documentation
  - onboarding
  - readme
  - screenshots
dependencies: []
references:
  - >-
    backlog/tasks/task-191 -
    Replace-blocking-setup-wizard-with-non-blocking-guided-coach-panel.md
  - packages/web-ui/src/components/onboarding/coach_panel.rs
  - checks/web-ui/
  - docs/screenshots/
  - README.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The README and project documentation are outdated and do not reflect the new guided onboarding coach tour implemented in TASK-191. New sysadmins (target audience: average Nix skills) need clear, screenshot-rich documentation that mirrors the in-app guided tour to successfully set up Crystal Forge for the first time.

Current gaps:
- README screenshots are from earlier UI iterations and don't show the coach panel
- No dedicated onboarding guide exists that walks through the complete setup flow
- The 6-step guided tour (Environment → Flake → Builder → Cache → System → Agent) is not documented outside the code
- No visual documentation of the progressive field-level callouts and contextual guidance
- Setup workflow documentation does not explain the relationship between the web UI tour and NixOS module configuration

## Goal

1. **Update README.md** with current screenshots showing the coach panel and modern UI
2. **Create `docs/onboarding-guide.md`** that mirrors the guided coach tour workflow with detailed explanations and screenshots
3. **Target audience**: Sysadmins with average NixOS/Nix skills setting up Crystal Forge for the first time
4. **Documentation style**: Step-by-step walkthrough mirroring the exact 6-step coach panel flow, with screenshots for each major interaction and additional detail where helpful

## Non-Goals

- Do not redesign the coach tour UI itself (TASK-191 already complete)
- Do not rewrite unrelated sections of the README (keep architecture, security model, etc. as-is unless screenshot updates require context changes)
- Do not create video tutorials or animated guides (screenshots only)
- Do not add advanced troubleshooting or operational runbooks (onboarding guide should focus on first-time setup only)
- Do not document every NixOS module option exhaustively (link to module reference instead)

## Proposed Structure

### README.md Updates

1. Update "What's New" section with current coach panel screenshot
2. Replace outdated screenshots in Web UI Views table with current versions
3. Add brief mention of guided onboarding coach in the "Quick Start" or "Getting Started" section
4. Ensure screenshot paths are correct and images exist in `docs/screenshots/`

### New docs/onboarding-guide.md

Structure mirroring the 6-step coach tour:

```markdown
# Crystal Forge Onboarding Guide

## Introduction
- What this guide covers
- Prerequisites (NixOS knowledge, Git, flake basics)
- Overview of the 6-step guided tour

## Before You Begin
- Server requirements
- Database setup
- Initial module configuration

## The Guided Setup Coach
- How to access the coach panel
- How the coach works (checklist, contextual callouts, progressive guidance)
- Screenshot of coach panel in minimized and expanded states

## Step 1: Create Environment
- What environments are in Crystal Forge
- Screenshot of Environments page with coach callout
- Screenshot of Create Environment modal with Required Policies callout
- Explanation of deployment policies
- Example environment configuration

## Step 2: Add Flake
- What flakes represent in Crystal Forge
- Screenshot of Flakes page with coach callout
- Screenshot of Add Flake modal with progressive field callouts (Name → Repo → Branch)
- Git repository URL formats
- Branch tracking behavior
- Example flake configuration

## Step 3: Register Builder
- What builders do (evaluation, CVE scanning, cache pushing)
- Screenshot of Builders page with coach callout
- Screenshot of Add Builder modal showing progressive field guidance:
  - Name callout
  - Public Key callout
  - Resource guidance callout (CPU/Memory/Concurrency)
  - Environment assignment
- Resource allocation recommendations for first-time setup
- Screenshot of builder runtime reminder modal
- How to enable the builder agent in NixOS config

## Step 4: Configure Cache
- What cache destinations are
- Screenshot of Caches page with coach callout
- Screenshot of Add Cache Destination modal with progressive guidance:
  - Name callout
  - Type callout (Nix/HTTP/S3/Attic)
  - Endpoint callout
  - Environment assignment
- Common cache types and when to use them
- Example cache configurations (S3, Attic, local)

## Step 5: Register System
- What systems represent
- Screenshot of Systems page with coach callout
- Screenshot of Add System modal with progressive field guidance:
  - Hostname callout
  - Public Key callout
  - Environment assignment
  - Flake assignment
- Public key generation (screenshot of key modal)
- How deployment policies interact with systems
- Screenshot of system creation success

## Step 6: Deploy Agent
- Screenshot of agent deployment reminder modal
- How to enable the Crystal Forge agent module in host config
- Example NixOS configuration snippet
- How to apply and rebuild host config
- How to verify agent service is running
- What to expect after agent connects (telemetry, deployment status)
- Screenshot of coach panel showing all steps complete

## After Onboarding
- Where to go next (Dashboard, monitoring, builds)
- How to reopen the coach (Server Management)
- Common first tasks (trigger build, deploy to system)

## Troubleshooting
- Coach panel not appearing
- Steps not marking complete
- Agent not connecting
- Common configuration mistakes
```

## Architecture Constraints

- All screenshots MUST be generated from the existing web-ui check flow (`checks/web-ui`) to ensure reproducibility
- Screenshots MUST be stored in `docs/screenshots/` with descriptive names
- Documentation MUST NOT duplicate NixOS module reference content (link to it instead)
- Guide MUST follow the exact 6-step sequence from coach_panel.rs: Environment, Flake, Builder, Cache, System, Agent
- Documentation MUST be plain markdown (no custom tooling, no doc generation framework)
- Keep existing README structure and sections that don't need screenshot updates

## Impact Areas

- README.md (screenshot updates, onboarding section addition)
- docs/screenshots/ (new/updated screenshot files)
- docs/onboarding-guide.md (new file)
- checks/web-ui/ (MAY need script adjustments to export additional screenshots if current coverage is insufficient)

## Risk Level

Low — documentation-only changes, no code or configuration changes required.

## Verification Plan

Tier 0:
- Manual review: verify all screenshots exist and are current
- Manual review: verify all links in documentation are valid
- Manual review: walk through onboarding-guide.md as a first-time user and confirm clarity
- Verify docs/ directory structure is clean and organized
- Verify README.md renders correctly on GitLab

Tier 1 (optional):
- Have a team member unfamiliar with Crystal Forge attempt setup using only the new onboarding guide
- Collect feedback on clarity and completeness

No Tier 2 verification needed (documentation only, no code changes).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README.md includes updated screenshots showing the current coach panel UI
- [ ] #2 README.md "What's New" section mentions the guided onboarding coach feature
- [ ] #3 All screenshot paths in README.md point to existing files in docs/screenshots/
- [ ] #4 docs/onboarding-guide.md exists and follows the 6-step guided tour structure exactly (Environment, Flake, Builder, Cache, System, Agent)
- [ ] #5 Onboarding guide includes screenshots for each of the 6 coach steps showing both the coach panel checklist item and the destination page with contextual callouts
- [ ] #6 Onboarding guide includes screenshots of progressive field-level callouts in all 5 forms (Environment, Flake, Builder, Cache, System)
- [ ] #7 Onboarding guide includes screenshots of the agent deployment reminder modal and builder runtime reminder modal
- [ ] #8 Onboarding guide includes example NixOS configuration snippets for enabling the agent module on target systems
- [ ] #9 Onboarding guide explains each onboarding step in language appropriate for sysadmins with average Nix skills (no assumed expert knowledge)
- [ ] #10 Onboarding guide includes a "Before You Begin" section covering server requirements, database setup, and initial module configuration
- [ ] #11 Onboarding guide includes an "After Onboarding" section explaining next steps and how to reopen the coach panel
- [ ] #12 All screenshots used in documentation are sourced from the web-ui check flow or generated using the same environment for consistency
- [ ] #13 Documentation follows standard markdown formatting and renders correctly on GitLab
- [ ] #14 All external links in documentation are valid and point to correct resources
- [ ] #15 Onboarding guide includes troubleshooting section covering common first-time setup issues (coach not appearing, agent not connecting, etc.)
<!-- AC:END -->
