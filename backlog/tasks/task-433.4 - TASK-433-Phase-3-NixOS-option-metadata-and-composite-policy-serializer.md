---
id: TASK-433.4
title: 'TASK-433 Phase 3: NixOS option metadata and composite policy serializer'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-08-24 01:22'
labels:
  - design-parity
  - policy
  - server
  - phase-3
dependencies:
  - TASK-433.3
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/data-enforcement.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 436000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 3 of 8 (contextual only). Adds production NixOS option search/type/enum metadata with unknown/custom fallback, and the smallest repository-consistent versioned composite rule-set with stable rule IDs, typed kind/config, deterministic serialization/digest, `all` semantics, and per-rule outcomes, while keeping legacy single-type policies compatible without rewriting immutable history.

## Explicit scope
- NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback sourced from real metadata (with safe fallback when metadata is unavailable).
- Long semantic values round-trip exactly, including a long multiline banner value.
- Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history (no rewriting existing versions).

## Explicit non-scope
No enforcement execution wiring (that is Phase 4). No POA&M changes. Do not flatten non-Nix rule kinds into Nix representation.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix develop -c bash -c 'cd packages/default && cargo sqlx prepare --workspace'
nix build .#checks.x86_64-linux.server-regressions --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback from real metadata or safe fallback.
- [ ] #2 Long semantic values round-trip exact difficult strings including the DoD multiline banner.
- [ ] #3 Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 3 implementation plan

1. Recover bookkeeping and partial work: retain TASK-433.3's proven Review state and TASK-433.4 In Progress state; replace the experimental nested-`nix eval` generator with a Snowfall-discovered data package using outer Nix evaluation from the pinned root `nixpkgs/release-26.05` input. Preserve unrelated dirty `dev` files.
2. Add `packages/nixos-options-metadata/default.nix`: evaluate the real NixOS option set, mirror option/submodule visibility traversal, conservatively unwrap transparent wrappers, classify boolean/enum/integer/string/lines/unknown from authoritative type objects, classify `lines` only for newline-separated strings, extract enum members from type functor payloads, sort by canonical option path, and materialize compact JSON. Add a Nix check for representative real boolean/enum/integer/string/lines entries; export the package from the flake.
3. Make the normal server package depend on the metadata artifact and compile its store path into `cf-server`; runtime resolution is explicit environment override, then packaged compile-time path, then a typed unavailable state. Add a provider that loads/parses/indexes once, returns deterministic bounded path/description search results, distinguishes available/unavailable/corrupt/zero-result states, and never runs Nix or accesses the network at runtime.
4. Define one strongly typed versioned composite model: `policy_type = composite`, `schema_version = 1`, `mode = all`, ordered non-empty rules with persisted UUIDs, and tagged `kind/config` for `nixos_option`, `packages_installed`, `custom_eval`, and `cve_block`. Store semantic JSON values (bool/integer/string) and preserve multiline strings byte-for-byte. UUIDs are generated once in new-rule UI state, never during serialization/digest; server rejects missing/malformed/nil/duplicate IDs and malformed typed configs/operators.
5. Centralize authoritative composite validation and normalization in server domain code. Reuse it from CRUD and all generic/CF-native interchange import paths that accept arbitrary `policy_type/config`. Preserve existing legacy validators and zero-enforcement `custom_check {mode: all, rules: []}` semantics. Add representation-only JSON/TOML/XCCDF read/write support as required without flattening composite kinds or executing them.
6. Add `DeploymentPolicy::Composite` (and any duplicated config representation required for exhaustive compilation) so every runtime match consciously classifies it. Composite remains non-executable in Phase 3: resolver/evaluator/deployment enforcement paths return explicit unsupported/fail-closed behavior; compliance reporting may remain NotChecked, but no enforce path may drop, partially execute, or treat composite as passing. Do not implement per-rule outcomes/execution/aggregation.
7. Correct exact-version loading where necessary so immutable version `policy_type` is paired with its own version `config`, not the mutable lineage type. Reuse the existing canonical JSON/digest implementation unchanged; add pure tests proving key-order invariance and sensitivity to rule order, IDs, kinds, typed values, and single-newline changes.
8. Extend the accepted Phase-2 common Dioxus editor: typed composite hydration/serialization, stable UUID state, debounced bounded metadata search with stale-response suppression, distinct loading/error/unavailable/zero-result states, manual arbitrary paths, metadata-driven boolean/enum/integer/string/lines controls, and unknown/custom semantic-string fallback. Untouched legacy policies remain legacy; unsupported/opaque types cannot be mistaken for intentional zero enforcement.
9. Add exact semantic round-trip tests using the repository's full `DOD_CONSENT_BANNER` content (read from the authoritative design file without trimming) plus a difficult quotes/backslashes/`${...}`/blank-lines/leading-trailing-whitespace string. Add provider unit tests, composite serde/validation/digest tests, a live isolated-Postgres immutable ancestor/derived composite draft regression, API/import bypass regressions, and browser workflow coverage for real metadata types, unknown fallback, exact banner reload, and stable IDs.
10. Run all required server/web/Nix/SQLx/browser checks plus the metadata package/check and `nix flake check --keep-going` if feasible. Then perform separate requirements, data integrity, runtime safety, metadata correctness, performance, UI/error-state, E2E, and regression-adequacy reviews. Only if AC1-AC3 are completely proven: record exact evidence, move TASK-433.4 to Review, update MR !318, commit/push, confirm clean worktree, and stop before Phase 4.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Ownership/preflight audit: task worktree is clean at 36736d8d with no later commits; 36736d8d is an ancestor and origin matches. Central task records show TASK-433.3 Review with all four ACs checked and TASK-433.4 In Progress. Partial Phase-3 work consisted only of an untracked experimental `packages/default/nixos-options-metadata.nix` in `dev`; it had useful raw-type/enum ideas but was incomplete (undefined `lib`, incorrect submodule traversal, overbroad separatedString→lines classification, no wrapper/integer handling, and proposed nested Nix evaluation). It will be replaced, not continued. Two unrelated untracked compliance-audit text files in `dev` were inspected and will not be touched. No commits exist after 36736d8d and no Phase-4 work was found.
<!-- SECTION:NOTES:END -->
