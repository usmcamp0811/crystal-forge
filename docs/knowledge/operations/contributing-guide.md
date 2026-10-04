---
type: Operator Guide
title: Contributing guide
description: Pointer and status record for the repository's CONTRIBUTING.md, summarizing its project-status statement, contribution process, testing requirements, documentation and style expectations, and where its commands no longer match the repository.
tags:
  - crystal-forge
  - contributing
  - process
  - testing
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:59:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file CONTRIBUTING.md at commit 3b23d36f"
    title: Contributing to Crystal Forge
---

# Contributing guide

This concept is a navigation and status record. The authoritative text is the
retained file [CONTRIBUTING.md](../../../CONTRIBUTING.md). It stays at the
repository root as a repository entry point and contribution policy. The
migration did not edit it.

## What CONTRIBUTING.md specifies

| Section | Content |
| --- | --- |
| Project Status & Goals | Crystal Forge is in active development with working system monitoring, flake tracking, and deployment enforcement. The maintainer states that the project stays free and open source for homelab and personal use. The maintainer plans paid support and features for organizations, companies, and government entities, and all core functionality stays open source ("Long-term Sustainability"). |
| How to Contribute | "Finding Work" points to the GitLab issue tracker and the `good first issue` and `help wanted` labels, and asks contributors to open an issue before proposing features. "Development Process" has five steps: open or comment on an issue, fork and branch, implement with tests, submit a merge request, address feedback. "What I'm Looking For" lists working features, tests, documentation, and no regressions. |
| Testing Requirements | Rust changes need unit tests in `#[cfg(test)]` modules (run with `cargo test` or `nix build`). Database changes need tests under the pytest package's `tests/database/` and `scenarios/` directories (run with a `database` check). Component integration lists server, builder, S3 cache, and Attic cache checks and the full `nix flake check`. It links the test plan. |
| Project Management & Process | The process is loose: no strict deadlines or sprints, merge requests reviewed as time allows (usually within a few days), discussion in issues and merge requests. |
| What I'm Still Learning | The maintainer is new to Rust and to parts of the Nix CLI and invites feedback. |
| Documentation | Update documentation in `docs/`, comment non-obvious code, update the changelog for user-facing changes, and include examples. Frontend contributors are pointed to the frontend component standards. |
| Code Style | Follow existing patterns, use `rustfmt`, keep functions focused, and write tests that document expected behavior. |
| Communication | Be respectful, ask questions, and share context. |
| Getting Help | Open an issue, tag the maintainer in merge requests, read `docs/`, and look at existing tests. |
| License | Contributions are licensed under the project license (see the `LICENSE` file). |

## Implementation status and evidence

Status: partial. The contribution policy and style guidance are intentional
text. Several commands and paths in the "Testing Requirements" and
"Documentation" sections do not match the repository at the migration base
commit.

> **Status:** stale testing commands. `CONTRIBUTING.md` names
> `packages/cf-test-modules/cf_test/tests/database/` and
> `packages/cf-test-modules/cf_test/scenarios/`. The package directory at the
> migration base commit is `packages/cf-test-suite/`. It names the checks
> `.#checks.x86_64-linux.database`, `.#checks.x86_64-linux.server`,
> `.#checks.x86_64-linux.builder`, `.#checks.x86_64-linux.s3-cache`, and
> `.#checks.x86_64-linux.attic-cache`. No directory with those names exists
> under `checks/`, and a search of `*.nix` and `*.yml` files found no
> `attic-cache` or `s3-cache` definition. The existing checks are in
> [Crystal Forge flake checks](../testing/flake-checks.md). The test plan link
> points to `docs/test_plan.md`, which the migration moved to
> [the testing plan](../testing/test-plan.md). The changelog the file mentions
> was not found at the repository root.

> **Status:** process drift. The sentence "No strict deadlines or sprints"
> describes a manual contribution flow. The agent-driven workflow in
> `AGENTS.md` and the planning process in
> [the AI sprint planning guide](ai-sprint-planning.md) use sprints and a
> Backlog.md lifecycle. The two audiences differ (external contributors versus
> agents), and the migration did not decide which text governs.

## Related concepts

- [Crystal Forge flake checks](../testing/flake-checks.md)
- [Crystal Forge testing plan](../testing/test-plan.md)
- [AI sprint planning and backlog grooming guide](ai-sprint-planning.md)
- [Backlog process documents](backlog-process-documents.md)
