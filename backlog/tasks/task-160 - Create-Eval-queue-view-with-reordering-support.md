---
id: TASK-160
title: Create Eval queue view with reordering support
status: Backlog
assignee: []
created_date: '2026-03-02 16:22'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a dedicated Eval view that mirrors the Build view functionality, allowing users to:
1. See the evaluation queue in order
2. Reorder the queue from the UI if desired
3. View evaluation logs (move the eval logs currently shown on Flake view to this new location)

The chip/button that currently shows eval logs on the Flake view should redirect to the location on the Evals view.

## Problem

Currently, evaluation logs are only accessible view via a chip. from the Flake There is no centralized place to:
- See all pending/running evaluations
- Monitor evaluation progress across multiple flakes/commits
- Reorder evaluation priority
- Access eval logs in a consistent location

## Desired Outcome

Create a new "Evals" view (similar to "Builds" view) that:
- Shows all queued and active evaluations
- Allows drag-and-drop or manual reordering of the queue
- Displays per-system status chips (pending → evaluating → success/failed)
- Provides access to evaluation logs from this view
- The Flake view eval log chip redirects to the appropriate location in Evals view

## Impact Areas

- New Evals view/page in web-ui
- Evaluation queue management (backend + frontend)
- Move eval log modal from Flake view to Evals view
- Navigation updates (sidebar, routing)
- Backend API for queue ordering
<!-- SECTION:DESCRIPTION:END -->
