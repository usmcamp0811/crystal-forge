---
id: TASK-450.3
title: >-
  Separate the core server build from the embedded-UI server so web UI changes
  stop rebuilding the backend
status: Review
assignee: []
created_date: '2026-08-31 22:39'
updated_date: '2026-09-01 02:05'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/324'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
modified_files:
  - packages/default/default.nix
  - modules/nixos/crystal-forge/default.nix
  - checks/integration/default.nix
  - checks/oidc-auth/default.nix
  - checks/web-ui/default.nix
  - lib/server-test-node/default.nix
  - packages/dev-env/composition.nix
  - packages/devScripts/default.nix
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The single server derivation always embeds the web UI. `packages/default/default.nix:110` passes `--features cf-server/embedded-ui` and `packages/default/default.nix:119` sets `CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public"`.

The web UI derivation is therefore a Nix input of the backend server derivation. A Dioxus-only change rebuilds the backend Rust server, and with it every check that boots a server, including `integration` and `oidc-auth` which do not exercise the browser UI at all.

This is the single most damaging invalidation edge for anyone working on the front end.

## Goal

Provide a server build without the embedded UI for the consumers that do not need it, and keep a server build with the embedded UI for production packaging and for the authoritative browser check.

`crates/cf-server/Cargo.toml:8` already declares `embedded-ui` as an optional feature with no default features, so the Rust crate already supports both shapes.

## Consumer split

Consumers that must not depend on the web UI derivation:

- the integration check
- the oidc-auth check
- server unit and regression test execution
- backend development workflows

Consumers that must keep the embedded UI:

- the production server package and the NixOS module service
- the authoritative pre-merge browser check

## Non-goals

- Removing the `embedded-ui` Cargo feature or changing how it serves assets when enabled.
- Changing the web UI build itself.
- Changing which check is authoritative for browser evidence.
- Making the core server variant a published production artifact.

## Architectural constraints

- The production guarantee must not weaken. At least one pre-merge check must still exercise the production server binary serving the embedded production WASM through a real browser.
- A server binary built with the embedded UI must behave identically to today.
- Requests for UI routes against a core server build must fail in a clear, documented way rather than silently returning an empty or misleading response.
- Both variants must come from the same crate source and differ only by Cargo feature selection and the UI asset input.

## Verification plan

- Compare server derivation paths before and after a scratch edit under `packages/web-ui`, for both the core variant and the checks that consume it.
- Build the integration and oidc-auth checks and confirm they pass against the core variant.
- Build the authoritative browser check and confirm it still passes against the embedded variant.
- Exercise a UI route against a core server build and confirm the documented failure behavior.

## Impact areas

`packages/default/default.nix`, the NixOS module service definitions, `checks/integration`, `checks/oidc-auth`, `checks/web-ui`, and any lib helper that starts a server.

## Risk level

Medium. The risk is not build breakage but silent loss of the production embedded-UI guarantee, or a check accidentally validating a variant that is not what production ships.

## Dependencies

None, but it is intended to land together with the server source filtering subtask.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A server build exists that does not take the web UI derivation as an input
- [x] #2 A server build exists that embeds the web UI and is used for the production package and the NixOS module service
- [x] #3 A source-only change to the web UI does not change the derivation hash of the server used by the integration and oidc-auth checks, demonstrated by comparing derivation paths before and after the edit
- [x] #4 The integration and oidc-auth checks pass using the server build that has no web UI input
- [x] #5 One pre-merge check still exercises the production server binary serving the embedded production WASM in a real browser, and that check passes
- [x] #6 The behavior of a core server build when it receives a request for a UI route is defined, implemented deliberately, and documented
- [x] #7 The Nix source documents which server build each consumer uses and why, so a future change does not silently reintroduce the web UI dependency for backend-only checks
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude-opus-5 on gray in /home/mcamp/code/crystal-forge/TASK-450-p0-build-graph

Implemented together with TASK-450.1, TASK-450.2, and TASK-450.4 in a single MR at the user's explicit direction.

Final web UI invalidation probe: embedded server changed l4c061z… to zpx7x1q…; core server stayed p9lw9l1…; integration and oidc-auth stayed unchanged after residual whole-flake inputs were removed; web-ui changed as required.

The core server registers no axum UI fallback and therefore returns the framework default 404 Not Found for UI routes. The embedded production server remains the module default and the web-ui check explicitly selects it.

Verification passed: core and embedded package builds, integration, oidc-auth, web-ui, and one complete nix flake check --keep-going -L run.

LOCK RELEASED: implementation is pushed and MR !324 is awaiting review.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created shared core and embedded-UI server variants from the same crate source. API-only checks and development paths use the core server with no web UI input. Production and the authoritative browser check retain the embedded server. Web UI changes no longer invalidate integration or OIDC.
<!-- SECTION:FINAL_SUMMARY:END -->
