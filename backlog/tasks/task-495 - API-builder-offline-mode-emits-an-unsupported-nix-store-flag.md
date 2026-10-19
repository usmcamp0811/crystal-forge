---
id: TASK-495
title: API builder offline mode emits an unsupported nix-store flag
status: Review
assignee:
  - '@openai'
created_date: '2026-10-19 05:37'
updated_date: '2026-10-19 19:23'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-builder/src/derivations/build.rs
  - modules/nixos/crystal-forge/default.nix
  - checks/integration-delivery/scenarios/fragments/source-build.py
  - /tmp/opencode/delivery-local-run12.log
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717'
modified_files:
  - packages/default/crates/cf-config/src/config/build.rs
  - modules/nixos/crystal-forge/default.nix
priority: high
type: bug
ordinal: 546000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real API-only builder received and materialized an authorized NixOS derivation, then failed before realization because offline=true passed --offline to nix-store 2.34.8, which rejects that flag. The owner explicitly authorized a separate product fix after this defect blocked the unchanged Delivery proof. This is not the already-resolved state-machine CI networking/Bash issue.

The correction lives in shared BuildConfig argument generation. It replaces the modern-only flag with compatible acquisition settings for both legacy nix-store and modern Nix, while preserving resource/sandbox and ordinary online behavior. Offline mode disables substitution, substituters, remote builders and fallback; it is not an OS firewall for fixed-output builders or a prohibition on required API/publication traffic. Full network isolation remains a host responsibility.

The implementation was developed in its dedicated TASK-495 worktree and committed separately. It is applied to the coordinator branch with offline=true and all actual realization/output assertions retained. No schema or protocol transition is involved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A configured offline API-only builder can realize an authorized derivation using the supported Nix CLI without an unsupported-flag error.
- [x] #2 Offline realization cannot acquire external substitutes or use remote builders or network fallback.
- [x] #3 Tests cover the effective invocation and a real successful local realization with its output absent before execution.
- [x] #4 Affected configuration and maintainer documentation describe the actual offline guarantee and missing-prerequisite failure.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-authorized separate product correction. Worktree /home/mcamp/code/crystal-forge/TASK-495-builder-offline, branch TASK-495-builder-offline, base dev at f3db8618. No main worktree is checked out; dev has no tracked modifications and its unrelated locks/session file remain untouched. Task lock is active.
1. Correct the shared BuildConfig::apply_to_command offline branch in cf-config, which currently appends modern-only --offline even when the caller is nix-store --realise. Use legacy-compatible Nix options that explicitly disable substitution, external substituters, remote builders and fallback. Keep ordinary online/resource/sandbox behavior unchanged and preserve all actual build assertions.
2. Add focused ordinary cf-config argument tests for direct nix-store, systemd-scoped nix-store and modern Nix callers, including offline overriding use_substitutes=true and online behavior unchanged. No new gate or testing framework.
3. Update adjacent public Rust contracts and the existing NixOS build.offline option documentation. Distinguish Nix input-acquisition policy from an OS network firewall and from API/publication traffic.
4. Run targeted cf-config tests, formatting and rustdoc through Nix, then build the real builder package. Commit the bounded product fix separately. Apply that reviewed product commit to the coordinator branch without modifying its Delivery assertions/configuration. Run the same offline=true Delivery proof to establish actual absent-before/present-after Nix realization; retain precise failures and no xfail. Do not merge/deploy or mark Done.

The planned implementation and verification are complete at code revision bea0c299. Hosted pipeline2964502717 passed all eight required jobs. The task is awaiting review in draft MR!335; release its implementation lock and retain its worktree until merge.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verified repair and review vehicle

- Status: Review, not Done. Draft MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335.
- Dedicated implementation worktree: `/home/mcamp/code/crystal-forge/TASK-495-builder-offline`.
- Independent product commit: `6844d185`; coordinator application: `fc0c5836`.
- Verified code revision: `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde`.
- Hosted pipeline2964502717 succeeded. All eight required jobs passed, including real Access/Delivery, component, database, CVE, dashboard, complexity and coverage execution.

The archived Delivery result records all eight mandatory cases passed with offline=true, independently absent-before/realized-after nonce output, signed local publication and exact successful job/session/output identities. Public egress rejection remained active. No offline=false workaround, expected failure or assertion bypass was used. Job17294404507 retains the artifact under `integration-evidence/integration-delivery/delivery-lifecycle/`.

## Focused and component checks

- `nix develop .#sqlx -c env SQLX_OFFLINE=true cargo test --offline --manifest-path packages/default/Cargo.toml --package cf-config --lib config::build::tests`: 5 passed.
- New ordinary regressions cover legacy, modern and scoped command arguments, offline overriding substitution, and unchanged online policy.
- The cf-config doctest passed. `cargo doc --offline --manifest-path packages/default/Cargo.toml --package cf-config --no-deps` succeeded through the SQLx shell with three existing bare-URL warnings in builder.rs/oidc.rs; no suppression was added.
- The coordinator cf-config suite passed all96tests. Normal builder package checks passed99library/34binary/2integration cases; agent checks passed15/11/1. Identical package results may be reused by Nix; no forced-rebuild claim.
- Targeted rustfmt, whitespace checks and legacy nix-store option parsing passed. Actual Nix2.34.8 realization then succeeded through the same offline=true builder path.

## Contract and limits

Shared Nix options disable substitution, substituters, remote builders and fallback without using modern-only --offline. Ordinary online/resource/sandbox behavior remains unchanged. Rustdoc and the NixOS option description distinguish acquisition policy from an OS firewall and required API/publication traffic. Required source inputs must be locally available; no external fallback supplied the successful test result.

All four acceptance criteria are verified. The MR is not merged, no deployment occurred, and the worktree is retained. TASK-496 heartbeat classification and the broader test-overhaul gaps are separate work.
<!-- SECTION:NOTES:END -->
