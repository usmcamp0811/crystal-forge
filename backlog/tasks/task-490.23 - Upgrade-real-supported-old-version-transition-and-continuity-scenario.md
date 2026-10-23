---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-23 03:58'
labels:
  - testing
  - integration
dependencies:
  - TASK-490.7
modified_files:
  - flake.nix
  - flake.lock
  - .gitlab-ci.yml
  - checks/integration-upgrade/default.nix
  - checks/integration-upgrade/README.md
  - checks/integration-upgrade/fixtures/released-module.nix
  - checks/integration-upgrade/fixtures/target.nix
  - checks/integration-upgrade/fixtures/signed-request.cjs
  - checks/integration-upgrade/scenarios/version-transition.nix
  - checks/integration-upgrade/scenarios/version-transition.py
  - checks/integration-upgrade/scenarios/version-transition.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 535000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-upgrade. Contract: upgrade.continuity. D2 is approved by the owner, not a remaining policy blocker. Predecessor is real Crystal Forge v0.3.1 at f9dd6b9c79530d3f911203645207bf6251552aba; local tag relationship verified. Pin actual source and required inputs, use genuine packages and normal supported configuration. Do not patch predecessor behavior or substitute current code/version strings, synthetic old schema, historical-looking seeded outcomes or another owner's live state.

Approved controlled maintenance sequence: establish the predecessor and representative persisted state through real workflows; stop predecessor server-side writers/builders at the cutover boundary; upgrade builder packages/config but keep current builders stopped; start current server normally against the SAME installation/database, letting normal migrations run; verify readiness then start current API-only builders; exercise old-agent/current-server signed heartbeat/state and one actual ordinary Nix deployment; deliberately upgrade agents preserving registered system/signing identity/history. No concurrent old/new writers, old-builder/current-server window, post-cutover builder DB-credential exception, invented timed expiry or new version-refusal policy.

Old-agent compatibility is limited to this exact approved pin and selected operations during the operator's agent rollout until its deliberate completion. Record actual executable/version/protocol identities; deployment must not replace the old agent before compatibility proof. Request acceptance is not target activation or reporting proof.

Preserve completed history/identity relationships and representative credentials. Establish queued/interrupted work through actual predecessor activity with controlled pause/stop/transport interruption. Recover through current execution, retaining logical identity/audit and legitimate successor-attempt relationships. Stale predecessor claims must not commit after replacement ownership; recovered operations must reach intended outcome without duplicate authoritative success or contradictory terminal history. Original evidence remains attributable. Incomplete publication and failed/missing evidence must not become deployment/completion authority. Physical retries need not execute exactly once. No new queue/recovery/compatibility framework.

Use one meaningful native Snowfall integration-upgrade owner with version-transition.{nix,py,cjs}, existing harness/roles/evidence and explicit registration. Preserve installed state across transition. No placeholder or per-criterion VM/check. Coordinator owns package/wiring/composition/CI; semantic cases and necessary immutable fixtures belong under this owner. D2 authorizes test/fixture/wiring implementation, not blanket production fixes. Any discovered compatibility/recovery defect keeps its correct failing assertion and requires applicable separate authority.

