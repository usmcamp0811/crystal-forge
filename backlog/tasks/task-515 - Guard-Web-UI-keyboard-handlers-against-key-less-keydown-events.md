---
id: TASK-515
title: Guard Web UI keyboard handlers against key-less keydown events
status: Backlog
assignee: []
created_date: '2026-11-02T13:37:00Z'
labels:
  - web-ui
  - wasm
dependencies: []
priority: medium
type: bug
ordinal: 566000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Chromium dispatches `keydown` without `key` for autofill and `<datalist>` picks. Dioxus 0.7.3 `KeyboardData::key` reads `key` through a non-catching web-sys getter and panics, which stops the whole Dioxus runtime (`RefCell already borrowed` follows). TASK-508 fixed the Add/Edit System configuration field and the registration dialog (`dom_key` in `components/forms/add_system_form.rs`). About 88 other `e.key()` handlers remain; any that can receive autofill/datalist events (login, search inputs, dialogs containing such inputs) can freeze the app the same way.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A shared safe key read replaces KeyboardData::key in handlers reachable by autofill or datalist keydown events
- [ ] #2 A key-less keydown dispatched on every autofill-capable input does not panic the app in a browser test
<!-- AC:END -->
