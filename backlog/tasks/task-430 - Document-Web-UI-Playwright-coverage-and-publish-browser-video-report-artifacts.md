---
id: TASK-430
title: Document Web UI Playwright coverage and publish browser video/report artifacts
status: To Do
assignee: []
created_date: '2026-08-21 17:36'
labels:
  - web-ui
  - playwright
  - nix
  - testing
  - documentation
  - artifacts
dependencies:
  - TASK-422
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/316'
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/design-parity/
  - docs/web-ui-check.md
  - backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md
  - .gitlab-ci.yml
documentation:
  - docs/web-ui-check.md
  - backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md
modified_files:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/design-parity/
  - docs/web-ui-check.md
  - .gitlab-ci.yml
priority: high
type: enhancement
ordinal: 430000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Context / Problem

Crystal Forge has substantial Web UI coverage, but its evidence is difficult to understand and reuse. The executable inventory is a roughly 9,000-line custom Playwright-library script; screenshots and visual reports are scattered under one `screenshots/` result directory; there is no polished browser video showing end-to-end Crystal Forge functionality. Some captures have appeared visually different from a normal deployed Chrome/Chromium desktop session, but the cause is not established and must not be “fixed” by blindly changing browsers.

The NixOS check is already expensive (40-minute global timeout; full UI/export runs can take 30 minutes or more). Artifact/video work must be measured and must not silently make merge-gating verification slower, larger, or less reliable.

This task is based on MR !316 (`TASK-422: Rebuild compliance view`). Its source branch is `TASK-422-compliance-view-redesign` at `8697c767`; the MR is still open. Implement only after !316 is merged into `dev`, starting from the merged post-!316 state. Do not reimplement the compliance feature here.

## User Story / Outcome

A developer/reviewer can run `nix build .#checks.x86_64-linux.web-ui -L` and immediately browse `result/` for screenshots, visual/design-parity evidence, reports, machine-readable results, and useful browser videos. An offline report explains what each check proves, and at least one deterministic video is polished enough to demo Crystal Forge. If video materially harms the normal check, a separately exposed video/demo check is available using established naming conventions, without duplicating the expensive suite.

## Current State Found

- `checks/web-ui/default.nix` defines a `pkgs.testers.runNixOSTest` named `crystal-forge-web-ui-mega-integration`, booting a real Crystal Forge server/PostgreSQL and gitserver. The UI runs in the VM and artifacts leave through `machine.copy_from_vm`; optional Attic/S3/builder phases are disabled in ordinary runs.
- The flake exposes `checks.x86_64-linux.web-ui`. `.gitlab-ci.yml` includes it in the check matrix but currently copies only `result/screenshots/*` to CI; the MR-comment job consumes PNGs, visual summaries, the design-parity matrix, and montages.
- `checks/web-ui/tests/integration-test.js` uses `require("playwright")`, `chromium.launch()`, one authenticated context, and `page.screenshot({ path })`. It is not Playwright Test and does not use MCP. It writes `results.json`, `visual-report.json`, and `visual-summary.md`.
- Nix installs `pkgs.playwright-test` and `pkgs.chromium`, but sets `PLAYWRIGHT_BROWSERS_PATH` to `pkgs.playwright-driver.browsers` without explicit `channel`/`executablePath`. The actual VM executable must be verified and reported; installed full Chromium is not proof it is used.
- On the !316 branch, `coverage-manifest.json` and the executable array reconcile to 137 steps: 98 in `ci_fast`, 137 in `full`, 71 with `mockedData: true`, and 83 with `interactions: true`. Nix defaults to `ci_fast`; direct JS defaults to `full`; focused selection uses `CF_UI_TEST_STEPS`. Basic name-set/duplicate drift is gated, but metadata is sparse and a separate hard-coded 14-name `critical_tests` list defines merge-blocking behavior. Strict visual baselines are another gate; design parity is non-blocking.
- The manifest defines 1920x1080, UTC, en-US, dark/light themes, routes, profiles, design references, and baseline policy. The script also uses 1440x900, 900x900, 560x900, and 375x812 responsive viewports, but no explicit device-scale/font contract or browser environment report.
- `checks/web-ui/design-parity/` renders 13 offline design-example views, including refreshed `/compliance`, and produces raw captures, drift JSON/Markdown, montages, and a matrix under `screenshots/`. OSCAL/SARIF final screenshots are copied, but their dedicated result JSON is currently left in the VM.
- `docs/web-ui-check.md` documents screenshots/baselines/design parity and CI behavior, but has no generated per-step catalog, browser report, video workflow, or stable top-level artifact contract.
- !316 adds compliance workflows/fixtures for bundle catalog/detail, revisions, requirement coverage, policy drill-in, systems/evidence, and STIG import/resume (`20ac`, `20ae`, `29a`, `29aa`, `29b`–`29f`, `30d`). The demo must use functionality/data available after merge. `backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md` §9.3–§9.5 provides the relevant storyboard.