Run the normal Upgrade aggregate and inspect actual old/current identities, normal migration, recovery, old-agent observation, deliberate agent upgrade and new current operation. Record exact inputs/commands/platform/exits/names/counts/fresh-or-reused/accessibility. Then integrated four-owner/lower-level/hosted verification is required. MR335 remains draft until all concrete merge blockers are resolved; no merge/deployment outside disposable tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Approved D2 predecessor is v0.3.1 at f9dd6b9c79530d3f911203645207bf6251552aba with real packages, pinned required inputs and the recorded controlled maintenance sequence.
- [ ] #2 Real predecessor-created data survives normal in-place current-server migration with representative credentials, registered system/signing identities, completed records and history relationships intact.
- [ ] #3 Real queued/interrupted predecessor activity recovers through current ownership/retry without stale authoritative completion or duplicate authoritative success; incomplete publication remains unauthorized and one new current operation completes.
- [ ] #4 Builders are upgraded before current-server cutover and held stopped until readiness with no old-builder/current-server operating window or DB-credential exception; the exact predecessor agent supports signed heartbeat/state and one actual ordinary Nix deployment during the operator's rollout, followed by deliberate agent upgrade preserving identity/history, without timed expiry or new version-refusal policy.
- [ ] #5 Coordinator composes one real version-transition scenario under native integration-upgrade using existing harness/evidence, retains the same installed state, executes the normal owner gate and records exact versions, inputs, commands, outcomes and accessible evidence.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Verify the approved exact predecessor source/tag and normal package/module/configuration interfaces; pin required inputs without modifying predecessor behavior or silently updating current inputs.
2. Build one real version-transition scenario under the native Upgrade owner and existing harness/evidence. Establish representative persisted state plus queued/interrupted activity through actual predecessor workflows; preserve the same PostgreSQL installation and target identities.
3. Implement controlled stop/fence boundary, upgraded-but-stopped builders, normal current-server migrations/readiness, then current API-only builders. Verify history/credentials/identity relationships and authoritative recovery, including stale claim/incomplete-publication rejection.
4. Observe the genuine old agent signed heartbeat/state and actual selected Nix deployment before deliberate agent replacement; upgrade its package preserving signing/system identity and verify a new current operation.
5. Execute focused/normal Upgrade gates, inspect actual versions/commands/rows/target outcomes and failures, then integrate meaningful owner/CI wiring into MR335. No placeholder, new framework, fifth check, invented window/refusal, or unapproved production fix. Coordinator performs integrated four-owner/lower-level/hosted verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Approved D2 decision

Owner explicitly approved v0.3.1@f9dd6b9c79530d3f911203645207bf6251552aba and clarified the complete contract now in this description and amended AC4. `git rev-parse v0.3.1^{commit}` returned that exact commit in the coordinator tree. There is no24-hour assumption and no new version-refusal behavior. A tag alone is not support policy; this owner instruction is the approval. Compatibility/recovery have not been verified yet.

## Historical proposals

Earlier records lacked an accepted predecessor/window and proposed24hours; those proposals are superseded. v0.3.0@f4a53b850d84dc3f6b36444ec6e0f474d344735c is not selected. Historical old builder DB usage is permitted only inside the genuine predecessor phase, not after current-server cutover. Existing current-version migration rehearsals or restart tests are not genuine Upgrade proof.

Preflight: dedicated clean worktree/branch TASK-490.23-version-transition created from integration c21a0d512c8c9006c8373c62b48532459c8f6cc4; owner task lock recorded. Current application production source matches the accepted wrap-up baseline, with test-only STIG preparation already present. Main coordinator retains its STIG composition change and other ownership; no resetting/rebasing/overwriting occurred. No main integration worktree is registered. Use only invocation-owned disposable Upgrade VMs/database; existing task preview and persistent developer instances are forbidden targets.

Resumed approved implementation in clean TASK-490.23-version-transition at c21a0d51. Verified canonical lock and local v0.3.1^{commit}=f9dd6b9c79530d3f911203645207bf6251552aba. dev has existing untracked task locks/session notes; no main worktree registered; left those untouched. Added exact HTTPS GitLab predecessor input with its release lock graph; no current input update requested. Genuine predecessor default package evaluates as crystal-forge-0.3.0 (the release source Cargo version, distinct from tag v0.3.1). Online preparation running tmux task49023-prepare, log/status /tmp/opencode/task49023-prepare.{log,status}, local --builders '' --max-jobs 1 --cores 2. No host application stack started. Impact: add upgrade.continuity/version-transition using existing harness, role topology and strict evidence gate; approved authority D2/AC4. Current source/library checks remain coordinator-owned.

