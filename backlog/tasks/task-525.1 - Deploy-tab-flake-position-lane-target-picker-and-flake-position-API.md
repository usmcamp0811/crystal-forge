---
id: TASK-525.1
title: Deploy tab flake-position lane, target picker, and flake-position API
status: In Progress
assignee: []
created_date: '2026-11-15T16:11:00Z'
updated_date: '2026-11-16T05:26:00Z'
labels:
  - web-ui
  - server
  - deployments
  - design-parity
dependencies: []
references:
  - commit:bd8d068dc287d222baeea9c7336363a03da55371
documentation:
  - docs/design/CrystalForge/components/FlakePosition.jsx
  - docs/design/CrystalForge/components/DeployTargetPicker.jsx
  - docs/design/CrystalForge/components/DeployGate.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/MgmtParts.jsx
modified_files:
  - packages/default/crates/cf-server/src/
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/system_detail/management_detail.rs
  - packages/web-ui/src/components/system/
  - packages/web-ui/assets/app.css
parent_task_id: TASK-525
priority: high
type: feature
ordinal: 573000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Dev commit `bd8d068d` redesigned the System Detail Deploy tab. The Dioxus Deploy tab (`packages/web-ui/src/views/system_detail.rs`, `DeployTab`) still uses the earlier layout:

- A full-width `DeployGatePanel` card sits above a two-column grid.
- A flat "Select target" list offers "New commit" and "Previous generation" modes.
- The deploy progress banner (`PendingDeployBanner`) sits above the tabs.

The new design adds a flake-position lane. The lane shows where the system is in the branch history of its flake, which commits are newer, and which other hosts run each commit. Operators can pick a deploy target from the lane, and the lane shows deployment progress. The design also adds the same card to the External/Reference overview.

The server has no API that returns the position of a system in its tracked branch. `GET /api/v1/systems/:id/commits` returns the 50 newest commits by timestamp. It does not filter by branch, it does not use `flake_branch_commit_snapshot.position`, and it does not return peer hosts. Only the dashboard timeline (`fetch_dashboard_flake_timelines`) returns hosts per commit.

## Goal

Implement the `bd8d068d` Deploy tab and External/Reference overview design in the Web UI. Back it with a per-system flake-position read API.

Design references (authoritative for this task):

- `docs/design/CrystalForge/components/FlakePosition.jsx` (`flakePositionOf`, `fpWindow`, `FlakePositionCard`)
- `docs/design/CrystalForge/components/DeployTargetPicker.jsx`
- `docs/design/CrystalForge/components/DeployGate.jsx` (`DeployGatePanel` `compact`)
- `docs/design/CrystalForge/components/SystemDetail.jsx` (`DeployTab`, banner suppression on the Deploy tab)
- `docs/design/CrystalForge/components/MgmtParts.jsx` (`ExternalOverview`)
- `docs/design/CrystalForge/styles.css` (`/* Flake position lane */` block, `tp-*`, `dg-compact`)
- `docs/design/CrystalForge/scraps/0[1-4]-fp-check.png` and `uploads/pasted-1794*.png`

## Non-goals

- The `copying` stage backend. TASK-449 owns it. This task keeps one stage list shared by the banner and the lane, so that TASK-449 adds `copying` in one place.
- Approval requests and approval tags, and the attestation chip with "Decide". TASK-415 owns them. This task does not render placeholders for them.
- A deploy-gate evaluation API (TASK-353.2). The compact gate keeps the current locally derived rules.
- A flake-position card on the managed Overview tab. The design does not render one.
- Changes to deployment authorization, policy enforcement, rollback eligibility, or the `/commits` endpoint contract.

## Architectural constraints