## Proposed Approach

1. Measure unchanged post-!316 baseline, screenshot/report-only overhead, and proposed video overhead under comparable Nix/VM conditions. Record wall time, result size, profile/step count, phases, and reliability in the MR.
2. Preserve `coverage-manifest.json` as the source of truth. Enrich it or add a generated schema so every executable step has stable name, purpose, workflow/feature, route, prerequisites/state, actions, semantic assertion, interaction status, data source (real/seeded/mocked), profiles, critical/advisory status, baseline policy, design reference, screenshot/video associations, and export/download validation. Generate Markdown/HTML from it rather than a second hand-written inventory.
3. Strengthen drift detection: added, removed, duplicated, or renamed executable steps must fail or emit a clear fatal result unless documentation metadata is updated. Keep refactoring small/data-driven; do not rewrite as `@playwright/test` solely for reporting.
4. Investigate browser fidelity before choosing a path. Compare the current Playwright-resolved browser/headless shell, `channel: "chromium"`/new headless, and explicit Nix `pkgs.chromium`; inspect executable/version, Playwright version, fonts/fallback, CSS/font assets, viewport, device scale, locale/timezone, GPU/software rendering, headless mode, and VM-versus-deployment differences. Choose and document a reproducible Chrome/Chromium mode representative of a normal desktop user. Branded Chrome is optional.
5. Record browser metadata in output: Playwright version, product/channel/version, executable/package source where practical, viewport, device scale, locale, timezone, headless/rendering mode, font contract, and launch flags. Keep responsive viewports explicit and use the normal desktop viewport (normally 1920x1080) for demo captures.
6. Use direct Playwright-library video recording unless investigation justifies another approach. Produce a small useful set of verification videos, not hundreds of tiny files. Review MCP/chapter support, but do not make MCP a runtime dependency solely for chapters; use stable offline sidecar/report metadata if sufficient.
7. Produce at least one deterministic, offline, polished demo using realistic seeded/mock data and shared helpers where safe. Prefer authenticated shell/dashboard, compliance catalog, bundle drawer, revisions or requirement coverage, coverage filter/policy drill-in or systems/evidence. Pace it for a human viewer and avoid debug garbage, blank/loading states, and destructive mutations. WebM is acceptable; transcode only with clear value.
8. Define an obvious Nix result contract, such as `screenshots/`, `videos/demo/`, `videos/checks/`, `reports/`, `design-parity/`, and exports. Generate a self-contained offline `reports/index.html` (or justified equivalent) with relative links to screenshots/diffs/montages/videos, failures, exports, JSON, and Markdown. Copy artifacts from the VM and update CI while retaining the existing MR screenshot summary.
9. If recording adds more than 10% or five minutes, materially increases storage, or causes reliability regression, expose a separate video/demo check after inspecting existing names (`web-ui`, `web-ui-reconciliation`) and reuse fixtures/harness code without duplicating the full suite. Document the final decision and exact check name.

## Verification Plan

From merged post-!316 `dev`, use the repository Nix environment:

```text
time nix build .#checks.x86_64-linux.web-ui -L
find -L result -maxdepth 5 -type f -print | sort
```

Inspect rather than merely trust exit status: open `result/reports/index.html`, inspect JSON/Markdown and relative links, browse screenshots/diffs/design-parity/exports, and manually open/play each required video. Verify the browser report identifies the actual VM executable. Exercise documented full/focused variants using the supported profile/`CF_UI_TEST_STEPS` mechanism. Test manifest drift in a temporary copy and confirm a clear failure. If a separate check is selected, run its exact documented `nix build .#checks.x86_64-linux.<chosen-video-check> -L` command and inspect its result. Confirm OSCAL/SARIF and existing visual/design-parity output remain present and correctly gated. Record comparable baseline, report-only, video, and demo runtimes/result sizes and at least two runs where practical.

## Documentation Deliverables

Update `docs/web-ui-check.md` with final check names, normal/full/focused/video commands, browser decision, performance measurements, verification-versus-demo policy, report entry point, and artifact tree. Generate any additional coverage catalog from the manifest; do not hand-maintain a duplicate inventory. Keep manifest/design-parity documentation and `.gitlab-ci.yml` artifact behavior accurate.

## Out of Scope

No unrelated Web UI redesign, compliance behavior change, broad Playwright rewrite, mandatory Google Chrome dependency, MCP runtime dependency solely for recording, replacement of visual baselines/design parity/OSCAL/SARIF semantics, or unrelated flaky-test cleanup.

