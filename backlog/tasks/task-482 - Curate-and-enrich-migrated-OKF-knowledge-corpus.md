---
id: TASK-482
title: Curate and enrich migrated OKF knowledge corpus
status: Backlog
assignee: []
created_date: '2026-10-05 16:14'
labels:
  - knowledge
  - curation
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/332'
priority: medium
type: docs
ordinal: 503000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The OKF migration established a useful current knowledge corpus but did not carry every detail from every pre-migration Markdown source. The one-time audit against baseline 3b23d36f identified remaining source-block fidelity differences. These findings are curation leads, not an invariant that every historical source must be copied into OKF. Future curation should keep useful current guidance complete, retain genuinely useful design/history with clear status, map duplicate material to a complete canonical destination, and leave process or temporary material out of durable knowledge. Do not depend on the old baseline or require C-NNN exceptions for routine future documentation changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Review remaining source-fidelity findings based on value and current correctness
- [ ] #2 Restore useful current operator developer API architecture and workflow details that are missing from canonical concepts
- [ ] #3 Keep approved design rationale and useful historical material clearly labeled
- [ ] #4 Keep duplicate sources mapped only when the canonical destination preserves their useful meaning
- [ ] #5 Keep task process temporary and obsolete material out of durable knowledge unless it has independent reference value
- [ ] #6 Do not require comparison with baseline 3b23d36f for ordinary future documentation changes
- [ ] #7 Document curation decisions and verify current OKF validation after edits
<!-- AC:END -->
