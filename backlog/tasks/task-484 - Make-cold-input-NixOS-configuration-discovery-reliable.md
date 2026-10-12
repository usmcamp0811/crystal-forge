---
id: TASK-484
title: Make cold-input NixOS configuration discovery reliable
status: In Progress
assignee:
  - '@openai-agent'
created_date: '2026-10-12 14:29'
updated_date: '2026-10-12 16:46'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/flake/commits.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/flake/verified_source.rs
priority: high
type: bug
ordinal: 506000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User requests a separate evaluation reliability fix based on TASK-470's committed worktree head; TASK-470 is expected to merge before this fix. CF ata-nix-config preflight at exact commit 3d6b517bd2b999a21814df3df4eb498202c4e2bd failed configuration discovery after 60s with cleanup confirmed=false before nix-eval-jobs. Manual local cold discovery succeeded in 145.41s with 12 systems and warm retry took 1.169s; CF succeeded after cache warming. A second Campground local cold discovery took 170.34s, fetched locked dotfiles c56cc6262ad191749226c34dc0c91f00959c340a (322.43 MiB) and CF a33c1d0cb534ee0a78c41e015a7c75e4c845df28 (78.64 MiB), and reported an unauthorized CUDA Cachix cache. Local user/checkout commands are not proof of identical CF runtime behavior. Investigate precise root cause and make valid cold-input flakes proceed without manual cache warming. Latest user explicitly redirects work to timeout now; leave pending Attic metrics work intact and do not finish its checks first. Do not merge or change cache/signing/deployment/derivation identity; preserve adaptive memory-aware recovery and fail-closed cleanup/source integrity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Authoritative configuration discovery uses the same exact verified immutable commit content and NAR-qualified source as policy evaluation without redundant root Git resolution or revision fallback; lock integrity and private input credentials remain enforced
- [ ] #2 Discovery has a configurable bounded outer process deadline with a cold-input-appropriate default and explicit upper limit independently of Nix realization and bulk evaluator timeouts
- [ ] #3 Discovery resource admission preserves PostgreSQL advisory-lock-before-semaphore order without nested deadlocks; unconfirmed child cleanup retains ownership and prevents replacement overlap
- [ ] #4 Timeout and cancellation provide bounded cleanup acknowledgment while preserving process-group isolation direct-child reap and fail-closed quarantine
- [ ] #5 Preflight logs distinguish evidenced source/input-resolution and discovery stages completion/system count timeout cleanup status and actual Nix errors without credential leakage; terminal failure does not remain indefinitely In Progress
- [ ] #6 Deterministic local cold/warm and private-input fixtures plus Rust regressions cover deadline source identity lock integrity concurrency cancellation cleanup quarantine discovery-to-policy success genuine errors and terminal status
- [ ] #7 Relevant evaluator/server integration and VM checks are run with exact results; documentation root-cause findings safety review commit pushed MR and exact-head CI evidence are recorded without merging
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect authoritative preflight and artifact discovery call chains, verified source identity, credential environment, configuration and process ownership. Establish facts separately from cold-fetch hypotheses.
2. Reuse the verified NAR-qualified source for authoritative name discovery. Add a bounded discovery-specific server timeout (default 300s with explicit maximum) and match evaluator source/lock/purity/IFD policy.
3. Admit cold discovery under existing advisory-lock-before-semaphore ownership scope without nested locks. Retain ownership through bounded cleanup/quarantine. Give timed-out bounded child cleanup a separate small acknowledgment opportunity without releasing unconfirmed ownership.
4. Add evidence-based bounded/redacted preflight diagnostics and verify terminal failure status. Start isolated task live preview before browser-visible workflow edits; report exact startup blocker if unavailable.
5. Add deterministic source/lock/auth cold-warm/error/deadline/concurrency/cancel/quarantine regressions, run affected Rust and VM checks, review deadlocks/leaks/source parity and update source/knowledge documentation.
6. Commit/push separate fix normally; open MR with TASK-470 merge dependency and exact-head CI outcomes. Do not merge.

