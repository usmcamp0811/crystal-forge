---
id: doc-1
title: Task Template
type: other
created_date: '2026-02-19 02:43'
---
# Title

<!-- Short, specific, outcome-focused title.
Example: Extract SystemCard into reusable component.
Avoid vague titles like "Refactor stuff".
-->

---

# Status

Backlog

---

# Problem Statement

<!--
Describe the problem in plain language.
What is wrong, missing, unclear, or inefficient?
Why does this matter?
-->

---

# Goal

<!--
What should be true after this task is complete?
Describe the desired outcome, not the implementation.
-->

---

# Non-Goals

<!--
Explicitly state what this task does NOT include.
Prevents scope creep.
Example:
- No styling changes
- No API changes
- No database changes
-->

---

# Acceptance Criteria

<!--
All criteria must be objective and testable.
If this list is unclear, the task is not ready for To Do.
-->

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3

---

# Architectural Constraints

<!--
Define required boundaries.
Example:
- No business logic in UI
- Must not introduce global state
- Must follow existing service pattern
-->

---

# Implementation Notes

<!--
Optional hints or constraints.
Not required for execution, but helpful.
Avoid prescribing exact code unless necessary.
-->

---

# Verification Plan

<!--
Define how completion will be verified.
This should match your pre-flight and MR verification steps.
-->

Automated:
- nix flake check
- cargo test
- cargo clippy -- -D warnings

Manual:
- Describe manual testing steps

---

# Impact Analysis

<!--
What areas of the system are affected?
- UI
- API
- Domain
- Infrastructure
- Database
-->

---

# Risk Level

Low | Medium | High

<!--
Explain why.
-->

---

# Dependencies

<!--
List blocking tasks.
Leave empty if none.
-->

---

# Follow-Up Work

<!--
Potential improvements discovered but intentionally excluded.
Must become separate Backlog tasks.
-->
