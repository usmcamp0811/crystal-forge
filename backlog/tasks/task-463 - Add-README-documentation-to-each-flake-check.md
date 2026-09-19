---
id: TASK-463
title: Add README documentation to each flake check
status: In Progress
assignee: []
created_date: '2026-09-19 15:12'
updated_date: '2026-09-19 15:12'
labels: []
dependencies: []
documentation:
  - checks/web-ui/baselines/README.md
priority: low
type: docs
ordinal: 481000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a `README.md` to each top-level directory under `checks/` (each is a flake check attribute discovered by snowfall-lib's convention: `checks/<name>/default.nix` becomes `checks.<system>.<name>`). Each README must explain, in the repository's documentation style (ASD-STE100-influenced, exact and concrete, no filler):

- What the check verifies and why it exists as a distinct check (rather than folding into another check or the normal package build).
- How to run it locally, including the exact `nix build .#checks.x86_64-linux.<name>` command and any required flags (for example `--impure`, `-L`, `CF_UI_TEST_STEPS=...`).
- What it does NOT cover, when that boundary is intentional and non-obvious (for example: OIDC-only auth flow vs. local auth; core server build vs. production embedded-UI build; VM-based vs. host-runnable).
- Important operational details: approximate runtime/timeout, whether it boots a NixOS VM, whether it needs network access during the Nix build (impure fetches), whether it is wired into `.gitlab-ci.yml`'s `flake-check` matrix or runs only via a broader `nix flake check`, and any environment variables that change its behavior.
- Pointers to closely related files in the same directory (test scripts, fixtures, manifests) when useful for a maintainer making a change.

Checks currently discovered under `checks/`:
- `oidc-auth` — NixOS VM test for the OIDC authentication flow against Keycloak.
- `integration` — NixOS VM test for server/database/Grafana dashboard integration (core server build, agent enabled, builder disabled).
- `server-regressions` — PostgreSQL-backed Rust integration/regression tests for cf-server that are outside the normal package build (`--lib --bins`), including a migration-upgrade rehearsal.
- `nixos-options-metadata` — pure Nix evaluation check validating the extracted NixOS options metadata package.
- `oscal-export` — validates a generated OSCAL Assessment Results document against vendored NIST 1.1.2 schemas.
- `xccdf-schema` — validates vendored XCCDF 1.2 and the CF-XCCDF extension schema against writer-generated and hand-authored fixtures, plus OpenSCAP validation.
- `stig` — pure Nix unit tests for `mkStigModule`'s priority/override/merge semantics (no VM, no build).
- `run-ui-dev-db-check` — regression test for `run-ui-dev`'s database usability probe and `db-only-start.sh` working-directory behavior.
- `web-ui-reconciliation` — focused NixOS VM Playwright check for one STIG-import reconciliation workflow against the static web-ui build (no backend server).
- `ui-screenshots` — lightweight, backend-free Playwright screenshot capture of the Dioxus web UI against fixture data (also exposed as a package).
- `web-ui-test-runner` — host-side regression test for the `web-ui-test` runner script's workflow-selection/readiness/exit-status contract (no VM, no services).
- `web-ui` — the authoritative manifest-driven Playwright integration check against a real Crystal Forge server (production embedded-UI build), including OSCAL/SARIF export validation and non-blocking design-parity comparison. This is the most detailed check and its README should reflect the scale of what it does.

This is a documentation-only task. Do not change check behavior, `.gitlab-ci.yml`, `flake.nix`, or any `default.nix` file under `checks/`. `checks/web-ui/baselines/README.md` already exists for the baselines subdirectory specifically and is out of scope; do not duplicate or replace it (the new `checks/web-ui/README.md` may reference it).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every top-level directory directly under `checks/` that contains a `default.nix` (i.e., is a discovered flake check attribute) has a `README.md`.
- [ ] #2 Each README states what the check verifies, why it is a separate check, how to run it with the exact `nix build .#checks.x86_64-linux.<name>` invocation (including any required flags/env vars), and any intentionally out-of-scope behavior.
- [ ] #3 Each README accurately reflects the check's actual current implementation (VM vs. non-VM, core vs. production server build, CI matrix membership) as read from its `default.nix` and `.gitlab-ci.yml`; no fabricated details.
- [ ] #4 No `default.nix`, `flake.nix`, or `.gitlab-ci.yml` behavior is modified.
- [ ] #5 The existing `checks/web-ui/baselines/README.md` is left untouched.
- [ ] #6 Markdown is well-formed and passes `git diff --check` (no trailing whitespace / whitespace errors).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Read every checks/<name>/default.nix to extract accurate behavior (VM vs. non-VM, server build variant, timeouts, env vars). Cross-reference .gitlab-ci.yml's flake-check matrix to state CI membership accurately. Write one README.md per check directory in the repository's documentation style. Verify with git diff --check and a visual read-through; do not run nix flake check for a docs-only change unless a formatting concern arises.
<!-- SECTION:PLAN:END -->