- The server owns the position, the peer, and the build-state derivation. The Web UI reads them only through the new API. The UI does not read the database or compute peers from other endpoints.
- The new endpoint is read-only. It uses the same authorization and environment scoping as `GET /api/v1/systems/:id/commits`. Peer hostnames include only systems that the caller can access.
- The running commit comes from the existing observational current-revision resolver (`resolve_observational_current_revision`). The endpoint does not fall back to HEAD for an unmapped running output (systems-view design §6.1).
- Branch order comes from `flake_branch_commit_snapshot.position` when `flakes.snapshot_ready_at` is set. Otherwise the endpoint uses the documented timestamp fallback, and the response flags it.
- No schema change is expected. If one becomes necessary, add a new migration and update the SQLx metadata.
- Put pure view logic in tested helpers outside RSX: the lane window, the picker collapse rule, relative labels, and the deploy distance and direction.

## Impact areas

- `packages/default/crates/cf-server`: a new handler and route, the query, and the API models.
- `packages/web-ui`: the API client and models, `views/system_detail.rs` (`DeployTab`, `DeployGatePanel`, banner placement), `views/system_detail/management_detail.rs` (External/Reference overview), new components under `components/system/`, and `assets/app.css`.
- Integration fragments: `checks/integration-delivery/scenarios/fragments/activation.cjs` (Deploy tab) and `checks/integration-assurance/scenarios/fragments/configuration-assurance.cjs` (External/Reference overview).
- Docs: `docs/knowledge/ui/systems-deployment-progress-spec.md`, the systems API knowledge concept, and `docs/knowledge/testing/integration-coverage-map.md`.

## Risk

Medium. The task adds a new read API with environment-scoped peer data, which is an information-disclosure boundary. It also restructures a large view. Deploy actions, auto-latest prompts, retry requests, and RBAC-disabled states must keep their current behavior.

## Dependencies

None blocking. TASK-449 and TASK-415 extend this UI later.

## Verification plan

- Server: unit tests for position derivation. Isolated-database tests for the endpoint, covering:
  - on HEAD, N behind, and untracked;
  - a removed configuration;
  - no snapshot (timestamp fallback);
  - peers filtered by environment;
  - 404 for an inaccessible system.

  Run them through `nix develop` with the commands in `docs/agents/verification.md`.
- Web UI: `cargo test` for the new pure helpers, and a WASM build of `packages/web-ui`.
- Live preview from this worktree with fixture data. Compare against the design screenshots in dark and light themes.
- Integration:
  - `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`
  - the Assurance scenario that owns `configuration-assurance.cjs`

  Both must pass at the review revision, with MR screenshots.
