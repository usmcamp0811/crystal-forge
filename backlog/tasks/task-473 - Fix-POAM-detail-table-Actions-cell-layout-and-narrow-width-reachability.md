---
id: TASK-473
title: Fix POA&M detail table Actions cell layout and narrow-width reachability
status: Backlog
assignee: []
created_date: '2026-10-03 00:23'
labels:
  - poam
  - web-ui
  - css
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
modified_files:
  - packages/web-ui/assets/app.css
priority: low
type: bug
ordinal: 494000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In the POA&M detail drawer tables (linked findings, linked vulnerabilities, retired history), the Actions cell uses display:flex directly on a td. That removes its table-cell behavior, so its row borders no longer line up with the other cells. At narrow widths (about 560px) the Evidence and unlink actions sit behind horizontal scroll inside the table wrapper, so they are easy to miss. Both conditions existed before the POA&M detail design-parity pass and were found while inspecting its screenshots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Row borders are continuous across all cells, including the Actions cell, in dark and light themes.
- [ ] #2 At 560px width the Evidence and unlink actions are reachable without discovering a hidden horizontal scroll, or the table makes the scroll affordance clear.
- [ ] #3 Action behavior, authorization, and test ids are unchanged.
- [ ] #4 A browser capture at desktop and narrow widths shows the corrected rows.
<!-- AC:END -->
