---
id: TASK-397
title: Evaluation errors silently drop systems from nixosConfigurations count
status: In Progress
assignee: []
created_date: '2026-07-24 00:29'
updated_date: '2026-07-24 03:22'
labels:
  - evaluator
  - reporting
  - ux
dependencies:
  - TASK-398
priority: high
type: bug
ordinal: 396000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When a `nixosConfiguration` fails to evaluate (e.g. due to a NixOS module type error), Crystal Forge silently drops the system entirely from the evaluation results. It does not appear as a failed system — it simply disappears from the total count.

**Observed behavior:**
- UI header shows "Systems: 12" (Crystal Forge discovered 12 nixosConfigurations in the flake)
- Evaluation summary shows "Total: 4 nixosConfigurations" and "Successful: 4"
- `nix-builder-1` is completely absent from both the success list and any failure list
- The log shows an ERROR for the system (`services.timesyncd.enable` not of type boolean, caused by a double-wrapping bug in `mkStigModule`/`overrideAttrs`) but Crystal Forge does not record it as a failed system

**Expected behavior:**
- Systems that fail to evaluate should appear as **failed** in the evaluation summary with their error
- "Total" should reflect all discovered systems (12), not just the ones that returned a derivation (4)
- A user should never have to debug why a system "disappeared" — the error and system name should be surfaced clearly

## Root Cause (nix-builder-1 specifically)

The immediate cause for `nix-builder-1` is a bug in `crystal-forge`'s `mkStigModule` helper (`lib/stig/default.nix:100`):

```nix
overrideAttrs = attrs: mapAttrsRecursive (_: v: mkOverride 1000 v) attrs;
```

When `stigConfig` already contains `mkForce` calls (e.g. `services.timesyncd.enable = mkForce true` in the timesyncd stig module), `overrideAttrs` double-wraps the value — producing an override-of-an-override attrset instead of a plain boolean. The NixOS module system then rejects `services.timesyncd.enable` as not being of type `boolean`.

However, this bug exposed a **separate Crystal Forge reporting bug**: a system that crashes the evaluator should still be recorded as a failed/errored system, not silently removed from the total.

## Evidence

```
# CF evaluation log — error is logged but system is not counted:
[ERROR] error: A definition for option `services.timesyncd.enable' is not of type `boolean'.
        - In `.../stig-modules/modules/timesyncd/default.nix':
            { _type = { _type = "override"; content = "override"; priority = 1000; ... }

# Summary — nix-builder-1 not mentioned anywhere:
✅ Successful: 4 systems
📦 Total: 4 nixosConfigurations   ← should be at least 5 (4 success + 1 failed)

# The system evaluates fine locally (no stig-modules conflict in local store):
nix eval .#nixosConfigurations.nix-builder-1.config.system.build.toplevel
→ «derivation /nix/store/clx6f...-nixos-system-nix-builder-1-26.05...drv»
```

## Scope

Two separate fixes are needed:

1. **Crystal Forge evaluator** (this task): When `nix-eval-jobs` or the evaluation wrapper catches an error for a specific system, record it as a failed system with the error message rather than omitting it from output entirely. The "Total" count should equal successes + failures, matching the discovered system count.

2. **`mkStigModule` double-wrap bug** (separate task): `overrideAttrs` should strip existing `mkOverride`/`mkForce` wrappers before re-wrapping, or `stigConfig` values should not use `mkForce` internally since `overrideAttrs` already applies `mkOverride 1000`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems that fail to evaluate appear as failed/errored entries in the evaluation summary with their system name and error message
- [ ] #2 The 'Total' nixosConfigurations count in the evaluation summary equals successes + failures (matches the number of systems discovered in the flake)
- [ ] #3 A system that crashes the evaluator never silently disappears — it is always accounted for in either the success or failure column
- [ ] #4 The evaluation UI shows nix-builder-1 (and similar systems) as failed with the root cause error, not absent
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Sprint readiness notes

**What this task is:** A Crystal Forge server-side evaluator bug. When `nix-eval-jobs` returns an error for a specific `nixosConfiguration`, the evaluator discards the system entirely instead of recording it as a named failure. The user sees fewer systems in "Total" than were discovered, with no indication which ones failed or why.

**Where to look:** The evaluation pipeline in the Crystal Forge server — specifically the code that:
1. Enumerates `nixosConfigurations` from the flake (this works — UI showed "Systems: 12")
2. Hands each system to `nix-eval-jobs` for evaluation
3. Collects results and builds the summary

Step 3 is where the bug lives: errors from `nix-eval-jobs` for a specific system are logged (the ERROR lines are visible in the log) but not added to any failed-systems list, so they fall out of the total count.

**Concrete trigger:** `nix-builder-1` in `ATALLC/nix-config` fails evaluation with:
```
error: A definition for option `services.timesyncd.enable' is not of type `boolean'
```
This error appears in the Crystal Forge log but `nix-builder-1` is absent from both success and failure lists.

**Dependency:** TASK-398 fixes the underlying Nix evaluation error for `nix-builder-1`. Once that is fixed, this task's fix can be verified end-to-end. However, this task's fix (surface the failure, don't drop it) is independent and should be implemented regardless.

**Verification:** Trigger a Crystal Forge evaluation of `ATALLC/nix-config`. Before TASK-398 is merged, `nix-builder-1` should appear as a **named failed system** with its error. After TASK-398, it should appear as a success.
<!-- SECTION:NOTES:END -->