Delegated fixture scope: add checks/nixos-discovery-reliability/{default.nix,README.md,test.py}. Use disposable sandbox-local Nix store/cache and Basic-auth loopback archive endpoint delayed 65 seconds. Verify exact NAR-qualified local root, locked private input, 300-second discovery success, warm no-refetch, mismatch rejection, unchanged lock, and production primary evaluation via nix-eval-jobs. Run focused Snowfall check through command-local public-cache wrapper; no parent Rust edits or host services.

Backend diagnostics follow-up in the existing locked worktree: observe only exact Nix stderr input-resolution markers with a fixed-size cross-chunk carry and one-shot watch<bool>; log source completion/failure, discovery start, evidenced resolving inputs and successful system count. Use a typed bounded-child timeout to retain deadline and cleanup state across anyhow context and preserve the operation error if lease confirmation fails. Remove repository URLs before truncating/redacting bounded terminal excerpts. Add focused regressions and run through the Nix shell. User waived persistent preview for backend log-only changes and excluded frontend/service/database/CI edits and commits/pushes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-agent on sledge in /home/mcamp/code/crystal-forge/TASK-484-discovery-timeout

Preflight: clean dedicated branch TASK-484-discovery-timeout at TASK-470 committed head a33c1d0cb534ee0a78c41e015a7c75e4c845df28. No main worktree registered. dev has preserved untracked TASK-478 lock and session-ses_f927.md; no integration writes or cleanup. Pending TASK-470 metrics files remain intact and excluded. User latest instruction prioritizes timeout now over earlier metrics-finalization sequence. Risk high: source identity and subprocess ownership. Merge dependency: TASK-470 must land in dev before this branch merges; implementation can proceed on its committed head. Verification: targeted config/discovery/process Rust tests; deterministic local cold/warm/private/immutable-source fixture and relevant verified-source/evaluator/server VM checks. No external GitLab download-speed test.

Delegated fixture implemented only checks/nixos-discovery-reliability/{default.nix,README.md,test.py}. Snowfall auto-discovery verified via successful focused build: bash packages/ci/public-cache-build.sh 'path:.#checks.x86_64-linux.nixos-discovery-reliability' --no-link -L --print-out-paths (tool timeout 500000ms; exit 0). Final source run: historical deadline timeout 60.063s; real cold discovery 65.040s under 300s; warm 0.021s without refetch; production primary_evaluation.nix via nix-eval-jobs 0.035s, alpha/beta derivations and policy markers succeeded. Separate sandbox-local historical store/cache; Basic-auth netrc endpoint rejected unauthenticated access; two authorized requests total (historical and cold), no warm/policy refetch. Root NAR mismatch exited 102 with actual NAR mismatch diagnostic; root hash and lock bytes unchanged. Output report and redaction-checked per-command stdout/stderr: /nix/store/5vq56mbbvjg9awbcxgq1kk2iw61ds6d2-crystal-forge-nixos-discovery-reliability. Build log: nix log /nix/store/1gd0p585x9l6afxslbqsjizv3a8c2ajd-crystal-forge-nixos-discovery-reliability.drv. Remaining parent scope: Rust coordinator/default-config binding, lock order, cancellation/quarantine and terminal status regressions. No Rust edits, host services, commits or pushes performed.

