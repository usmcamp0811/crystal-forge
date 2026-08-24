---
id: TASK-433.4
title: 'TASK-433 Phase 3: NixOS option metadata and composite policy serializer'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-08-24 13:03'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
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
- [x] #1 NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback from real metadata or safe fallback.
- [x] #2 Long semantic values round-trip exact difficult strings including the DoD multiline banner.
- [x] #3 Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history.
- [ ] #4 Permanent repository documentation states that packaged NixOS option metadata is authoring guidance from Crystal Forge's pinned nixpkgs rather than authoritative target schema and regression coverage preserves the unknown/custom path
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

11. Review follow-up: add a focused architecture document under the existing `docs/` hierarchy that records metadata source, authoring uses, foreign-flake limitations, authority hierarchy, unknown/custom invariant, version-skew examples, future target-specific metadata/cache identity, and the Phase 3/Phase 4 boundary. Link it from the policy enforcement documentation and TASK-433.4 notes; add concise provider/API authority comments; confirm server and browser tests preserve unknown-option authoring rather than treating baseline absence as target invalidity. Run targeted documentation/code checks, Web UI and server tests, and the authoritative browser check as needed, then update MR evidence and return the task to Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Ownership/preflight audit: task worktree is clean at 36736d8d with no later commits; 36736d8d is an ancestor and origin matches. Central task records show TASK-433.3 Review with all four ACs checked and TASK-433.4 In Progress. Partial Phase-3 work consisted only of an untracked experimental `packages/default/nixos-options-metadata.nix` in `dev`; it had useful raw-type/enum ideas but was incomplete (undefined `lib`, incorrect submodule traversal, overbroad separatedString→lines classification, no wrapper/integer handling, and proposed nested Nix evaluation). It will be replaced, not continued. Two unrelated untracked compliance-audit text files in `dev` were inspected and will not be touched. No commits exist after 36736d8d and no Phase-4 work was found.

Final verification proved all Phase 3 acceptance criteria. The authoritative browser workflow `20ab1-policy-editor-composite-metadata-roundtrip` passed against real packaged NixOS metadata and produced dark/light screenshots at `/nix/store/kcfzj8xvfrvfslbj7q5wcy24dik5zwvh-vm-test-run-crystal-forge-web-ui-mega-integration/screenshots/`. The workflow verifies boolean, enum, integer, lines, and unknown/custom controls; exact DoD banner and difficult-string reload; semantic JSON values; unique stable UUIDv4 IDs; hydration; reorder; and reserialization. A catalog reload defect in the test was corrected by reopening the Security domain, and dynamic enum hydration was made explicit by preserving the selected option. Targeted Web UI tests passed (30/30), the authoritative Web UI Nix check passed, and final `nix flake check --keep-going` completed with `all checks passed!`. One earlier Web-check attempt failed in the known unrelated timing-sensitive hardening scanner test; the unchanged retry passed. Phase 4 execution remains intentionally unimplemented and composite execution paths fail closed.

Pushed commit `a00e15ff` to `origin/TASK-433-policy-poam-workflows`. MR !318 was updated with the Phase 3 summary, exact verification evidence, and the passing dark-mode browser screenshot. The new MR pipeline is running at https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2784287195. Per repository workflow, the task is now in Review rather than Done; it must not move to Done until the MR is merged and the dedicated worktree is removed.

After the initial push, GitLab reported one merge conflict in the canonical TASK-433.2 backlog record because `dev` had advanced through task-metadata commits. `git merge-tree` confirmed this was the only conflict and no application code conflicted. Merged `origin/dev` with the canonical `dev` TASK-433.2 record in merge commit `0e03d783` and pushed it. MR !318 now reports `has_conflicts: false`; the task worktree is clean and matches origin. The Phase 3 implementation commit remains `a00e15ff`.

Review feedback reopened Phase 3 to make metadata authority explicit. The packaged catalog is generated from Crystal Forge's pinned nixpkgs and is authoring guidance only; a monitored foreign flake's own evaluation remains authoritative. Scope is documentation, concise provider/API comments, and regression protection for the existing unknown/custom path only. Target-specific metadata generation remains future work and Phase 4 execution will not be implemented here.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary
- Added a pinned-Nixpkgs NixOS option metadata package/check and packaged, cached server provider with authenticated bounded search and explicit unavailable/corrupt states.
- Added the versioned `composite` policy representation with ordered stable UUID rules, typed configs and semantic JSON values, deterministic digest/round-trip behavior, exact-version persistence, immutable derived history, and JSON/TOML/XCCDF representation support.
- Extended the shared policy editor with real metadata search, metadata-driven boolean/enum/integer/string/lines controls, safe unknown/custom fallback, exact multiline hydration, and stable rule IDs.
- Kept Phase 4 out of scope: composite policies are representation-only and fail closed on execution paths.

## Verification
- Full server and Web UI Cargo test suites passed.
- SQLx workspace preparation passed against the isolated repository database.
- Server, Web UI, metadata, and server-regression Nix builds passed.
- Authoritative Web UI workflow `20ab1-policy-editor-composite-metadata-roundtrip` passed with dark/light screenshots.
- `nix flake check --keep-going` passed all checks.

## Risk
- Composite execution is deliberately unsupported until Phase 4; callers receive explicit unsupported/fail-closed behavior rather than partial execution or success.
<!-- SECTION:FINAL_SUMMARY:END -->
