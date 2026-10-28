---
id: TASK-498
title: Investigate signature propagation for preexisting SSH cache dependencies
status: In Progress
assignee:
  - openai
created_date: '2026-10-20 03:51'
updated_date: '2026-10-20 22:34'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-config/src/config/cache.rs
  - packages/default/crates/cf-builder/src/derivations/build.rs
  - packages/default/crates/cf-agent/src/deployment/agent.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: medium
type: bug
ordinal: 549000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A successful completed publication to a populated supported destination must leave the required closure readable by a target using the configured CF trust. The real Delivery workflow showed recursive source signing and successful three-path transfer, followed by agent rejection of an already-valid destination dependency without a trusted signature. Source signing and destination metadata propagation are separate operations. The owner now authorizes the smallest supported warm-cache publication/readability repair and missing-dependency proof in the existing topology; do not trust receiver-test keys or disable signatures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A reproduction distinguishes source recursive signing from signature metadata available for preexisting destination dependencies.
- [ ] #2 The supported publication and target-read trust contract for a warm SSH destination is documented.
- [ ] #3 If the contract requires signature propagation then a focused repair and regression demonstrate full-closure target acquisition without disabling signatures or trusting test-side receiver keys.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Dedicated worktree/branch TASK-498-publication-trust, clean base df11c800. Sol owns topology/registration and final integration; worker owns cf-config cache command construction and real Nix publication execution paths/tests and their adjacent contracts.
1. Inspect pinned Nix and supported Nix/SSH publication contract. Reproduce populated-destination copy skipping signatures on already valid paths, separately from source recursive signing.
2. Preserve recursive signing and normal transfer; explicitly synchronize the valid configured source signatures across the required destination closure using the supported pinned Nix operation. Publication completion must fail if required trust synchronization fails. No receiver-test key, disabled checks or privileged target import.
3. Use focused isolated-store command/regression tests and negative propagation-failure tests; no new public check/package or test-specific VM. Preserve other cache adapters' semantics and redaction.
4. Commit repair separately. Sol adds a missing-dependency acquisition proof inside the existing five-role Delivery lifecycle while retaining ordinary prepared activation and the original enforced receiver proof.

Owner follow-up after 07616c56 (integrated by Sol as 18ef4274): bounded new commit in the same worker worktree. Sign the resolved server Nix publication root with the selected attempt cache configuration before transfer; API builder signing remains unchanged. For legacy ssh:// return only recursive configured-key destination verification, so complete CF trust succeeds despite unrelated missing signatures and missing CF trust fails without unsupported writes. Add focused regression/error tests and make local-store fixture virtual store configuration explicit for Nix build sandboxes. Sol owns default.nix test wiring; worker does not edit packaging or start databases/VMs.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner authorization: current TASK-490 assignment selects TASK-498 for bounded production repair. Failed reproduction: /nix/store/cbxbrqalva8jm45crs4bx4gfhpmz0jm6-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Ordinary activation may share immutable prerequisites but cannot replace the separate missing-dependency acquisition assertion. Dedicated product worktree from df11c800; coordinator owns integration topology/registration.

Publication worker: implemented in TASK-498-publication-trust from df11c800. Paths: packages/default/crates/cf-config/src/config/cache.rs; cf-builder/src/derivations/{build,cache}.rs; necessary server counterpart cf-server/src/derivations/{build,cache}.rs. No topology, harness, CI, Nix module or packaging edits. Pinned Nix 2.34.8 src/nix/sigs.cc uses `store copy-sigs --substituter SOURCE --store DEST --recursive OUTPUT` (not --from), compares NAR hash/size/references and silently skips mismatches. Worker therefore also requires recursive destination `store verify --no-contents --sigs-needed 1`, clearing ambient trusted-public-keys/extra-trusted-public-keys and using only configured secret-key-files to derive the public half locally. src/libstore/daemon.cc AddSignatures has no trusted-client guard; ssh-ng supports it, legacy ssh lacks addSignatures. Signed Nix publication fails on propagation/verification failure; other adapters keep native semantics.

Verification so far, all exit 0: offline cf-config config::cache::tests with --include-ignored --nocapture (4 passed), offline cf-builder derivations::cache::tests (14 passed), offline cf-config --doc (16 passed), offline cf-server --lib cargo check, and explicit rustfmt --check on the five changed files. Real regression invoked with TMPDIR=/tmp/opencode and NIX_TEST_BIN=/nix/store/s9x74dmyx8wphr49pxxpnh2yrxwla1np-nix-2.34.8/bin/nix, after verifying parent and preparing evaluatorNix from cache.nixos.org. Three invocation-owned local Nix stores only; no shared PostgreSQL or daemon stack. Evidence: source CF signatures 2/2; destination before 1/2 and after 2/2; before propagation verify exit 2 and signature-enforcing consumer copy exit 1; after propagation acquisition exit 0; repeat propagation preserves metadata. Wrong warm-path key remains but consumer trusts only CF public key. The real Nix test is an explicitly selected ignored Cargo test because ordinary Cargo/package environments do not supply Nix. Initial workspace cargo fmt command exited 1 ('Failed to find targets'); explicit rustfmt on scoped files passes. Owner still owns SSH topology execution and knowledge-base composition.

