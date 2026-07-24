---
id: TASK-398
title: mkStigModule overrideAttrs double-wraps mkForce values causing type errors
status: Review
assignee:
  - claude-sonnet-4-6
created_date: '2026-07-24 00:29'
updated_date: '2026-07-24 00:45'
labels:
  - stig
  - nix
  - evaluator
dependencies: []
references:
  - 'lib/stig/default.nix:100'
  - 'modules/nixos/stig-modules/modules/timesyncd/default.nix:18'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/308'
modified_files:
  - lib/stig/default.nix
priority: high
type: bug
ordinal: 397000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`mkStigModule` in `lib/stig/default.nix` applies `mkOverride 1000` to every leaf value in `stigConfig` via `overrideAttrs`. When a `stigConfig` already contains `mkForce` (which is `mkOverride 50`) calls, the result is a double-wrapped override attrset — `mkOverride 1000 (mkForce true)` — rather than a plain boolean. NixOS module system then rejects the option as not being of type `boolean`.

## Location

`lib/stig/default.nix:100` in the `crystal-forge` flake (nix store path seen in evaluations: `/nix/store/28lss8qqhsrwzlg3qvr41can182bqkdh-v7vq3wkqh9712i1ym9hmh9mmjm0v8ppk-source/`):

```nix
overrideAttrs = attrs: mapAttrsRecursive (_: v: mkOverride 1000 v) attrs;
```

The timesyncd stig module (`modules/nixos/stig-modules/modules/timesyncd/default.nix:18`) passes this into `stigConfig`:

```nix
stigConfig = {
  services.timesyncd.enable = mkForce true;  # ← mkForce inside stigConfig
  services.timesyncd.extraConfig = ''...'';
};
```

`overrideAttrs` then produces:
```nix
services.timesyncd.enable = mkOverride 1000 (mkForce true);
# = { _type = "override"; priority = 1000; content = { _type = "override"; priority = 50; content = true; }; }
# NOT a boolean → NixOS module system error
```

## Impact

Any system that enables the `timesyncd` stig module (directly or via a stig preset) will fail to evaluate with:
```
error: A definition for option `services.timesyncd.enable' is not of type `boolean'
```

This currently affects `nix-builder-1` (uses `crystal-forge.stig-presets.off.enable = true`). Other systems with stig presets enabled may be affected too.

## Fix Options

**Option A (recommended):** Remove `mkForce` from `stigConfig` values in stig modules — since `overrideAttrs` already applies `mkOverride 1000` (higher priority than `mkForce`'s 50), the `mkForce` is redundant and harmful:

```nix
# modules/nixos/stig-modules/modules/timesyncd/default.nix
stigConfig = {
  services.timesyncd.enable = true;  # let overrideAttrs handle the priority
  services.timesyncd.extraConfig = ''...'';
};
```

**Option B:** Make `overrideAttrs` unwrap existing override values before re-wrapping:

```nix
overrideAttrs = attrs: mapAttrsRecursive (_: v:
  mkOverride 1000 (if v ? _type && v._type == "override" then v.content else v)
) attrs;
```

Option A is simpler and more correct — `stigConfig` values should be plain Nix values; the override priority is the responsibility of `mkStigModule`, not the individual stig modules.

## Verification

After fix, `nix eval .#nixosConfigurations.nix-builder-1.config.system.build.toplevel` should succeed in the Crystal Forge evaluator environment (not just locally), and the system should appear in the Crystal Forge evaluation summary.

Also audit all other stig modules under `modules/nixos/stig-modules/` for `mkForce` usage inside `stigConfig` blocks and remove them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 overrideAttrs in mkStigModule does not produce nested override attrsets when stigConfig contains mkForce values
- [ ] #2 All stig modules that use mkForce inside stigConfig blocks are updated to use plain values
- [ ] #3 nix-builder-1 evaluates successfully in the Crystal Forge evaluator (not just locally) and appears in the evaluation summary
- [ ] #4 No regression: stig controls still apply at the correct priority (overriding user config)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Approach: Option B — fix `overrideAttrs` in `lib/stig/default.nix`

After auditing all stig modules, Option A (removing priority wrappers from stigConfig) is **not safe** for all cases:

- `pwquality` uses `lib.mkDefault(lib.mkBefore(...))` — a nested merge-order expression that carries semantic meaning beyond just priority. Stripping it would lose the `mkBefore` ordering.
- `aide` uses `mkDefault { text = ...; mode = ...; }` — wrapping an attrset, not a scalar. `mapAttrsRecursive` would recurse into the attrset and try to wrap its string/mode fields, but `mkDefault` sits at an intermediate level.
- `getty` uses both `lib.mkDefault` and `lib.mkForce` on two different options for deliberate priority reasons.

The correct fix is to make `overrideAttrs` unwrap any existing priority wrapper before applying `mkOverride 1000`. This correctly handles all cases: plain values are wrapped as before; already-wrapped values are unwrapped then re-wrapped at the STIG priority.

### Files to change

1. **`lib/stig/default.nix` line 100** — change `overrideAttrs` to unwrap before re-wrapping:

