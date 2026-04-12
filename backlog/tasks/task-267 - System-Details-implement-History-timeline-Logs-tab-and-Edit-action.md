---
id: TASK-267
title: 'System Details: implement History timeline, Logs tab, and Edit action'
status: To Do
assignee: []
created_date: '2026-04-12 18:24'
updated_date: '2026-04-12 18:25'
labels:
  - ui
  - systems
  - audit
  - observability
milestone: System Details Hardening
dependencies: []
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The System Details view currently has placeholder/static content for **History** and **Logs** tabs, which prevents admins and auditors from understanding what configuration was deployed to a system over time and when deployment-related activity occurred.

## Goal
Implement System Details so an admin/auditor can answer:
- What was deployed when?
- Did the system revert to a prior configuration?
- Which flake/config/commit was active at each point in time?
- What agent events occurred around deployments?

## User Intent
As an admin/auditor, I need a trustworthy per-system timeline and event log that demonstrates control over configuration changes and deployment behavior.

## Scope (In)
1. **History tab**
   - Replace static content with a vertical timeline sourced from **system states**.
   - Show, per entry: timestamp, active configuration identity, flake/commit linkage, actor/reason context, and status/outcome metadata.
   - Support easy navigation to the corresponding flake context (external/open-in-new action acceptable).
   - Make reverts visually and semantically identifiable (when configuration returns to a previous state).

2. **Logs tab**
   - Replace static content with **agent events only** (v1 scope).
   - Prioritize readability of deployment-related moments (event type/time ordering/filtering for deployment events where relevant).

3. **System Details top-right action**
   - Add **Edit** button matching behavior of existing Systems table Edit flow (same modal/fields/validation pattern).

4. **Data plumbing**
   - Add/extend backend/API/query plumbing only as needed to support the above UI without introducing unrelated refactors.

## Non-Goals
- Full multi-source event correlation (server + agent) beyond agent events for this task.
- Redesigning unrelated System Details sections.
- Broad navigation or IA refactors.

## Architectural Constraints
- UI should remain presentation-focused; data shaping/business rules should live in appropriate domain/api layers.
- Follow existing Systems page and modal patterns for consistency.
- Keep scope minimal; no unrelated refactors.

## Verification Plan
- Add/extend tests for timeline rendering and data mapping (including revert scenario).
- Add/extend tests for Logs tab rendering of agent events and ordering.
- Add/extend test for Edit button parity with Systems table flow.
- Run targeted frontend/backend tests plus relevant Nix check(s) covering System Details behavior.

## Impact Areas
- System Details UI components (History, Logs, header actions)
- Existing edit modal invocation path for Systems
- API/query layer for system history and agent events (if needed)

## Risk Level
Medium (crosses UI + possible API plumbing, but constrained to one view/flow).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 History tab shows a vertical timeline per system based on system-state transitions (not static placeholder content).
- [ ] #2 Each history entry includes: timestamp, active configuration identity, flake/commit linkage, actor/reason context, and status/outcome metadata.
- [ ] #3 History timeline enables quick navigation to related flake context for an entry (e.g., open link action).
- [ ] #4 History timeline makes configuration reverts detectable (same/previous config reappears) and visually understandable.
- [ ] #5 Logs tab renders real agent events for the selected system (v1: agent events only) in chronological order and is no longer static placeholder content.
- [ ] #6 Logs tab makes deployment moments easy to identify (through event typing, grouping, filtering, or clear visual emphasis).
- [ ] #7 System Details includes a top-right Edit button that uses the same edit flow/modal semantics as the Systems table Edit action.
- [ ] #8 Existing Systems table Edit behavior remains unchanged and consistent after adding the System Details Edit entry point.
- [ ] #9 Implementation follows existing architecture boundaries (no business logic in view components; minimal necessary API/domain plumbing only).
- [ ] #10 Verification includes tests that cover: (a) history rendering with revert case, (b) logs rendering from agent events, and (c) system edit action parity from details view.
<!-- AC:END -->