Implemented and committed 94bf8a0dbba2d6a685e02f964f73f47d7fb56336 (TASK-490.23: Add exact-release Upgrade transition gate). One native Snowfall owner/scenario, 13 authored mandatory cases, unchanged shared harness and five existing roles. Root input pins exact GitLab predecessor and its release graph; automated comparison confirms every pre-existing lock node and root input mapping is unchanged. CI uses existing integration-owner-build.sh aggregate/evidence mechanism. Owner-local guidance records maintenance, preparation, exact release/Cargo-version distinction, runtime boundaries and failure semantics. No production source changes.

Genuine unmodified predecessor default package preparation PASSED exit 0. Command: nix build --impure --expr '(builtins.getFlake (toString ./.)).inputs.upgrade-predecessor.packages.x86_64-linux.default' --no-link --print-out-paths --builders '' --max-jobs 1 --cores 2 --option substituters https://cache.nixos.org --option extra-substituters '' --option accept-flake-config false -L. Output /nix/store/jrha7c4i8fbh7wpvhffmwpbznmcrwmb9-crystal-forge-0.3.0. Release library: 454 passed/70 ignored; builder/agent selected binaries: 2+2 passed; integration selections: 4+6 passed/3 ignored; release UI: 64 passed/1 ignored. No skipped build checks or source patches. Log/status /tmp/opencode/task49023-prepare.{log,status}. All 127 released migration files are byte-identical to current files at the relocated cf-server migration path.

Static verification PASSED: normal Upgrade aggregate derivation evaluation; exactly [integration-access,integration-assurance,integration-delivery,integration-upgrade]; Python AST parse; node --check for scenario and signed-request fixture; git diff --check. Runtime is NOT yet verified. First normal owner invocation is in persistent tmux task49023-owner (pane PID 21210; /tmp/opencode/task49023-owner.{log,status}); it started from an earlier dirty fixture snapshot while online closure preparation continued. Exact committed-head normal invocation is serialized behind it in task49023-owner-current (/tmp/opencode/task49023-owner-current.{source,log,status}); it archives first invocation evidence before rerunning the normal aggregate. Both use only this worktree's disposable VM state, no host application ports, local one-job/two-core builds; wrappers wait for coordinator Assurance/other QEMU runs to finish. Current worktree clean. Do not treat authored count or a running preparation as executed/pass count. Keep In Progress and no AC checked until actual results inspected.

Verification continuation: earlier dirty c21a0d51 normal owner exited 1 with 0 passed / 1 failed / 12 not reached. Case exact-predecessor-services-and-source-ready failed (1038.5 s); actual predecessor server rejected default socket-derived SQLx connection URL: both host and hostaddr are missing. Original evidence remains /nix/store/gpn8lma0vfcnb635vxnqzrcnppfn1kgi-vm-test-run-cf-it-upgrade-version-transition-run/result.json and journals; log /tmp/opencode/task49023-owner.log. Earlier collector reevaluated newer files after the run and therefore did not copy the old output; original immutable store evidence was read directly.

94bf8a0d committed invocation had NOT started: tmux has-session -t task49023-owner prefix-matched task49023-owner-current itself. Verified waiter PID10790's command, workdir and sole sleep child; stopped only that task-owned waiting session. Removed blanket host QEMU wait and used exact tmux target matching. Host measured 49 GiB available, no running actual coordinator test QEMU, coordinator sessions sleeping. No foreign VM/process stopped. Test-only setup fix committed separately as 657c0e49: explicit supported loopback PostgreSQL host, wait for normal PostgreSQL provisioning target, bounded 180 s readiness waits. No production/predecessor changes and no mandatory assertions removed. Normal owner now actually running in task49023-owner-current, shell PID27823 / Nix PID27839 from this dedicated worktree; /tmp/opencode/task49023-owner-current.{source,log,status}. This invocation uses 657c0e49, 13 mandatory cases; runtime result pending.