```nix
# Before:
overrideAttrs = attrs: mapAttrsRecursive (_: v: mkOverride 1000 v) attrs;

# After:
overrideAttrs = attrs: mapAttrsRecursive (_: v:
  let unwrapped = if v ? _type && v._type == "override" then v.content else v;
  in mkOverride 1000 unwrapped
) attrs;
```

This extracts the inner `.content` when the value is an override wrapper (`mkForce`, `mkDefault`, `mkOverride N`) and applies `mkOverride 1000` to the unwrapped value. Plain values are unaffected.

Note: `mkBefore`/`mkAfter` produce `{ _type = "order"; ... }` — not `"override"` — so they pass through `overrideAttrs` as plain values and get wrapped correctly.

### Verification

1. `nix eval` of `nix-builder-1` in the crystal-forge evaluator environment succeeds
2. No regressions on systems using other stig modules (getty, account, login, aide, pwquality)
3. Run `nix flake check` in the worktree to verify the flake evaluates cleanly
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Sprint readiness notes

**What this task is:** A bug in the Crystal Forge flake's `lib/stig/default.nix`. The `mkStigModule` helper applies `mkOverride 1000` to all leaf values in `stigConfig` — but the `timesyncd` stig module already uses `mkForce true` inside its `stigConfig`. The double-wrap produces a nested attrset instead of a boolean, which the NixOS module system rejects.

**Exact file to fix in the crystal-forge repo:**
- `lib/stig/default.nix` line 100 — `overrideAttrs` definition
- `modules/nixos/stig-modules/modules/timesyncd/default.nix` line 18 — `mkForce` inside stigConfig

**Recommended fix (Option A):** Remove `mkForce` from `stigConfig` in the timesyncd module (and audit all other stig modules for the same pattern). The `overrideAttrs` wrapper already applies `mkOverride 1000`, which has higher priority than `mkForce` (priority 50), so `mkForce` inside `stigConfig` is both redundant and harmful.

```nix
# modules/nixos/stig-modules/modules/timesyncd/default.nix
stigConfig = {
  services.timesyncd.enable = true;        # was: mkForce true
  services.timesyncd.extraConfig = ''
    PollIntervalMaxSec=60
  '';
};
```

**Audit step:** Search all files under `modules/nixos/stig-modules/` for `mkForce` inside `stigConfig` blocks and remove them.

**Why `nix-builder-1` was affected:** It sets `crystal-forge.stig-presets.off.enable = true`, which enables the stig modules including timesyncd. Other systems without that preset are not affected.

**Why it works locally but not in Crystal Forge:** The local Nix store for `nix-config` has a different pinned version of the `crystal-forge` flake than what Crystal Forge's evaluator downloads fresh. The local pin may predate the introduction of `mkForce` into the timesyncd stig module.

**Verification:**
1. After fix is merged to the crystal-forge flake and `nix-config`'s `flake.lock` is updated to the new pin:
   - `nix eval .#nixosConfigurations.nix-builder-1.config.system.build.toplevel` succeeds
   - Crystal Forge evaluation shows `nix-builder-1` as a successful system
2. Check that `crystal-forge.stig-presets.off.enable = true` still compiles cleanly on `nix-builder-2` (regression check)

MR pushed to branch TASK-398-fix-mkStigModule-double-wrap targeting dev. Open MR at: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-398-fix-mkStigModule-double-wrap

Verification passed locally:
- nix eval .#nixosConfigurations.cf-test-sys.config.system.build.toplevel → derivation (stig-presets.off)
- nix eval .#nixosConfigurations.test-agent.config.system.build.toplevel → derivation (regression)

Note: AC#3 (nix-builder-1 in CF evaluator) and AC#4 (regression at CF eval priority) require merging this fix and bumping the crystal-forge flake pin in ata-nix-config, then triggering a new CF evaluation.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## What was done

Fixed `overrideAttrs` in `lib/stig/default.nix` (line 100) to unwrap any existing `mkForce`/`mkDefault`/`mkOverride` wrapper before applying `mkOverride 1000`.

**Root cause:** `mapAttrsRecursive` visited every leaf in `stigConfig` and wrapped it with `mkOverride 1000`. When a leaf was already wrapped (e.g. `services.timesyncd.enable = mkForce true`), the result was a nested override attrset — not a boolean — causing NixOS module system type errors.

**Fix (one change, `lib/stig/default.nix`):**
```nix
overrideAttrs = attrs: mapAttrsRecursive (_: v:
  let unwrapped = if v ? _type && v._type == "override" then v.content else v;
  in mkOverride 1000 unwrapped
) attrs;
```

`mkBefore`/`mkAfter` use `_type = "order"` and are correctly unaffected.

**Verified:**
- `cf-test-sys` (uses `stig-presets.off.enable = true`, identical pattern to `nix-builder-1`) evaluates to a derivation
- `test-agent` evaluates cleanly (regression)
- Single file changed: `lib/stig/default.nix`

**Remaining:** After MR is merged, `ata-nix-config`'s `flake.lock` needs a `flake update` bump to pick up the new crystal-forge pin, which will allow `nix-builder-1` to appear in Crystal Forge evaluations.
<!-- SECTION:FINAL_SUMMARY:END -->