## Dependencies

Blocked on MR !316 (`TASK-422: Rebuild compliance view`, https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/316) being merged into `dev`. Start from merged post-!316 `dev` and include its Web UI test/fixture changes. Preserve the existing NixOS VM, manifest, design-parity harness, visual gates, export validation, and CI artifact jobs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every executable Playwright Web UI step on the post-!316 branch is represented in maintained documentation; the 137-step manifest/script inventory is reconciled after !316 merge.
- [ ] #2 Documentation cannot silently drift from executable coverage: added, removed, duplicate, or renamed steps fail the check or emit a clear fatal/drift result requiring metadata updates.
- [ ] #3 Each documented step identifies stable name, purpose, route/page, workflow/feature area, prerequisites/state, important actions, semantic behavior asserted, interaction status, real/seeded/mocked data source, profiles, and critical/merge-blocking versus advisory status.
- [ ] #4 Each documented step identifies visual-baseline policy, design reference when applicable, screenshots, associated videos when applicable, and related export/download validation.
- [ ] #5 The report distinguishes ci_fast, full, focused-step selection, and curated video/demo profile; critical/advisory status has one authoritative mapping with no contradictory hard-coded inventory.
- [ ] #6 Browser identity/rendering mode is recorded: Playwright version, product/channel/version, executable/package source where practical, viewport, device scale factor, locale, timezone, headless/rendering mode, font contract, and launch flags.
- [ ] #7 Browser-fidelity investigation compares the current resolved browser, Playwright new-headless/channel behavior, and explicit Nix pkgs.chromium; observed causes of screenshot differences are documented rather than assumed.
- [ ] #8 The chosen reproducible Chrome/Chromium execution mode and rationale are documented, including fonts/fallback, CSS/font assets, GPU/software rendering, viewport, and VM-versus-deployment differences.
- [ ] #9 Existing per-step dark/light screenshots and failure screenshots remain available.
- [ ] #10 Existing visual baseline/diff and design-parity artifacts remain available, including raw captures, montages, matrix, drift JSON, and drift Markdown.
- [ ] #11 Existing OSCAL/SARIF validation remains merge-blocking as before; result JSON and final screenshots are not regressed and are exposed where practical.
- [ ] #12 At least one deterministic verification video demonstrates actual Crystal Forge interaction.
- [ ] #13 At least one deterministic video is intentionally suitable as a polished Crystal Forge demo, with realistic deterministic data, human-readable pacing, no debug garbage, and no unexplained blank/loading states; its post-!316 storyboard is documented.
- [ ] #14 Verification videos are a useful curated set rather than hundreds of tiny recordings, and are associated with manifest/report workflow entries.
- [ ] #15 Video filenames/paths are stable; chapter metadata, if used, is represented offline through a stable sidecar/report mapping. Playwright MCP is not required without compelling justification.
- [ ] #16 Videos are copied from the VM into the Nix result and playable directly from result/ without locating /tmp or reading build logs.
- [ ] #17 A self-contained offline human-readable report/index is produced, preferably result/reports/index.html, covering status, profile, browser metadata, workflow grouping, critical/advisory status, descriptions, screenshots, visual comparison, design parity, videos, export validation, failures, and failure screenshots.
- [ ] #18 Machine-readable JSON and useful Markdown remain available for coverage, execution, browser environment, visual results, design drift, and export validation.
- [ ] #19 The result has a documented stable artifact tree with obvious screenshot, video, report, design-parity, diff, and export locations discoverable after normal nix build.
- [ ] #20 Artifact generation works offline inside the Nix build after declared/fetched inputs; the report does not require a network server or external assets.
- [ ] #21 Baseline runtime before changes, screenshot/report runtime, and video runtime are measured comparably and reported in the MR.
- [ ] #22 If video materially harms runtime, storage, flakiness, or merge-gating reliability under the documented criterion, it is split into a separately exposed check with an exact documented name and command, reusing fixtures/harness without duplicating the full suite.
- [ ] #23 The normal Web UI check is not materially less reliable without justification; critical, strict visual, design-parity, OSCAL, and SARIF semantics remain intact.
- [ ] #24 Documentation explains how to run normal web-ui, full, focused, and video/demo variants, including exact supported commands.
- [ ] #25 Documentation explains the artifact tree, report entry point, browser decision, verification/demo distinction, baseline workflow, and how to open/play artifacts.
- [ ] #26 CI preserves the existing screenshot/MR-comment workflow and exposes new report/video directories as reviewable artifacts without secrets or network-dependent reports.
- [ ] #27 Changes remain focused on check/test-driver/reporting/artifact/documentation architecture; no unrelated redesign or broad @playwright/test migration is introduced.
<!-- AC:END -->
