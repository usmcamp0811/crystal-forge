---
id: TASK-264
title: Fix Systems Edit modal rendering regression with flake warning banner
status: Backlog
assignee: []
created_date: '2026-04-11 16:00'
labels:
  - bug
  - ui
  - systems
  - modal
milestone: Sprint
dependencies: []
references:
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/views/systems_list.rs
priority: high
ordinal: 5100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
On latest `dev`, opening **Systems → Edit** for an unlinked system (example: `nix-builder`) shows malformed modal behavior: the global flake-link warning banner and large portions of page content appear inside/under the modal flow instead of a proper isolated overlay.

Repro (reported):
1. Go to Systems view.
2. Click **Edit** on `nix-builder` (or any system missing flake link).
3. Observe edit modal content mixed with global warning/content (`Review affected systems`, nav/body content).

## Desired Outcome
Edit modal must render as a proper overlay container, with only modal content visible in modal body. Global page warning/layout content must remain outside modal and not leak into modal rendering/scroll area.
<!-- SECTION:DESCRIPTION:END -->
