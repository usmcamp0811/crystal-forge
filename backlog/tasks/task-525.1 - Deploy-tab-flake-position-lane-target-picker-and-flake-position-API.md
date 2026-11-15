---
id: TASK-525.1
title: Deploy tab flake-position lane, target picker, and flake-position API
status: To Do
assignee: []
created_date: '2026-11-15T16:11:00Z'
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
- [ ] #1 GET /api/v1/systems/:id/flake-position (viewer or above, environment-scoped, 404 for an inaccessible system, read-only) returns the flake identity, the tracked branch, an ordered branch history (position 0 = HEAD) with sha, message, author, and committed_at, a flag for snapshot or timestamp-fallback ordering, the running position or a typed unknown reason (untracked, no_history, or configuration_removed), and per-commit peer hostnames of other systems that run that commit and that the caller can access. The list has a documented bound and always includes HEAD and the running commit when it is on the branch.
- [ ] #2 The response includes a per-commit build state for this system's configuration (ready, building, failed, or not_built) that the server derives from existing derivation and publication records. The picker shows the building indicator only for building. No state is fabricated.
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
