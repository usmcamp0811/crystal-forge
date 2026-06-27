---
id: TASK-373
title: Enforce builder CPU and RAM limits on systemd-scoped builds
status: Backlog
assignee: []
created_date: '2026-06-27 04:04'
labels:
  - builder
  - resource-limits
  - systemd
  - cpu
  - memory
  - bug
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/default/src/config/build.rs
  - packages/default/src/derivations/utils.rs
  - packages/default/src/bin/builder.rs
  - packages/web-ui/src/components/builders/add_builder_modal.rs
  - packages/web-ui/src/components/builders/edit_builder_modal.rs
  - packages/web-ui/src/views/builds.rs
documentation:
  - >-
    https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Builder records can show capacity/limit values in the UI, such as `16c · 96GB`, but those values do not currently appear to guarantee that build execution is constrained to that CPU/RAM budget. The actual build process is constrained by builder process configuration such as `build.systemd_memory_max`, which can diverge from the builder settings displayed and edited in the UI.

This is confusing and unsafe: if a builder is configured in Crystal Forge as `16 cores` and `96GB`, users expect builds launched for that builder to be prevented from exceeding those resources.

## Desired Outcome

When a builder has CPU and RAM limits defined, systemd-scoped build runs for that builder should enforce those limits so a build cannot use more than the configured memory or CPU budget.

## Goal

Make builder CPU/RAM settings authoritative for build execution resource limits, or clearly establish a single source of truth that the UI and builder execution both use.

## Non-Goals

- Redesigning the Builders or Builds views
- Changing queue scheduling policy beyond using the selected builder's resource limits
- Replacing systemd-scoped execution
- Adding a new resource-management subsystem
- Changing unrelated Nix build settings unless required to align with the enforced CPU limit

## Architectural Constraints

- Keep resource-limit enforcement in the builder execution path, not in UI-only logic.
- Do not rely on display-only fields to imply enforcement.
- Preserve existing `build.systemd_memory_max` behavior unless explicitly superseded with a documented compatibility path.
- Avoid hidden global mutable state.
- If both static config and persisted builder limits exist, define deterministic precedence and document it.
- Ensure limits are applied per build scope/job and do not accidentally cap the entire Crystal Forge builder process.
- Follow existing config, API, and builder execution patterns.

## Impact Areas

- Builder configuration model and persisted builder limit fields
- Builder job claim/execution path
- systemd-run scope property generation
- Builders/Builds UI labels or help text if needed to reflect enforcement semantics
- Tests around build command construction/resource-limit application

## Risk Level

Medium: resource-limit enforcement can affect build reliability and host stability. Incorrect precedence or unit conversion could under-limit, over-limit, or fail builds unexpectedly.

## Verification Plan

- Add targeted tests for resource-limit derivation and unit conversion from builder RAM/CPU values.
- Add or update tests for the generated systemd-run properties for a builder configured with `16 cores` and `96GB` RAM.
- Verify that a builder without explicit persisted limits continues to use existing config/default behavior.
- Verify that memory units are unambiguous and not treated as percentages.
- Run appropriate Rust checks/tests for the affected crate, for example:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo test --manifest-path packages/default/Cargo.toml <targeted resource-limit tests>`
  - `nix develop -c cargo check --manifest-path packages/default/Cargo.toml`
- If UI help text/labels are changed, run the relevant web UI check and include screenshot evidence in the MR.

## Proposed Approach

1. Trace how a builder's `max_cpu_cores` and `max_memory_mb` values are stored, fetched, and made available to the build execution path.
2. Determine the authoritative precedence between persisted builder limits and static `build.systemd_*` config.
3. Apply the selected builder's memory limit to the systemd build scope so a `96GB` builder limit becomes an actual memory cap.
4. Apply the selected builder's CPU limit consistently, including the systemd CPU budget and any relevant Nix per-build core setting if needed.
5. Update UI labels/help text only if necessary so the displayed values accurately communicate enforced limits.
6. Add tests for enforced limits, fallback behavior, and unit conversion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A builder configured with a RAM limit, such as 96GB, has that limit enforced for systemd-scoped builds assigned to that builder.
- [ ] #2 A builder configured with a CPU/core limit has that limit enforced for systemd-scoped builds assigned to that builder.
- [ ] #3 Memory limits are treated as actual memory values, not percentages.
- [ ] #4 Builders without explicit persisted CPU/RAM limits continue to use existing config/default behavior.
- [ ] #5 The precedence between persisted builder limits and static builder process config is deterministic and documented in code or user-facing help text.
- [ ] #6 Tests cover resource-limit generation for explicit builder limits and fallback behavior for missing limits.
<!-- AC:END -->
