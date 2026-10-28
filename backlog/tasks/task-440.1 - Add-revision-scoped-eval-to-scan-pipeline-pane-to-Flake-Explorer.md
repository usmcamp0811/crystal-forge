---
id: TASK-440.1
title: Add revision-scoped eval-to-scan pipeline pane to Flake Explorer
status: Backlog
assignee: []
created_date: '2026-09-09 03:32'
labels:
  - flakes
  - pipeline
  - web-ui
  - api
  - design-parity
  - observability
dependencies:
  - TASK-246
  - TASK-326.1
  - TASK-337
references:
  - git commit e1b7434899e23f43770632e59d80a76a8fc8459e
documentation:
  - docs/design/CrystalForge/components/FlakeExplorer.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/data-flake-explorer.js
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/default/crates/cf-server/src/handlers/api/
  - packages/default/crates/cf-server/src/queries/
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-440
priority: high
type: feature
ordinal: 474000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design commit `e1b74348` adds a Pipeline pane to the Flake Explorer for one selected full revision. The pane combines authoritative per-configuration evaluation, build, cache-presence, and vulnerability-scan state so operators can understand why a revision is deployable or blocked. Implement this as a post-TASK-440 follow-up. Consume existing immutable revision artifacts and the cache and scan contracts from its dependencies; browsing must remain bounded, side-effect free, visibility scoped, and free of synthetic status values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The selected full flake revision governs every Pipeline count row action and deep link and changing revisions cannot leave stale data visible
- [ ] #2 A bounded visibility-scoped API reports each declared configuration's evaluation build cache and scan state from authoritative records without launching evaluation build cache verification or scanning on read
- [ ] #3 The Pipeline funnel reports evaluated built cached and scanned totals with explicit failed in-flight evicted unverified blocked and findings counts
- [ ] #4 Per-configuration rows distinguish every design state including failed evaluation blocked build partial cache presence waiting scan and unavailable data without collapsing unknown into success
- [ ] #5 Cache observations identify each configured destination result and observed timestamp and display push receipts separately from later presence verification
- [ ] #6 All Blocked In flight and Deployed filters use revision-global counts and reset predictably when the selected revision changes
- [ ] #7 Blocked summaries explain the exact build or cache reason and Rebuild and push Open scanning Build log Scan log and Open config actions preserve flake configuration and full revision context
- [ ] #8 The Pipeline tab alert count reflects scan-blocked configurations and hidden systems or environments cannot influence counts or be disclosed
- [ ] #9 Root revisions missing artifacts unsupported legacy records loading empty partial and API failure states remain explicit and browsing stays bounded and side-effect free
- [ ] #10 Focused aggregation authorization revision-identity and no-side-effect tests plus the authoritative web-ui check pass with light dark narrow keyboard navigation and screenshot coverage
<!-- AC:END -->
