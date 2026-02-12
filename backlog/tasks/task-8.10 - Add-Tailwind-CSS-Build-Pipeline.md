---
id: TASK-8.10
title: Add Tailwind CSS Build Pipeline
status: To Do
assignee: []
created_date: '2026-02-11 10:00'
labels:
  - ui
  - tooling
  - css
dependencies:
  - TASK-8.1
  - TASK-8.9
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Set up a proper Tailwind CSS build pipeline for the Dioxus web UI, replacing the initial CDN approach used in the PoC (TASK-8.1).

Steps:
1. Add tailwindcss CLI to the Nix dev shell (or use standalone binary)
2. Create tailwind.config.js with:
   - Content paths pointing to Dioxus RSX files (src/ui/**/*.rs)
   - Dark mode: 'class' (or 'media' - choose one)
   - Custom colors for status/severity (from TASK-8.4)
3. Create input.css with Tailwind directives (@tailwind base/components/utilities)
4. Configure Trunk.toml to run Tailwind build as a pre-build hook:
   - Input: input.css → Output: dist/tailwind.css
   - Purge unused classes for production builds
5. Verify hot reload still works (trunk serve should rebuild CSS on changes)
6. Measure production CSS bundle size (should be < 50kb after purging)
7. Remove CDN link from index.html

Architecture notes:
- Trunk supports pre-build hooks via [[hooks]] in Trunk.toml
- Tailwind needs to scan .rs files for class strings (content: ["./src/**/*.rs"])
- Consider using the standalone Tailwind CLI binary (no Node.js dependency)

Expected: Tailwind CSS is built from source, tree-shaken for production, integrated with trunk serve
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tailwind CSS builds from source (no CDN dependency)
- [ ] #2 Trunk.toml configured with Tailwind pre-build hook
- [ ] #3 Hot reload works for CSS changes
- [ ] #4 Production build tree-shakes unused classes
- [ ] #5 CSS bundle < 50kb after purging
- [ ] #6 No Node.js dependency required (standalone CLI)
<!-- AC:END -->
