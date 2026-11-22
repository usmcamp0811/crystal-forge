---
id: TASK-540
title: 'Prepare v0.4.0 release: version, README, docs, screenshots, and changelog'
status: In Progress
assignee: []
created_date: '2026-11-22'
labels:
  - docs
  - screenshots
  - release
dependencies: []
references:
  - TASK-274
  - TASK-534
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge has no v0.4.0 release preparation. Audit of `origin/dev` at
`dd5ff524` (2026-11-22):

- **Version.** All ten workspace `Cargo.toml` files, the three lockfiles
  (`packages/default/Cargo.lock`, `packages/web-ui/Cargo.lock`,
  `packages/oscal-fixture/Cargo.lock`) and `packages/oscal-fixture/default.nix` say `0.3.0`. The newest tag is
  `v0.3.1`, and its manifest also says `0.3.0` (recorded in
  `checks/integration-upgrade/README.md`). `cf-nixos-module` derives its
  generator string from `CARGO_PKG_VERSION`, and its test fixture
  (`packages/default/crates/cf-nixos-module/nix/tests-helper/default.nix`,
  lines 243 and 269) hardcodes `cf-nixos-module 0.3.0`. A version bump that
  misses it fails that test.
- **README.** `README.md` says `Current status: v0.3.0`, links only the v0.3.0
  release notes, and its feature list does not mention access tokens, the MCP
  server, or agent remediation.
- **Screenshots.** The README embeds `docs/screenshots/06-dashboard.png`,
  last changed 2026-04-16 (v0.3.0 era). No current command regenerates
  `docs/screenshots/`: the legacy `integration-test.js` suites were retired
  (TASK-490.24). `packages/ui-screenshots` exists but renders design-fixture
  data, not a running server, and sets `__noChroot = true`.
- **Changelog.** There is no `CHANGELOG.md`. 2927 commits separate `v0.3.1`
  from `dev`.
- **Knowledge docs.** These concepts state v0.3.0 as current:
  `docs/knowledge/overview/project-introduction-and-key-features.md`,
  `release-roadmap-and-milestones.md` (v0.4.0 row says "Backlog"),
  `system-context-and-current-state.md` ("version 0.3.0"), and
  `docs/knowledge/compliance/nixos-module-generation.md` (example manifest).
  `docs/knowledge/log.md` and `checks/integration-upgrade/README.md` also
  mention versions.

## Goal

A clean, accurate v0.4.0 release candidate: one consistent version, a README
and knowledge base that describe the product as of v0.4.0, current
screenshots, and a changelog.

## Non-goals

- No product behavior, schema, or migration change.
- No tag, GitLab release, or merge to `main`. The owner creates the tag.
- No rewrite of historical records (`backlog/completed`, release-notes-v0-3-0,
  design documents under `docs/design`).
- No new flake check (the four-check inventory is fixed).

## Decisions (owner, 2026-11-22)

1. Start now from `origin/dev`; add the MCP/agent-access entries, docs and screenshots after !349, !351 and !353 reach `dev`.
2. Bump every version to 0.4.0 in this task.
3. Produce screenshots with `.#ui-screenshots` on the farm first. It renders design-fixture data and sets `__noChroot`; if the farm rejects it, report and ask. Do not touch the dev instance.
4. `CHANGELOG.md` in Keep a Changelog format, with a [0.4.0] section for v0.3.1 to release and short [0.3.1] and [0.3.0] stubs, plus a `release-notes-v0-4-0` concept in `docs/knowledge/overview`.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every workspace crate, all three lockfiles, `packages/oscal-fixture`, and the `cf-nixos-module` test expectations agree on one version, and `rg '0\.3\.0'` finds no stale current-version reference outside historical records.
- [ ] #2 `README.md` states the v0.4.0 status, lists features present in the release, and links valid release-note, changelog, and roadmap targets.
- [ ] #3 Every image the READMEs embed shows the UI as of the release head, with a recorded source and date.
- [ ] #4 `CHANGELOG.md` exists, follows one stated format, and covers every release-visible change since `v0.3.1`, each traceable to a task or MR.
- [ ] #5 Knowledge concepts that describe current state (overview, roadmap, system context, nixos-module-generation) are updated; historical concepts are kept and marked historical with a successor link.
- [ ] #6 `nix build .#okf-knowledge -L` passes.
- [ ] #7 Every relative link in changed Markdown files resolves.
- [ ] #8 The affected unit and integration checks pass at the release-prep head, including `integration-upgrade` after the version bump.
<!-- AC:END -->

