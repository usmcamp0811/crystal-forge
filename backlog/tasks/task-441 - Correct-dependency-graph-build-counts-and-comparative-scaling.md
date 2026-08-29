---
id: TASK-441
title: Correct dependency graph build counts and comparative scaling
status: In Progress
assignee:
  - openai-gpt-5.6-sol
created_date: '2026-08-29 16:26'
updated_date: '2026-08-29 16:35'
labels:
  - backend
  - frontend
  - nix
  - dependency-graph
dependencies: []
documentation:
  - 'https://nix.dev/manual/nix/2.35/command-ref/nix-store/realise.html'
  - 'https://manpages.ubuntu.com/manpages/jammy/man1/nix-store.1.html'
priority: high
type: bug
ordinal: 450000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The evaluation dependency graph currently conflates closure paths, locally absent outputs, substitutions, and source builds. This can make every evaluated NixOS system appear to require its full closure to be built and can produce full-width bars for every row. Correct the graph so that “to build” means the number of dependency derivations that Nix reports it would build when realizing the evaluated system with Crystal Forge’s effective build configuration. Exclude the top-level NixOS system derivation because the graph measures dependencies. Preserve a separate, accurate dependency-derivation total where the product still needs that value. Align the API contract and UI terminology with the fact that each row represents a NixOS system, not a package. Present build work on a shared scale across systems so bar lengths compare absolute build counts. A zero-build plan, unavailable plan data, and plan failure must remain distinct states.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each successfully evaluated NixOS system, the graph reports the number of dependency derivations that Nix would build under the same substitute and offline configuration used for the real build.
- [ ] #2 The reported build count excludes the evaluated top-level NixOS system derivation.
- [ ] #3 Dependencies that Nix would download from a substituter do not increase the reported build count.
- [ ] #4 A system whose realization requires no dependency builds reports zero builds.
- [ ] #5 Any retained dependency total counts derivations only and does not count source files, patches, configuration files, outputs, or other non-derivation store paths.
- [ ] #6 Failure or unavailability of build-plan calculation is represented separately from a valid zero-build result and does not silently become zero, the closure total, or a successful count.
- [ ] #7 The dependency graph API names rows and fields as systems and dependency/build counts rather than packages, and all in-repository consumers use the corrected contract.
- [ ] #8 Every evaluated system’s build-work bar uses one common maximum build count across the response, so equal build counts have equal widths and a count of 100 is ten times the width of a count of 10 when 100 is the maximum.
- [ ] #9 When all evaluated systems have zero builds, the graph renders a stable zero-work state without division errors or misleading full-width bars.
- [ ] #10 Failed systems remain visually and semantically distinct from systems with valid build-plan data.
- [ ] #11 Backend regression tests cover mixed derivation and non-derivation closure paths, build and substitution plans, no-op plans, singular and plural build counts, top-level derivation exclusion, and plan failure.
- [ ] #12 Frontend regression coverage verifies equal-count widths, proportional 10-to-100 widths, all-zero data, unavailable plan data, and failed-system presentation.
- [ ] #13 Affected API and maintainer-facing documentation defines the build-count semantics, top-level exclusion rule, configuration dependency, and failure behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a forward-only migration that records dependency build-plan count and an explicit unavailable/calculating/complete/failed state on each NixOS derivation. Keep `closure_total` as the retained internal dependency-derivation total, but write only filtered `.drv` requisites and exclude the evaluated top-level derivation. Treat legacy rows as unavailable.
2. Replace closure package/cache inference with a documented build-plan calculation. Query requisites for the derivation-only total, run `nix-store --realise --dry-run` with the same `BuildConfig` substitute/offline and build options used for realization, parse only the derivations Nix says it would build, exclude the top-level derivation, accept a no-op plan as zero, and persist explicit failure instead of fallback counts.
3. Update every evaluation finalization path to pass `BuildConfig`, mark plan calculation state, and persist success or failure without blocking build queue activation.
4. Replace package-oriented query/API DTO fields with system name, dependency derivation count, optional dependency build count, explicit plan status, and system failure state. Update all in-repository consumers and API rustdoc to define configuration dependence, top-level exclusion, valid zero, unavailable, and failure semantics.
5. Extract frontend graph presentation calculations. Use one maximum valid dependency build count for all rows, render zero work without division, and render unavailable plan data, plan failure, and failed systems as distinct states. Update labels to systems/dependency derivations/build work.
6. Add focused backend parser/state tests for mixed requisite paths, singular/plural build plans, substitutions, no-op output, top-level exclusion, command/configuration behavior, and failures. Add frontend unit tests for equal/proportional widths, all-zero values, unavailable plans, and failed systems.
7. Extend the authoritative Web UI integration fixture/assertions and screenshot coverage for the dependency graph, then run targeted server/Web UI checks, package builds, migration/SQLx verification as required, and the authoritative Web UI check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-gpt-5.6-sol on gray in /home/mcamp/code/crystal-forge/TASK-441-dependency-graph-counts
<!-- SECTION:NOTES:END -->
