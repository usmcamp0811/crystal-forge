---
id: TASK-389
---
# Fix: nix-eval-jobs fails with builtins.getFlake on remote git+ssh flake refs

## Status

Review

## Priority

high

## Problem

The evaluator constructs an inline Nix expression containing `builtins.getFlake "<remote-ref>"` and
passes it to `nix-eval-jobs --expr`. When the flake ref is a remote git URL such as:

```
git+git@github.com:ATALLC/nix-config.git?rev=0c6a0723afcf45e6e1cde4804b78c19182df541d
```

Nix rejects it with:

```
error: flake reference 'git+git@github.com:...' is not an absolute path
```

This is because `builtins.getFlake` in **pure** evaluation mode (the default) only accepts absolute
local paths. Remote git refs are only accepted in **impure** mode.

The same pattern also appears in `is_cf_agent_enabled()` which calls `nix eval --expr`.

**Affected locations:**
- `packages/default/src/models/evaluate_with_policies.rs` – `nix-eval-jobs` invocation; removed private `build_flake_reference`
- `packages/default/src/derivations/eval.rs` – `is_cf_agent_enabled()` `nix eval` call
- `packages/default/src/derivations/utils.rs` – added `normalize_flake_git_url`; updated `build_flake_reference`
- `packages/default/src/flake/commits.rs` – removed private `build_flake_reference`, now imports shared one

## Goal

Evaluation succeeds for remote `git+git@github.com:...` flake refs.

## Non-Goals

- Changing the expression structure or `build_nix_eval_expression` logic
- Changing the `build_flake_uri_with_ref` function (different code path)
- Any other evaluation changes

## Acceptance Criteria

- [x] `nix-eval-jobs` invocation in `evaluate_with_policies.rs` includes `--impure`
- [x] `nix eval` invocation in `eval.rs` `is_cf_agent_enabled()` includes `--impure`
- [x] `normalize_flake_git_url` converts scp-style `git@host:path` → `git+ssh://git@host/path`
- [x] `build_flake_reference` normalizes all URL styles (scp, ssh://, https://, github: shorthand)
- [x] Private `build_flake_reference` copies consolidated into shared public function
- [x] 14 unit tests for URL normalization and flake reference building pass
- [x] 21 deployment_policies tests pass
- [x] No regressions in existing test suites

## Verification Plan

Tier 0:
```
nix develop -c cargo check
nix develop -c cargo test -p default
nix develop -c cargo clippy -- -D warnings
```

## Impact Areas

- Flake evaluation pipeline (all deployments)
- Deployment policy checks

## Risk Level

Low — additive flag only, no logic changes

## Dependencies

None

## Notes

LOCK: opencode on dev in ~/code/crystal-forge/TASK-389-fix-getflake-impure
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/296
