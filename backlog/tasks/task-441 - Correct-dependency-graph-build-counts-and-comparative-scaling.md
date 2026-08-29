---
id: TASK-441
title: Correct dependency graph build counts and comparative scaling
status: To Do
assignee: []
created_date: '2026-08-29 16:26'
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
