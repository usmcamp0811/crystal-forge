---
type: Operator Guide
title: Contributing guide
description: "Summarizes the repository's CONTRIBUTING.md: the external contribution process, current test-package and Nix check commands, documentation and style expectations, and the distinction from the internal agent workflow."
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
| Testing Requirements | Rust changes need unit tests in `#[cfg(test)]` modules (run with Cargo or Nix). Database tests live in `packages/cf-test-suite/cf_test/tests/database/`; reusable scenarios live in `packages/cf-test-suite/cf_test/scenarios/`. The guide gives the `cf-test-suite.runTests` database-marker command, `server-regressions`, `integration`, and `nix flake check`. It links the current test plan and check catalog. |
| Project Management & Process | The process is loose: no strict deadlines or sprints, merge requests reviewed as time allows (usually within a few days), discussion in issues and merge requests. |
| What I'm Still Learning | The maintainer is new to Rust and to parts of the Nix CLI and invites feedback. |
| Documentation | Update documentation in `docs/`, comment non-obvious code, update the changelog for user-facing changes, and include examples. Frontend contributors are pointed to the current component isolation standards. |
| Code Style | Follow existing patterns, use `rustfmt`, keep functions focused, and write tests that document expected behavior. |
| Communication | Be respectful, ask questions, and share context. |
| Getting Help | Open an issue, tag the maintainer in merge requests, read `docs/`, and look at existing tests. |
| License | Contributions are licensed under the project license (see the `LICENSE` file). |

## Audience and current testing references

The retained `CONTRIBUTING.md` is the policy for external contributors. The
internal agent workflow in `AGENTS.md` is a separate process and uses Backlog.md
task selection, task locks, and dedicated worktrees. These documents serve
different audiences.

The guide lists `nix flake check` as the broad check. Use the [flake-check
catalog](../testing/flake-checks.md) for the current check matrix and the
[testing plan](../testing/test-plan.md) for test locations. External
contributors use the issue and merge-request process above. Internal agents
follow the repository agent workflow in `AGENTS.md`.

## Related concepts

- [Crystal Forge flake checks](../testing/flake-checks.md)
- [Crystal Forge testing plan](../testing/test-plan.md)
- [AI sprint planning and backlog grooming guide](ai-sprint-planning.md)
- [Backlog process documents](backlog-process-documents.md)
