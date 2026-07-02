---
id: TASK-373
title: Enforce builder CPU and RAM limits on systemd-scoped builds
status: To Do
assignee: []
created_date: '2026-06-27 04:04'
updated_date: '2026-06-28 02:14'
labels:
  - builder
  - resource-limits
  - systemd
  - cpu
  - memory
  - bug
  - high-priority
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/default/src/config/build.rs
  - packages/default/src/derivations/utils.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/models/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/web-ui/src/components/builders/add_builder_modal.rs
  - packages/web-ui/src/components/builders/edit_builder_modal.rs
  - packages/web-ui/src/views/builds.rs
documentation:
  - >-
    https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Builder records can show capacity/limit values in the UI, such as `16c · 96GB`, but those values do not currently appear to guarantee that build execution is constrained to that CPU/RAM budget. The actual build process is constrained by builder process configuration such as `build.systemd_memory_max`, which can diverge from the builder settings displayed and edited in the UI.

This is confusing and unsafe: if a builder is configured in Crystal Forge as `16 cores` and `96GB`, users expect builds launched for that builder to be prevented from exceeding those resources.

## Historical Context: v0.1.41 Behavior

A read-only comparison against tag `v0.1.41` found that resource limiting was originally config-driven and applied directly to `systemd-run`:

- `build.systemd_memory_max` was an actual memory value such as `"4G"` or `"2048M"`.
- `build.systemd_cpu_quota` was a CPU quota percentage such as `300` for roughly 3 cores worth.
- Build execution added systemd scope properties:
  - `MemoryMax=<build.systemd_memory_max>`
  - `CPUQuota=<build.systemd_cpu_quota>%`
- Actual builds were run via a scoped command equivalent to:
  - `systemd-run --scope --collect --quiet --property MemoryMax=... --property CPUQuota=...% -- nix-store --realise <drv>`
- Nix commands also received `--cores` and `--max-jobs` from build config.

In current code, the systemd resource-control mechanism still exists through `build.systemd_memory_max`, `build.systemd_cpu_quota`, and `apply_systemd_props_for_scope(...)`. However, current multi-builder UI/server records also contain `max_cpu_cores` and `max_memory_mb`, and the Builds UI displays those as values like `16c · 96GB`.

The apparent regression/confusion is that these newer persisted/UI builder limits look authoritative but do not appear to be wired into the systemd scope properties used to execute builds. The original systemd cap mechanism was not removed; the newer builder UI metadata appears disconnected from enforcement.

Important caveat: both old and current code have fallback paths where if `systemd-run` is unavailable or fails, execution may fall back to direct `nix`/`nix-store`. In direct execution mode, systemd `MemoryMax`/`CPUQuota` does not apply.

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
- Account for direct-execution fallback: either preserve current behavior with explicit documentation/warnings or prevent silent fallback when resource enforcement is required.

## Impact Areas

- Builder configuration model and persisted builder limit fields
- Builder job claim/execution path
- systemd-run scope property generation
- Direct-execution fallback behavior and warnings/errors
- Builders/Builds UI labels or help text if needed to reflect enforcement semantics
- Tests around build command construction/resource-limit application

## Risk Level

High: resource-limit enforcement protects builder host stability. Incorrect precedence, unit conversion, or fallback behavior could under-limit, over-limit, or fail builds unexpectedly.

## Verification Plan

- Add targeted tests for resource-limit derivation and unit conversion from builder RAM/CPU values.
- Add or update tests for the generated systemd-run properties for a builder configured with `16 cores` and `96GB` RAM.
- Verify that a builder without explicit persisted limits continues to use existing config/default behavior.
- Verify that memory units are unambiguous and not treated as percentages.
- Verify direct-execution fallback behavior is explicit and cannot silently bypass required resource enforcement without a warning/error.
- Run appropriate Rust checks/tests for the affected crate, for example:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo test --manifest-path packages/default/Cargo.toml <targeted resource-limit tests>`
  - `nix develop -c cargo check --manifest-path packages/default/Cargo.toml`
- If UI help text/labels are changed, run the relevant web UI check and include screenshot evidence in the MR.

## Proposed Approach

1. Trace how a builder's `max_cpu_cores` and `max_memory_mb` values are stored, fetched, and made available to the build execution path.
2. Compare current behavior to `v0.1.41`, where `build.systemd_memory_max` and `build.systemd_cpu_quota` were directly converted into `MemoryMax` and `CPUQuota` systemd properties.
3. Determine the authoritative precedence between persisted builder limits and static `build.systemd_*` config.
4. Apply the selected builder's memory limit to the systemd build scope so a `96GB` builder limit becomes an actual memory cap.
5. Apply the selected builder's CPU limit consistently, including the systemd CPU budget and any relevant Nix per-build core setting if needed.
6. Decide and document what happens if systemd scope creation fails when resource enforcement is required.
7. Update UI labels/help text only if necessary so the displayed values accurately communicate enforced limits.
8. Add tests for enforced limits, fallback behavior, and unit conversion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A builder configured with a RAM limit, such as 96GB, has that limit enforced for systemd-scoped builds assigned to that builder.
- [ ] #2 A builder configured with a CPU/core limit has that limit enforced for systemd-scoped builds assigned to that builder.
- [ ] #3 Memory limits are treated as actual memory values, not percentages.
- [ ] #4 Builders without explicit persisted CPU/RAM limits continue to use existing config/default behavior.
- [ ] #5 The precedence between persisted builder limits and static builder process config is deterministic and documented in code or user-facing help text.
- [ ] #6 Direct-execution fallback behavior cannot silently bypass required resource enforcement; it is explicitly documented, warned, or rejected according to the chosen policy.
- [ ] #7 Tests cover resource-limit generation for explicit builder limits, fallback behavior for missing limits, and behavior when systemd enforcement is unavailable.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Read-only comparison with `v0.1.41`: original resource limits were config-driven and actually applied as `MemoryMax`/`CPUQuota` properties on `systemd-run`. Current code still has that config-driven path, but newer builder UI/server fields `max_cpu_cores` and `max_memory_mb` appear to be persisted/display metadata and were not verified as inputs to systemd scope creation. Task description updated with this historical context and fallback caveat.
<!-- SECTION:NOTES:END -->
