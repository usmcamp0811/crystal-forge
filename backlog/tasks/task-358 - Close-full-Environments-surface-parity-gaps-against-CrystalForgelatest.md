---
id: TASK-358
title: Close full Environments surface parity gaps against CrystalForgelatest
status: In Progress
assignee: []
created_date: '2026-06-14 18:56'
updated_date: '2026-06-14 19:10'
labels:
  - design-parity
  - environments
  - web-ui
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: high
ordinal: 301000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments user experience (/environments) has accumulated across partial tasks and ad-hoc changes, but there is no single execution record that guarantees the entire Environments surface matches the CrystalForgelatest design example end-to-end. Visual gaps, inconsistent stat strip, health bars, cache assignment, policy enforcement UI, and modal behavior can block acceptance.

## Goal
Bring the full Environments surface into parity with the CrystalForgelatest reference at `/environments`, so a reviewer can compare the implemented UI against the design example and find no material visual or interaction discrepancies on the core desktop flows.

## Non-Goals
- Environments sidebar surface (tracked separately by TASK-339)
- Backend refactors unrelated to parity-driven API/data needs (environment CRUD, cache assignment API, compliance bundles, gate policies)
- Mobile-first redesign beyond responsive behavior already implied by the reference
- Replacing authoritative backend data with mock-only UI shortcuts in production paths, except temporary mock/placeholder data explicitly authorized for parity gaps without current backend support; any such mocks must be tracked by follow-up Backlog tasks

## Acceptance Criteria
- [ ] Environments list header, subtitle (tiers, systems, caches count), and "Add environment" button materially match CrystalForgelatest on desktop
- [ ] Stat strip displays 5 metrics (Total tiers, Systems, Caches, Manual policy, Auto-sync off) with colored accent rails per the reference
- [ ] Filter bar with search, cards/table view toggle, and count text matches the reference
- [ ] Cards mode: env card with color rail, name/title with PROD badge, description, systems stat, flake chips, health bar (colored segments), health legend (with CVE count), KV grid (Deploy, Enforcement, Cache, Auto-sync, Approval), footer (role assignments, Edit button) — all materially match the reference
- [ ] Table mode columns (Environment, Systems, Health bar, Deploy, Enforcement, Cache, Auto-sync, Approval, row actions) match the design layout, typography, and spacing
- [ ] Add/Edit environment modal (name, color picker with presets and custom, description, cache assignment with dropdown and detail display, deployment mode selector, gate policy picker with search/multi-select, compliance bundle selector, production toggle, auto-sync/approval toggles, danger zone) functionally matches the reference
- [ ] Delete environment confirmation with type-to-confirm and systems guard matches the reference
- [ ] Loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering (except authorized temporary placeholders)
- [ ] All displayed values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps
- [ ] checks/web-ui captures screenshot evidence and behavior assertions for the full Environments surface (/environments)
- [ ] A human reviewer can compare the implemented Environments surface against the CrystalForgelatest reference and find no remaining material parity gaps

## Architectural Constraints
- No business logic in UI views
- Existing repository patterns first (e.g., shared Icon component, Dioxus patterns)
- New environments-related components go in packages/web-ui/src/components/environments/
- Color picker uses native HTML input[type=color] per the reference; present palette is a shortcut
- Cache assignment dropdown references cache destinations from the Caches view (may need cross-view data wiring)
- Gate policy picker and compliance bundle selector reference the Policies and Compliance views data (may need API endpoints)
- Any temporary mock data must be clearly commented in code and tracked by a follow-up Backlog task

## Verification Plan
- cargo fmt -- --check
- cargo clippy -- -D warnings
- cargo test (targeted: environments-related packages)
- Visual diff against the CrystalForgelatest reference for cards mode, table mode, and all modals
- checks/web-ui VM screenshot comparison (run on request)

## Impact Areas
- packages/web-ui/src/views/environments_list.rs
- packages/web-ui/src/views/environments.rs (env detail)
- packages/web-ui/src/components/environments/ (new sub-components)
- packages/web-ui/assets/app.css
- checks/web-ui/default.nix (if screenshot coverage needs expansion)
- packages/default/src/handlers/api/environments.rs (API changes if needed)
- packages/default/src/queries/environments.rs (query changes if needed)

## Risk Level
Medium — primarily UI component work; backend changes moderate if cache/policy/compliance data wiring is missing; overlaps with TASK-339 for sidebar surface (separate tracking).

## Dependencies
- TASK-339 (Environments sidebar surface umbrella — separate scope, but may share components)
- CrystalForgelatest reference: /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Backend (cheap data): new SQL view + query for per-environment rollups (health breakdown healthy/warning/critical/offline, CVE totals, flakes list) derivable from systems. New migration only. Extend EnvironmentSummary. Update query/handler. cargo sqlx prepare.
2. API model + adapter: extend EnvironmentItem with health breakdown, CVE total, flakes, and clearly-commented placeholder fields (deploy_policy, cache, auto_sync, requires_approval, is_production) for deferred backend.
3. UI list rewrite: header/subtitle, 5-metric stat strip, filter bar (search + cards/table seg + count), cards mode (color rail, PROD badge, health bar+legend, KV grid, footer), table mode.
4. Unified Add/Edit modal: color picker, description, cache dropdown (placeholder), deploy mode, gate policy picker, compliance selector (placeholder), production toggle, auto-sync/approval toggles, danger zone.
5. Delete confirmation: type-to-confirm + systems guard.
6. web-ui check: screenshot + assertions for /environments.
7. Tests: query mapping, adapter mapping, validation.
8. Follow-up Backlog tasks for deferred backend (cache assignment, gate policies, compliance bundles, RBAC, per-env deploy policy persistence, production flag persistence).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in ~/code/crystal-forge/TASK-358-environments-surface-parity

Scope decision (maintainer-approved 2026-06-14): (1) Real backend now for cheap data derivable from existing systems table (per-env health breakdown, CVE totals, flakes-per-env). Clearly-commented temporary placeholders + follow-up Backlog tasks for heavier features needing new schema: cache assignment, gate policies, compliance bundles, RBAC role counts, per-env deploy policy + production flag persistence. (2) Replace inline add form + split edit modals with single unified Add/Edit modal per reference; placeholder fields wired to follow-ups.
<!-- SECTION:NOTES:END -->