Worker commit: 07616c5657dfb2616b7632edf814c515f5d10f93 (TASK-498-publication-trust, parent df11c800). Five intended files committed; worktree clean; no push. Exact preparation commands (exit 0): `nix develop .#sqlx --option substituters https://cache.nixos.org --option extra-substituters '' --option accept-flake-config false -c cargo fetch --locked --manifest-path packages/default/Cargo.toml`; `nix build .#default.evaluatorNix --no-link --print-out-paths --option substituters https://cache.nixos.org --option extra-substituters '' --option accept-flake-config false`. The SQLx devshell's ambient nix is 2.34.5, so the isolated regression explicitly selected the prepared product evaluator Nix 2.34.8.

Exact offline verification prefix: `nix develop .#sqlx --offline --option substituters '' --option substitute false --option builders '' --option accept-flake-config false -c`. Commands after that prefix, each exit 0: (1) `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true NIX_TEST_BIN=/nix/store/s9x74dmyx8wphr49pxxpnh2yrxwla1np-nix-2.34.8/bin/nix TMPDIR=/tmp/opencode cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-config config::cache::tests -- --include-ignored --nocapture` (4 passed; last run includes explicit destination-root CF signature assertion and consumer dependency still absent after rejection); (2) `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-builder derivations::cache::tests -- --nocapture` (14 passed); (3) `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-config --doc` (16 passed); (4) `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo check --locked --offline --manifest-path packages/default/Cargo.toml -p cf-server --lib`; (5) `rustfmt --check --edition 2024 packages/default/crates/cf-config/src/config/cache.rs packages/default/crates/cf-builder/src/derivations/build.rs packages/default/crates/cf-builder/src/derivations/cache.rs packages/default/crates/cf-server/src/derivations/build.rs packages/default/crates/cf-server/src/derivations/cache.rs`. `git diff --check` also exit 0. SSH transport/systemd scope runtime and owning Delivery execution remain coordinator evidence; worker did not start servers, VMs, or process stacks. Guarantee applies to enabled, unfiltered signed CacheType::Nix publication after source signing; it verifies destination closure metadata against the configured key (or Nix-verifiable CA content), with no guarantee for network/auth/GC retention or unscanned content integrity. Targets must read that same metadata and trust the matching public key.

Follow-up worker commit 77aef4551159dff71f3619508139b3bf9d903e18 on top of 07616c56, same dedicated worktree; clean status, no amend/push. P1: normal server Nix publication now calls sign_publication_root with resolved root and selected attempt CacheConfig before transfer. Its shared redacted/scoped executor is used for selected-key signing and destination verification. The new nonignored server unit tests exercise DB-only and different-static-key selection, recursive/resolved-root args, success and exit-23 propagation/redaction, plus no signing for unsigned configurations or other adapters. API builder source signing is unchanged. P2: signed Nix legacy ssh:// command construction returns configured-key recursive verify only; no copy-sigs/addSignatures. Missing configured trust fails with a static legacy-protocol limitation context in both publication executors. ssh-ng:// retains propagation then verify. No weaker trust settings or private-key transport.

Follow-up verification: common exact prefix `nix develop .#sqlx --offline --option substituters '' --option substitute false --option builders '' --option accept-flake-config false -c`. Suffix (exit 0; 4 passed): `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true NIX_TEST_BIN=/nix/store/s9x74dmyx8wphr49pxxpnh2yrxwla1np-nix-2.34.8/bin/nix TMPDIR=/tmp/opencode NIX_STORE_DIR=/dummy NIX_REMOTE=daemon NIX_DAEMON_SOCKET_PATH=/nonexistent-cf-daemon cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-config config::cache::tests -- --include-ignored --nocapture`. Evidence: legacy verify-only missing CF trust exit 2; complete CF trust despite source-only unrelated late signature exit 0. Warm-copy verification/acquisition remains exit 2/1 before propagation and acquisition exit 0 afterward. Real test name for owner packaging wiring remains `config::cache::tests::warm_nix_publication_propagates_closure_trust`; select with `-p cf-config --lib ... -- --ignored --exact --nocapture`, set NIX_TEST_BIN to pinned evaluatorNix/bin/nix and leave TMPDIR at sandbox-owned writable parent. Fixture explicitly uses `local?root=TEMP&store=/nix/store`, removes child NIX_REMOTE, isolates HOME/cache, and does not require a host daemon. Worker used existing /tmp/opencode parent. This is poisoned-environment local verification, not a claim of an executed Nix package sandbox; Sol owns default.nix wiring and sandbox package execution.

Other follow-up suffixes after the same offline prefix, all exit 0: `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-server --lib derivations::cache::publication_signing_tests -- --nocapture` (2 passed on final run; initial cold compilation hit tool timeout at 120000 ms and was retried with 300000 ms); `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-builder derivations::cache::tests -- --nocapture` (14 passed); `env CARGO_NET_OFFLINE=true SQLX_OFFLINE=true cargo test --locked --offline --manifest-path packages/default/Cargo.toml -p cf-config --doc` (16 passed); `rustfmt --check --edition 2024 packages/default/crates/cf-config/src/config/cache.rs packages/default/crates/cf-server/src/derivations/cache.rs packages/default/crates/cf-builder/src/derivations/cache.rs`. git diff --check exit 0. Only these three source files changed. No default.nix, shared harness, owner composition, CI, Nix module, database, VM or server-stack changes.
<!-- SECTION:NOTES:END -->
