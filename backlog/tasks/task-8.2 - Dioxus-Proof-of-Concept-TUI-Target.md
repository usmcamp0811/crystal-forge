---
id: TASK-8.2
title: Dioxus Proof of Concept - TUI Target
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - poc
  - tui
dependencies:
  - TASK-8.1
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build and validate a minimal TUI application using Dioxus to prove terminal interface works.

Steps:
1. Create tui/ directory with Cargo.toml
2. Add dioxus and dioxus-tui dependencies
3. Create main.rs with same counter component as web
4. Build: cargo build --bin cf-tui
5. Run in terminal: cargo run --bin cf-tui
6. Test keyboard navigation (Tab, Enter, q to quit)
7. Measure binary size: ls -lh target/release/cf-tui
8. Document any rendering issues or limitations

Expected: TUI renders correctly, binary < 10MB, keyboard nav works
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 TUI app runs in terminal
- [ ] #2 Counter works with keyboard input
- [ ] #3 Binary size documented and < 10MB
- [ ] #4 Keyboard navigation functional
- [ ] #5 No crashes or rendering glitches
<!-- AC:END -->