Fixture followup strengthened only owned test.py and README.md: actual nix store add-path --name source into three isolated local stores; verify ingested NAR/lock and read-only permissions; remove staging tree before historical/cold/warm/policy discovery. All stores retain same source basename. Real unauthenticated Nix cold discovery before netrc configuration failed HTTP 401 in 0.017s with zero authenticated/delayed responses. Authenticated flow uses explicit NIX_CONFIG netrc-file. Endpoint counters: authorized=2 completed=2 rejected=2 unexpected=0; no root route or root repository fetch. Added ingested genuine throwing nixosConfigurations error (exit1, 0.021s) and incomplete private-input lock parity: discovery exit1 0.042s, normal policy exit1 0.026s, both cannot update unlocked flake input private in pure mode, no fetch/lock mutation. Final official build command bash packages/ci/public-cache-build.sh 'path:.#checks.x86_64-linux.nixos-discovery-reliability' --no-link -L --print-out-paths, timeout500000ms, exit0. Historical timeout60.021s; cold65.078s; warm0.022s; same-source production policy0.046s. Report/raw per-command stdout/stderr: /nix/store/q4a283izgfsh6dpxny28amsd64qn7yif-crystal-forge-nixos-discovery-reliability. Full build log: nix log /nix/store/g2wyr2q6pjl6yz0xnvqi8kxf3n9awphz-crystal-forge-nixos-discovery-reliability.drv. No production/knowledge/CI edits, services, commits, pushes; parent-owned concurrent changes preserved. Remaining scope Rust coordinator/config/ownership regressions; stale-lock-specific case not added (incomplete-lock parity covered).

Backend diagnostics follow-up finished locally in /home/mcamp/code/crystal-forge/TASK-484-discovery-timeout. Changed only packages/default/crates/cf-server/src/models/evaluate_with_policies.rs and packages/default/crates/cf-server/src/flake/commits.rs in this follow-up; all pre-existing changes preserved. User explicitly waived persistent live preview for backend log-only changes. No frontend/service/database/CI edits or commits/pushes. Added exact stderr marker evidence (unpacking ' / fetching git repository) with longest-marker-sized carry (23 bytes) and one-shot task-local watch<bool>; scanner continues beyond stderr retention cap and sends no text/URLs. Logs materialization start/completion/failure, discovery start, evidenced resolving locked inputs once, and successful system count after cleanup/admission scope returns. Typed bounded-child timeout retains cleanup confirmed/pending diagnosis through anyhow context; failed lease confirmation preserves the original operation error and does not commit/release owned quarantine. Typed genuine discovery Nix failure records confirmed cleanup; selects the retained fatal error before excerpt truncation. Repository URLs are removed in addition to shared credential redaction; terminal excerpts capped at 350 chars and chain rendering capped at 16KiB. Corrected SECURITY comment to include Nix discovery private-input authentication while documenting bulk isolation.

Verification command (exit 0): nix develop --command bash -c 'export SQLX_OFFLINE=true; cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib preflight_ -- --test-threads=1 && cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib bounded_process_ -- --test-threads=1 && cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib discovery_scope_ -- --test-threads=1 && cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib flake::commits::tests::verified_discovery -- --test-threads=1'. Final results: 9 + 3 + 3 + 4 = 19 passed; zero failed/ignored in selected runs. Final underlying log: /tmp/opencode/task484-backend-diagnostics-tests-final.log. Coverage includes cross-chunk exact markers, cap-independent live-before-EOF one-shot signal, stdout exclusion, no elapsed-time evidence, typed real runner timeout, context/cleanup states, hostile stderr unable to impersonate runner timeout, bounded/redacted logs, actual discovery failure beyond warning prefix, bounded collection deadlines, inherited pipes, cancellation leases, cold/warm verified sources and incomplete/stale lock rejection. Earlier development runs exposed a missing helper and a hard-coded marker-length test assertion; both repaired before successful final run. Existing repository warnings remain.

Final formatting/check command (exit 0): nix develop --command rustfmt --edition 2024 --check packages/default/crates/cf-server/src/models/evaluate_with_policies.rs packages/default/crates/cf-server/src/flake/commits.rs && git diff --check && git status --short. Reviewed source contracts and adjacent security/concurrency/progress comments. Remaining gap: strengthened ignored discovery_quarantine_retains_real_advisory_and_capacity regression now injects a typed timeout and asserts its survival through real lease-confirmation failure; compiled by cargo lib test build but not run here. Parent-owned official server-regressions running snapshot predates this edit and must not be cited as verification of this follow-up. No database services or fresh authoritative VM checks started by this follow-up. Task remains In Progress.
<!-- SECTION:NOTES:END -->
