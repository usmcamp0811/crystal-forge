---
id: TASK-482
title: Curate and enrich migrated OKF knowledge corpus
status: Backlog
assignee: []
created_date: '2026-10-05 16:14'
updated_date: '2026-10-05 16:15'
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
The OKF migration produced a useful current corpus, but the one-time audit against baseline 3b23d36f reports 214 unresolved source-block fidelity findings. These mostly mean the new concepts contain less detail than their pre-OKF sources; they are not evidence that the current corpus is invalid. Representative affected areas include architecture, API, derivation lifecycle, builder API, onboarding, SQL views, STIG, and testing. Treat findings as curation leads, not a requirement to reproduce every historical document. Review based on usefulness and current correctness. Keep process, temporary, historical, duplicate, and obsolete material out of durable knowledge unless it has independent reference value. Never depend on baseline 3b23d36f or require C-NNN exceptions for routine future documentation changes.
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
