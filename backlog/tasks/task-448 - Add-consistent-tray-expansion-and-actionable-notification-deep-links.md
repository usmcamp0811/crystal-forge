---
id: TASK-448
title: Add consistent tray expansion and actionable notification deep links
status: To Do
assignee: []
created_date: '2026-08-31 02:22'
updated_date: '2026-08-31 02:23'
labels:
  - web-ui
  - drawers
  - notifications
  - navigation
  - design-parity
dependencies:
  - TASK-325
  - TASK-433
  - TASK-440
  - TASK-441
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/321'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/314'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/322'
documentation:
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/app.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/src/state/navigation_focus.rs
  - packages/web-ui/src/alerts/mod.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/cves.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: high
type: enhancement
ordinal: 459000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring the cross-surface interaction changes from design commit ac582592 into the Rust frontend after the active CVE, policy, configuration, and notification work merges. Detail trays should share an accessible expand/restore behavior; notifications must navigate to the exact real entity they describe; and one-shot attention flashes must stop as soon as the user interacts rather than obstructing clickable targets.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build evaluation flake CVE policy compliance bundle and compliance evidence trays expose a consistent Expand and Restore control that widens content without covering classification banners or losing the selected item
- [ ] #2 Expanded state preserves each tray's tab selection filters scroll context and in-flight loading or error state and closing restores focus to the invoking control
- [ ] #3 Tray expansion works at desktop and narrow widths and remains accessible through keyboard controls labels Escape order and reduced-motion preferences
- [ ] #4 Notification records carry stable typed navigation metadata for their real build evaluation CVE system policy or POA&M target instead of discarding detail after route-prefix matching
- [ ] #5 Selecting a notification opens the exact accessible entity and appropriate detail surface while hidden deleted stale or unauthorized targets follow existing non-disclosing fallback behavior
- [ ] #6 CVE deep links support opening an exact vulnerability after the merged TASK-325 state without relying only on a list query string
- [ ] #7 Build evaluation system and policy deep links integrate with the merged navigation-focus and URL-backed state from TASK-440 without leaving stale focus on later navigation
- [ ] #8 Attention-flash animation stops immediately on pointer keyboard or wheel interaction and still honors one-shot acknowledgement and reduced-motion behavior
- [ ] #9 No demo-only notification target or fabricated focus payload is introduced into production responses
- [ ] #10 Focused model navigation and interaction tests plus the authoritative web-ui check pass with exact-target fallback expand/restore focus keyboard reduced-motion light dark and narrow assertions and screenshots
<!-- AC:END -->