Runtime follow-ups after 94bf8a0d are test-only commits 657c0e49 (release TCP DB/startup), d00bf4e2 (explicit release AUTH_MODE=local), cc38aabd (ordinary release deployment defaults; valid offline lock and recipe-metadata fixture; content-stable immutable preparation), a6d990de (release-only IPv6 DB HBA plus both-family cutover denial), and 6a3c5fee (retain failed build logs; continue safe independent cases without converting failures or inventing completed history). All thirteen mandatory case names/assertions remain. Generated source offline validation passed: nix eval --offline --no-update-lock-file --raw path:/nix/store/83nabla1m4jchshws71d0dygca3nmqda-upgrade-version-transition-source#nixosConfigurations.targetA.config.system.build.toplevel.drvPath.

Actual normal gate outcomes: 657c0e49 exited 1, 0 passed/1 failed/12 not reached (release local routes absent without AUTH_MODE); immutable evidence /nix/store/i0xwq47dhinqpq98g0csdwjgl2a9k1pf-vm-test-run-cf-it-upgrade-version-transition-run. d00bf4e2 exited 1, 1 passed/1 failed/11 not reached (exact old agent rejects nondefault deployment_poll_interval string 2s; ordinary defaults now used without patching old code); evidence /nix/store/ibx6c0g5vq9f7bp16l2k38mvddkcg191-vm-test-run-cf-it-upgrade-version-transition-run. cc38aabd showed 2 passed then case3 interrupted after verified old-builder IPv6 HBA setup error; only this worktree's verified Nix PID14251 was signaled; original build exit 1 preserved. No foreign VM/process stopped.

a6d990dec81c5e690fcc3bbfd25fa0fbc9171ea2 normal gate completed exit 1: 2 passed/1 failed/10 not reached, 629.6 seconds of named cases. Case1 real release API registration/login/environment passed 14.7s; case2 real registered builder/cache/agent credentials and old signed state passed 2.3s. Exact old server/builder executable /nix/store/5mggz18hj76lckhrpw8jxdc6f1mj4b9k-server-0.3.0/bin/{server,builder}; old agent /nix/store/yb7dzrrw17sxy4ir14j504hrv4n09sm1-agent-0.3.0/bin/agent. Real target SystemID b1fcab0e-beb2-4ea5-a35b-679bd705a5a3; authored source e83e623ea89cae08286f50c4c1f5750b3db0508f; real admitted job b0108346-bf73-4ec1-afd6-77c63fac569f; evaluated drv /nix/store/wsywxg43w9vsqlxhfijhsb66pz5bh27z-nixos-system-targetA-test.drv. Genuine release builder claimed jobs and launched nix-store --realise --no-substitute --offline, repeatedly exited 1; failure-report API returned 400 then used predecessor DB fallback. No authoritative success within 600s. Correct case3 assertion failed at 612.6s. Exact native stderr requires persisted job-log diagnostics added in 6a3c5fee; do not label the root cause established yet.

Durable local evidence archive for a6d990de: /tmp/opencode/task49023-a6d990de-evidence.tar.gz, containing original log/source/status plus immutable result/meta/journals from /nix/store/s39fhhw6n4xn3dpignkx7gpdkjv4c4gv-vm-test-run-cf-it-upgrade-version-transition-run. Prior archives use task49023-657c0e49-, task49023-d00bf4e2-, and task49023-cc38aabd-evidence.tar.gz. Source 6a3c5fee now runs one normal aggregate in exact tmux session task49023-owner-current (shell PID11246 when launched), log/source/status /tmp/opencode/task49023-owner-current.{log,source,status}. Prefix self-wait and blanket host QEMU wait are removed. Measured 40GiB available before launch; only invocation-owned fresh VMs/database, with same installed state retained within the transition. No cutover/current compatibility result claimed yet.
<!-- SECTION:NOTES:END -->
