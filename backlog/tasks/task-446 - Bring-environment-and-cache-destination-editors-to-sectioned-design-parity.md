---
id: TASK-446
title: Bring cache destination editor to sectioned design parity
status: Backlog
assignee: []
created_date: '2026-08-31 02:21'
updated_date: '2026-09-20 03:16'
labels:
  - web-ui
  - caches
  - modals
  - design-parity
dependencies: []
references:
  - ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - TASK-339.1
  - docs/design/CrystalForge/components/CachesView.jsx
  - docs/design/CrystalForge/styles.css
documentation:
  - docs/design/CrystalForge/components/CachesView.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/views/caches.rs
  - packages/web-ui/src/components/caches
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: enhancement
ordinal: 457000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Own the Cache destination Add/Edit editor only. Environment Add/Edit modal parity is canonical in TASK-339.1 and is not part of this task.

Align the cache destination editor with the committed `docs/design/CrystalForge/components/CachesView.jsx` and styles while preserving real cache types, credential handling, connection testing, environment assignment, authorization, validation, and deletion safeguards. Do not duplicate or reimplement the Environment modal. Coordinate only through the existing cache/environment assignment APIs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cache destination Add/Edit uses the sectioned Destination, Credentials, and Environments editor with authoritative field validation, section badges, and footer state.
- [ ] #2 S3-compatible, Attic, and Nix HTTPS fields preserve existing real credential creation, redaction, connection testing, and mutation error behavior.
- [ ] #3 Environment assignment uses the existing authorized cache/environment API and preserves hidden-environment behavior; it does not implement Environment Add/Edit modal behavior.
- [ ] #4 Delete safeguards and duplicate-submit, focus, Escape, backdrop, narrow, light, and dark behavior remain safe and accessible.
- [ ] #5 Focused cache editor workflows and semantic assertions cover create, edit, validation, connection failure, environment assignment, deletion, and responsive states.
<!-- AC:END -->
