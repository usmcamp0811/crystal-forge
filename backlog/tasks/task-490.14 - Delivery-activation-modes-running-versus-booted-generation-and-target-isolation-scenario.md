---
id: TASK-490.14
title: >-
  Delivery: activation modes, running versus booted generation, and target
  isolation scenario
status: In Progress
assignee:
  - openai
created_date: '2026-10-17 16:37'
updated_date: '2026-10-20 05:24'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
modified_files:
  - checks/integration-delivery/scenarios/fragments/activation.py
  - checks/integration-delivery/scenarios/fragments/activation.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 526000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.modes. Goal: prove real activation policies, BootOnly semantics and target isolation; mocked UI or seeded deployment rows are insufficient. Risk: high (boot and identity assertions). Non-goals: rollback/drift (.15), product changes, shared helper/composition/registration/CI/docs edits.

Writable paths (only):
- checks/integration-delivery/scenarios/fragments/activation.py
- checks/integration-delivery/scenarios/fragments/activation.cjs

Composition target: checks/integration-delivery/scenarios/delivery-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs mapping and explicit owner registration/aggregation. No packet-specific VM derivation. Earlier real builds/publications/deployments in this same fresh Delivery VM/database MAY be consumed through receipts. Another derivation's live targets, rows, outputs or receipts MUST NOT be consumed.

manual changes /run/current-system only after Deploy; auto_latest follows a newly evaluated commit; pinned ignores newer commits. BootOnly sequence: before action record next-boot selection, /run/current-system and /run/booted-system independently. Before reboot selection changes, while running and booted identities each equal their own pre-action value. Observe next-boot selection from the configured bootloader selection/entry, separately from /run/booted-system; do not assume running and booted were initially equal. After reboot the target boots and runs the selected deployment. System detail Deploy/History must display actual running versus booted identity.

A second real agent on builderB uses its own key. Deploying targetA leaves that second target's generation/services unchanged; prove the target-specific output independently absent in builderB's store before the isolation operation. Expected identities come from authored facts/target observations and same-run receipts, not CF response data under test.

Dependencies/readiness: TASK-490.12 publication and its .10/.7 build/role prerequisites must be integrated at actual tested revisions; .7 supplies bootable targetA and builderB/second-agent wiring. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26; it does not prove activation or future roles. This packet remains unexecuted and awaits selection, these concrete code interfaces and accepted policy/boot UI authority.

Read-only source/authority: cf-server models/systems.rs DeploymentPolicy, cf-config config/deployment.rs DeploymentStrategy, handlers/agent/*, api/systems.rs deploy/status/generations, cf-agent deployment; docs/knowledge/deployment/*; approved SystemDetail.jsx/DeployGate.jsx. As-built D1 is evidence, not acceptance authority; uncertain cases stay blocked pending owner decision.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Retain correct failures and linked product defects; escalate helper/authority gaps without weakening assertions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 manual, auto_latest and pinned follow accepted requirements on real targetA, including manual Deploy and pinned ignoring newer evaluated commits
- [ ] #2 BootOnly records next-boot selection, /run/current-system and /run/booted-system independently before action; before reboot selection changes while each running/booted identity equals its own recorded pre-action value; after reboot the target boots and runs the selected deployment; next-boot selection is observed separately through configured bootloader selection/entry, never inferred from /run/booted-system or assumed pre-action identity equality
- [ ] #3 Deploying targetA leaves the second real agent on builderB and its generation/services unchanged, with target-specific output absence independently proved
- [ ] #4 System detail Deploy/History running/booted identities match independent observations of the real target
- [ ] #5 Coordinator composes only the named activation fragments into delivery-lifecycle after real same-scenario build/publication setup via receipts; no packet-specific derivation or cross-derivation live-state reuse occurs
- [ ] #6 Coordinator executes the focused delivery-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-selected bounded contribution to the existing coordinator worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation, branch same, existing draft MR335. Base for this increment 3db47c89; coordinator lock/ownership retained. Shared task worktree explicitly required by user. Keep .12/.10 and broader .14 acceptance open; only manual ImmediatePersist path now.
1. Share safe pinned QEMU/test-instrumentation target baseline/network/isolation between initial VM and Git candidates. Enable switching; preserve root/store overlays, backdoor, actual production agent and runtime key. Prepare immutable baseline only, never runtime nonce outputs.
2. Give target its own read SSH identity and pinned host trust. Use test-owned publication source/public keys; disable alternate substituters/builders. Preserve CF publication and independently inspect publisher signature before deliberate receiver-test signing.
3. Record initial actual running/profile/generation/boot/system/state identities and two unchanged heartbeats. Deploy first real CF-built revision through production UI, require cold target before acquisition and actual marker/running/profile convergence plus succeeded pending row/history/retained lineage.
4. Build distinct revision through same Git/evaluation/real builder/publication chain. Deploy via same production UI; require genuine full state S2/new running output, then two heartbeat rows referencing S2 without duplicate unchanged states/errors. Capture reload/system/history screenshots and receipts. No direct state writes/fabricated reports/agent stop.
5. Supply observed retained first generation to .15 rollback in same scenario. Run normal CI owner script and Access if shared inputs change; inspect exact committed hosted pipeline/artifacts. No UI production changes or unrelated repair. Record all omitted modes explicitly.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: foundation bba718be passed; .12 publication and .7/.10 role/build interfaces plus accepted activation authority remain packet prerequisites. No activation VM result is claimed.

### Historical
2026-10-17 review recorded unresolved foundation/package prerequisites and no VM execution at that time. Former BootOnly wording conflated boot selection with booted identity; the current AC #2 records each identity independently. The former per-packet activation derivation is no longer the composition model.

Owner explicitly selected the bounded manual live-switch contribution in the connected Delivery increment, despite broader .12/.10 variants remaining open. Reuse existing actual SSH publication/build interfaces and the same five roles. This does not satisfy Niks3, other strategies, auto_latest/pinned, BootOnly/reboot or second-agent isolation. Coordinator owns target fixture/composition; no production/UI edits are authorized.

Connected manual contribution executed via sh packages/ci/integration-owner-build.sh integration-delivery on x86_64-linux, input3db47c89-dirty. Latest VM evidence /nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run; driver303.80s, gateexit1 with13passed/1failed. Both real authored revisions were evaluated/built/published/acquired cold and actually activated through production UI. Target/profile/marker, one new full state per transition, two later heartbeats and exact succeeded request/history agree. Browser generation/history reload passed for both. All previous10cases retained. Rollback fails mandatory TASK499 authority prerequisite; no rollback or full .14 completion claim. Auto_latest/pinned, BootOnly/reboot, secondagentisolation and complete designparity remain open. FixtureGRUB correction is direct-boot test plumbing, not productbootloadercoverage.

Original ten-case increment committed/pushed3db47c8953e3cf1f29473be7595a660d69e6a988; hostedpipeline2965867360 all8success with exact artifacts. Laterconnected code not covered bythatgreenpipeline. Access normal CI script executed fresh23/23 exit0 driver53.48s evidence /nix/store/ci1m5937pfh4b1p3hzpmgxfa88vlk47z-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run. Fullflakecheck on sameinputexit1 reusesidenticalA/DVMrunsandexecutesfailingDeliverygate. Docsvalidationexit0 58tests/215Mermaid/192concepts. DB517/0/0atclean3dbhosted; unchangedRustinputsnotrerunforfixture-onlyedits. No new production repair beyond separate d11de059 TASK496. TASK497/498/499/500 findings recorded.
<!-- SECTION:NOTES:END -->