## Verification plan

- `nix build .#okf-knowledge -L`.
- Farm build of `cf-nixos-module` tests and `.#packages.x86_64-linux.default.cf-server-core-drv` after the bump.
- `integration-upgrade` `version-transition` after the bump: it records predecessor and current identities, and a changed current version must not break its assertions.
- Link check over changed Markdown.
- Screenshots: source and capture command recorded in this task.

## Impact areas

Cargo manifests and lockfiles, one Nix fixture, README, CHANGELOG, `docs/knowledge/overview`, `docs/knowledge/compliance`, `docs/screenshots`. Also `packages/okf-knowledge/validate.py`: `CHANGELOG.md` is added to `ROOT_KEPT` so the validator accepts a root changelog kept outside the bundle.

## Risk

Low for documents. Medium for the version bump: a missed reference fails a build or test, and changing `Cargo.lock` changes derivation inputs for every package.

## Dependencies

The MCP stack (!349 → !351 → !352 merged into their parents; !353 blocked) decides what the changelog and README must cover.
<!-- SECTION:DESCRIPTION:END -->

## Implementation notes

LOCK: claude-agent on vm-agent in /home/mcamp/code/crystal-forge/TASK-540-release-0.4.0-prep

Base: `origin/dev` at `dd5ff524`. Selection delegated by the owner on 2026-11-22 (explicit request to prepare the release).

## Progress (2026-11-22)

Done in the worktree (uncommitted):

- Version 0.3.0 → 0.4.0 in 10 `Cargo.toml` files, 3 lockfiles (workspace members only; third-party crates at 0.3.0 such as `gloo-storage` and `jni-sys` were left alone), `packages/oscal-fixture/default.nix`, and the `cf-nixos-module` fixture and docs example.
- `CHANGELOG.md` (Keep a Changelog) from the 73 MRs merged to `dev` after `v0.3.1`, taken from the GitLab MR titles, not from backlog task titles (several branch-to-task matches were wrong).
- `docs/knowledge/overview/release-notes-v0-4-0.md`, and updates to the project introduction, release roadmap, system context, overview index, and bundle log.
- `README.md`: status, features, documentation table.
- `packages/okf-knowledge/validate.py`: `CHANGELOG.md` added to `ROOT_KEPT`.

Verified on the farm at 0.4.0 (uncommitted tree): `tests-helper` EXIT 0, `tests-generation` EXIT 0, `oscal-fixture` EXIT 0, `okf-knowledge` EXIT 0 (after adding `CHANGELOG.md` to `ROOT_KEPT` and removing it from the concept's `sources`), `agent` EXIT 0 on retry.

Failures that were not caused by this task:

- `cf-agent` test `short_basic_credentials_are_redacted_before_marking_report_safe` failed once and passed on retry of the same tree; `origin/dev` passes. Recorded as TASK-541.
- `cf-server-core` and `integration-upgrade` first runs failed with sandbox crashes (signal 11 in a tiny Nix derivation, signal 6 in `fix-paths.patch`). Retried; results below when complete.

Findings that need an owner decision:

- `.#ui-screenshots` builds on the farm, but every view shows deserialization errors and an error banner: `packages/ui-screenshots/routes.js` fixtures no longer match the API (`Healthy` vs `healthy`, a map where a list is expected). Its `results.json` reports `ok: true` for all 26 captures, so it does not detect this. Its output is not fit for the README.
- `accessTokens.enable` defaults to `true`, so upgrading users get token creation on. Recorded in the changelog and release notes.