- Docs: `nix build .#okf-knowledge -L`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 GET /api/v1/systems/:id/flake-position (viewer or above, environment-scoped, 404 for an inaccessible system, read-only) returns the flake identity, the tracked branch, an ordered branch history (position 0 = HEAD) with sha, message, author, and committed_at, a flag for snapshot or timestamp-fallback ordering, the running position or a typed unknown reason (`no_history`, `not_observed`, `untracked`, `off_branch`, or `beyond_window`), and per-commit peer hostnames of other systems that run that commit and that the caller can access. The list is bounded to 500 commits from HEAD. It always includes HEAD and includes the running commit when that commit is within the bound; a running commit deeper than 500 commits returns `beyond_window`. The `configuration_removed` state for External and Reference systems is derived in the Web UI from configuration-assurance `availability = removed`, which owns that signal; it is not a flake-position reason. (Amended 2026-11-16 at the owner's decision after MR review P2-3.)
- [ ] #2 The response includes a per-commit build state for this system's configuration that the server derives from existing derivation, build-job, and cache-publication records: `ready` (a build job succeeded and a completed cache push exists for the output, the evidence `view_system_deployment_status` requires), `unpublished` (a build job succeeded without that cache push), `building`, `failed`, or `not_built`. The picker shows a building indicator only for `building` and an "awaiting cache" marker only for `unpublished`. No state is fabricated. (Clarified 2026-11-16 after MR review P2-7.)
- [ ] #3 The Deploy tab layout matches bd8d068d. The flake-position card is full width above a two-column grid with the Deploy target picker on the left and the deployment or rollback plan panel on the right. The top-level DeployGatePanel card is removed, and a compact gate (dg-compact) renders inside the plan panel above the action buttons.
- [ ] #4 The flake-position card renders the design summary states (on the latest commit, N commits behind, not in history), the matching chip, a horizontal lane windowed like fpWindow with +N commits gap pills, the host pin on the running commit, the HEAD tag, dashed ahead segments, +N hosts peer markers with hostnames in the tooltip, a focus row that updates on hover and keyboard focus, and the store-path row.
- [ ] #5 Unknown positions render the design copy for untracked, configuration-removed, and no-history states. View history switches to the History tab. Capture to flake (untracked only) opens the flake commit peek for the system's flake without a commit focus.
- [ ] #6 On the Deploy tab, a lane dot selects that commit as the target (or the matching generation in Generations mode). The selected node shows the selected pin. The focus row shows N ahead of or N before the host, and the hint text appears. The picker selection and the lane selection stay synchronized in both directions.
- [ ] #7 The Deploy target picker matches DeployTargetPicker.jsx: Commits and Generations tabs with role=tab and aria-selected, the subtitle (N newer than what's deployed, or deployed commit is HEAD), vertical-rail rows with message, sha link, author, and time, a host tag on the running commit, a HEAD tag, collapsing to max(8, running index + 4) rows with Show N older commits or Show fewer, a selected row that is always kept visible, gap rows, scroll-into-view on selection, and top and bottom fade on scroll. The Commits picker and the lane use the same server-ordered history.
- [ ] #8 Generation rows show the generation number, the sha link or no commit, the time, an active tag on the current generation, and an origin warning derived only from real fields (no commit hash, not rollback-eligible). The mapping is documented in code.
- [ ] #9 While a deployment progress record exists, the card shows deploy mode: the headline (Deploying, Rolling back, or Deployment/Rollback complete with the target sha or generation), N commits forward or back (or from an untracked config), the stage subtext, a Logs button that opens the Logs tab, a dismiss button after activation, the target and was here pins, and the stage branch under the lane spanning from the running commit to the target (with the same-commit loop variant). The failed state shows the server failure message. Stages come from the single list shared with PendingDeployBanner.
- [ ] #10 PendingDeployBanner is not rendered on the Deploy tab of managed systems. Its behavior on other tabs is unchanged.
- [ ] #11 Existing deploy behavior is preserved: the auto-latest prompt, retry requests, the RBAC-disabled label, Verify closure, rollback-eligibility gating, deploy notices, and initial generation preselection from History.
- [ ] #12 The External/Reference overview renders the flake-position card after the evidence pipeline, with the declared or tracked verb, no picking, and no deploy mode, positioned at the revision whose evidence is shown. It shows configuration_removed when assurance availability is removed.
- [ ] #13 Styles from the bd8d068d styles.css (Flake position lane, tp-*, dg-compact) are ported to packages/web-ui/assets/app.css using existing tokens. The layout stays usable at the existing narrow breakpoint.
- [ ] #14 Tests: server unit and isolated-database tests cover on-HEAD, behind, untracked, configuration-removed, timestamp-fallback, environment-filtered peers, and an inaccessible system. Web UI unit tests cover the lane window, the picker collapse rule, relative labels, and the deploy distance and direction.
- [ ] #15 Integration: the Delivery delivery-lifecycle activation fragment asserts the Deploy tab card and picker in the behind, deploying, and activated states. The Assurance configuration-assurance fragment asserts the External/Reference overview card, including the removed configuration. Both pass at the review revision, with dark and light screenshots attached to the MR.
- [ ] #16 The docs/knowledge deployment-progress spec, the systems API concept (new endpoint contract), and the integration coverage map are updated. nix build .#okf-knowledge -L passes.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus-5-5 on vm-agent in /home/mcamp/code/crystal-forge/TASK-525-deploy-tab-flake-position

Progress 2026-11-15 (branch TASK-525-deploy-tab-flake-position, pushed via altssh.gitlab.com:443 because port 22 times out):
- Implemented: GET /api/v1/systems/:id/flake-position (queries/flake_position.rs), FlakePositionCard + DeployTargetPicker components, compact gate in the plan panel, banner hidden on the Deploy tab, External/Reference overview card, CSS port, docs (systems-api, deployment-progress spec, coverage map), Delivery/Assurance browser fragment assertions.
- AC #1 deviation: unknown reasons are no_history, not_observed, untracked, off_branch, beyond_window. configuration_removed is derived in the UI from configuration-assurance availability=removed, because the server already owns that signal there. The response also has last_tracked_position (newest commit the host has run) for the 'last tracked' marker.
- AC #9 note: the U connector uses CSS percentages from equal grid tracks instead of runtime DOM measurement; the same-commit case uses the same U, not the design SVG loop. Copying stage remains with TASK-449.
- Verified: farm .#packages.x86_64-linux.web-ui EXIT 0 (includes 16 new/affected unit tests). Farm cf-server/cf-server-core checkPhase: new flake_position unit tests pass; build fails on 15 unrelated parser-timeout tests (TASK-526). Endpoint verified against the task-owned preview DB: 403 unauthenticated, 404 missing, behind/head/untracked/no_history states, peers. Browser (Playwright, farm-built embedded-UI server): lane/picker/plan selection sync, gate present, banner absent, no page errors. Fixed: default selection now follows the running commit after load.
- Not yet run: isolated-DB tests tests/flake_position.rs (#[ignore], needs a disposable DATABASE_URL), integration-delivery and integration-assurance owners, MR screenshots. Ruled out: local cargo builds (farm-only per AGENTS.md).

MR prep 2026-11-15 at afae8d19: farm .#packages.x86_64-linux.web-ui passed (870 tests); farm cf-server-drv with doCheck=false built for the preview; full checkPhase blocked only by TASK-526. Browser preview found and fixed three defects (default selection, no_history fallback, plan From row). Integration owners delivery-lifecycle and evidence-remediation-lifecycle: not run (two attempts died on host 'Stale file handle' and a farm signal 6 before any test). Task stays In Progress until owners pass.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/343 (Draft, target dev). Created with git push options because glab has no API token on this host.

Owner-reported bug fixed (f998583e, b1f55f4b): a same-commit redeploy drew the U between the neighboring commits, so it read as a deploy from the older neighbor; it now draws the design loop out of and back into the running dot. The deploy-mode commit row has a fixed 104px height so dot offsets are known without DOM measurement; this supersedes the earlier same-commit deviation note. Also fixed inverted stage direction for forward deploys (Queued now starts at the running commit). Farm web-ui 872 passed; preview verified: loop top and center match the running dot (y 629, x 789), forward deploy reads Queued to Activated from the host toward the target.

Owner review round 2 (2026-11-16): (1) lane shows picked targets during an active deployment and keeps target + pick in the window (fad10069); (2) plan Show diff reads GET /flakes/:id/commits/:sha/diff (Flakes view source) with loading/error/empty states, mock removed (fad10069); (3) running position = newest branch commit whose nixos output equals the running store path, matching view_system_deployment_status, resolver only as fallback (2dc11c39); (4) owner asked to fix TASK-453 AC #10 here: record_report_events_tx completes a still-pending deployment whose target equals the unchanged reported store path (90a7eee9). Verification: farm web-ui 873 passed; farm sandbox PostgreSQL run of tests/flake_position.rs (3) and tests/already_current_deployment.rs (2): 5 passed (build overrides in ~/.cache/cf525/db-tests.nix); preview: atlas-01 with shared HEAD closure placed on HEAD, picker selection drawn during deploy, diff panel shows real server 404 for demo commits. Still not run: integration owners.

Owner review round 3 (2026-11-16): progress label for shared-output deployments now uses requested_commit_id, else the newest commit with that output by branch position (6048710e, 13856446); untracked-host reference lane is pickable on the Deploy tab instead of opening the flake drawer (6f95467a). Farm: web-ui 873 passed; sandbox PostgreSQL DB tests 6 passed (flake_position 4, already_current_deployment 2). Preview: untracked host gaia-web-03 lane click selects target in lane, picker and plan; no drawer; no page errors.
<!-- SECTION:NOTES:END -->
