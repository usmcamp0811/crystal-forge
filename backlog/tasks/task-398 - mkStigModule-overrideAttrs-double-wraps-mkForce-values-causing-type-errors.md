---
id: TASK-398
title: mkStigModule overrideAttrs double-wraps mkForce values causing type errors
status: Backlog
assignee: []
created_date: '2026-07-24 00:29'
labels:
  - stig
  - nix
  - evaluator
dependencies: []
references:
  - 'lib/stig/default.nix:100'
  - 'modules/nixos/stig-modules/modules/timesyncd/default.nix:18'
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
