---
id: TASK-502
title: Preserve private credential permissions through NixOS activation
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-23 05:20'
updated_date: '2026-10-23 14:57'
labels:
  - security
  - nixos
  - upgrade
dependencies: []
references:
  - TASK-490.23
  - modules/nixos/crystal-forge/default.nix
  - /tmp/opencode/task49023-e61a535a-evidence.tar.gz
  - >-
    /nix/store/0aia91nw74j1fl6cn7k0axb3a9ys06r9-vm-test-run-cf-it-upgrade-version-transition-run/journals
  - /tmp/opencode/task502-evidence.tar.gz
  - /tmp/opencode/task502-source-proof.json
  - /tmp/opencode/task502-compact.json
modified_files:
  - modules/nixos/crystal-forge/default.nix
  - modules/nixos/crystal-forge/tests/packaging.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/scenarios/fragments/credential-permissions.py
  - checks/integration-upgrade/README.md
  - docs/knowledge/operations/nixos-module-configuration.md
  - >-
    docs/knowledge/security/builder-credential-boundary-key-management-and-audit-logging.md
priority: high
type: bug
ordinal: 553000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Upgrade verification exposed an activation-time credential regression: an operator-created cache SSH private key at /var/lib/crystal-forge/cache-ssh changed from mode 0600 to 0755. OpenSSH then refused the key and current-server derivation publication failed. The Upgrade fixture now keeps operator-managed keys outside that directory; the production permission behavior remains unchanged and needs separate repair authority. Original evidence is retained in the TASK-490.23 e61a535a result and controller journal. This task covers credential permission preservation and the related operational guidance; it does not authorize changes to builder database boundaries or predecessor source.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Existing private cache credentials retain owner-only file permissions and their contents across normal NixOS activation and server or builder restart.
- [x] #2 A configured authenticated cache remains usable after activation without an operator manually repairing file permissions.
- [x] #3 Focused regression evidence covers restrictive credential files within the service state directory and preserves generated agent or builder key handling.
- [x] #4 Applicable credential-storage and upgrade guidance describes the verified permission contract.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce unmodified current generated tmpfiles/activation behavior with a runtime-generated service-owned private cache key inside /var/lib/crystal-forge, recording safe owner/group/mode/content digest and actual isolated SSH/cache failure. Inspect only directly related startup/activation permission mechanisms.
2. Correct directory management nonrecursively; preserve legitimate private-file owner/group/mode/content and supported external/secret-managed/symlink paths. Keep fresh state/cache/work/log writes and CF-owned generated-key lifecycle intact; no blanket chmod/chown or relocation/root-service workaround.
3. Extend existing Delivery lifecycle and focused module verification, not a new scenario/check. Prove real service-identity key use before/after actual activation, unrelated-user denial, actual authenticated cache read with fallback disabled, repeated activation/restart, fresh directories and external control, without chmod repair inside the test.
4. Rerun affected normal Delivery and independent genuine Upgrade owner, inspect exact source/results and safe evidence. Keep failing baseline and no old-source modifications.
5. Document prevention versus remediation, bounded checks/restoration through existing secret manager, exposure assessment and possible rotation/revocation without asserting compromise or automatically changing identities. Commit narrowly; integrate/hosted verification remains coordinator-owned.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Review clarification requested during TASK-490.23: this permission observation is NOT the cause of the old-server activation failure. Unmodified f9dd6b9c deploy_system stores a commit SHA in systems.desired_target; its heartbeat handler returns that value unchanged; its root agent rejects it before cache copy with 'This is not a store path'. The same old agent accepts a resolved /nix/store path from the current server and activates successfully. Do not request current-agent or deployment-planner changes to repair this old-pair behavior.

The original cache-key observation was an operator-created, crystal-forge-owned cache SSH identity under /var/lib/crystal-forge/cache-ssh. Both the predecessor and current NixOS module declare a recursive Z /var/lib/crystal-forge/ 0755 crystal-forge crystal-forge rule. The released agent has a separate root-owned /var/lib/crystal-forge-agent state directory (0750), /run/crystal-forge-agent runtime directory (0700), and a caller-supplied client.private_key path. Upgrade's agent key is root-owned at /var/lib/upgrade-identity/agent.key; cache credentials now start in /var/lib/upgrade-credentials and retain mode 0600 without post-activation chmod. New fixture diagnostics record actual effective UID/GID, configured path sources and stat metadata only, not key contents.

Before any production implementation, verify the supported credential-placement contract. The bounded current-production component implicated by the original 0755 observation is modules/nixos/crystal-forge/default.nix tmpfiles permission handling. Any repair authority should be limited to preserving configured private-file ownership/modes through activation plus regression/docs; no old-source backport, protocol change, builder DB exception, or unrelated queue/retry changes are authorized by this discovery.

Owner explicitly authorized the narrow current-module permission correction, runtime-generated inside-state-tree credential reproduction, real activation/cache use/restart regression, external/symlink control and bounded remediation/exposure guidance. No predecessor patch, new secret system, blanket recursive mode/ownership operation, root service, credential relocation as production fix, protocol/DB-policy change or unrelated permissions audit. The outside-directory Upgrade pass is independent evidence, not TASK502 proof.

Preflight: clean dedicated TASK-502-private-file-permissions branch/worktree from reviewed real Upgrade879a7210226ba3cc1851c35f435430fef162865b, preserving its genuine predecessor source and passing 13-case tree. Task lock recorded. Main coordinator worktree retains only its own STIG/analysis changes. Existing task preview/default development databases are forbidden test targets; permission reproduction uses disposable owner VMs and runtime-generated nonproduction keys only.

Resumed clean owner-authorized worktree /home/mcamp/code/crystal-forge/TASK-502-private-file-permissions at 879a7210. Canonical plan/lock retained. Integration impact: extend existing delivery-lifecycle publication/restart coverage with runtime inside-tree private credentials, real activation and service-identity SSH/cache probe; retain all prior assertions. Genuine Upgrade remains independent and predecessor inputs unchanged. Current module also recursively chmod/chowns .cache and .config in both server/builder preStart; these directly related mechanisms must become directory-only to prevent restart from undoing preservation. No browser-visible changes. Normal owner gates will run serially with --max-jobs 1 --cores 2. Dev has only existing untracked owner locks/session notes; no integration changes will be touched. No known cf-it VMs/test-driver invocations were running at initial capacity inspection.

Baseline probe preparation: first two normal Delivery invocations failed before activation because the probe inherited root's PATH and Nix rejected inaccessible /root/.nix-profile/bin/ssh. Those failures are retained, not permission-regression evidence. Explicit guest PATH/HOME corrected the probe. Third normal invocation successfully queried the real published output as UID/GID crystal-forge, then reproduced a directly related restart failure: recursive preStart chown attempts to change a root-owned secret-manager symlink under .config and fails EPERM. /run/current-system/activate alone does not execute tmpfiles in this pinned NixOS. Inspected nixpkgs system/boot/systemd/tmpfiles.nix: systemd-tmpfiles-resetup is the generated switch-to-configuration reactivation unit (ExecStart systemd-tmpfiles --create --remove --exclude-prefix=/dev). Regression now uses real switch-to-configuration test plus explicit reapplication of that generated unit for same-generation repeats. Production module remains unmodified while obtaining the required Z/0755/OpenSSH-failure baseline.

Required baseline obtained with unmodified current production module: normal Delivery gate exit 1, 8 passed / 1 failed (later cases not reached). Rooted result /tmp/opencode/task502-baseline5-root -> /nix/store/dik5zp183qpmfbhq0sk6lhc7mr88ixgv-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Actual switch-to-configuration test exit 0, generated systemd-tmpfiles-resetup exit 0. Controller cache-ssh UID:GID 999:999 mode600 -> mode755; SHA256 unchanged b0afde46ab8ca2d08ce9f2b64a99f11eaa7243452b9a803208ebfaa72c3b1ce6. Before authenticated remote cache path-info exit0; after exit1 with OpenSSH UNPROTECTED PRIVATE KEY FILE / bad permissions, no fallback. Nobody read-access probe after activation exit0. Generated tmpfiles receipt includes actual Z rule. Safe receipts contain metadata/digests and diagnostics only. Baseline source and patch: /tmp/opencode/task502-baseline5.{source,patch}; full log/status retained. Earlier probe/startup failures separately rooted as baseline1-4; they are not the required activation proof.

Implemented current-module-only correction: remove recursive Z rule; remove recursive .cache/.config chown/chmod from both preStart scripts. Existing named d rules and explicit generated-config/default-key lifecycles remain. Added structural packaging checks, credential/upgrade remediation and exposure guidance. Fixed permission fragment runs after existing normal lifecycle consumers because restarting builders creates new sessions; original execution-session assertions remain unchanged. No predecessor source/input or production Rust/API/policy modifications. Verification in progress.

Normal fixed Delivery gate passed exit0: 16 passed / 0 failed, all original cases retained; new permission case took 13.2s, total recorded case time 488.9s. Fresh VM/database/runtime keys and authored source nonce 8308f2c0-b9fc-4477-9381-a9b9d8da3767; output /nix/store/di50a223cyfi0xzyzqdbcrfq8adz4awa-nixos-system-targetA-test. Root /tmp/opencode/task502-fixed-delivery-root -> /nix/store/d067bahmrpdgpgg03108x9h20g9v17gz-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Controller/builderA/builderB each UID:GID999:999; two real activation/tmpfiles and service restart cycles each, every cache probe exit0, nobody probes denied, all file/lstat owner/group/mode/SHA256 snapshots unchanged. Includes configured builderB external credential through root-owned symlink, root-owned .config secret-manager control, restrictive .cache/.config files, default generated Git keys and existing registered default builder API keys. Fresh work/tmp/build/cache/config writes and journald writes passed. Controller fixed key digest a878518f665ff4aa35af903d7f6a1da7fc04cb6478a17bf3405c17417050732f retained mode600. Full raw metadata and digest comparison: /tmp/opencode/task502-proof.json; compact summary /tmp/opencode/task502-compact.json.

Focused packaging module evaluation returned true exit0; generated module-only runtime checks exit0. Knowledge validation is a retained hard failure: current corrected tree and explicit clean879a7210 both exit1 solely because checks/integration-upgrade/README.md has no migration manifest entry. Both run58 passing validator unit tests and parse215 Mermaid blocks with0errors. Reported on existing TASK-478 (matching manifest-completeness scope), no validator/manifest edits. Logs/status /tmp/opencode/task502-{module-eval,module-runtime,knowledge,base-knowledge}.*. Independent normal Upgrade gate now running serially after Delivery, using the same corrected tree and genuine unchanged predecessor. No known competing cf-it invocations were running at handoff between owners.

Independent normal Upgrade gate passed exit0: 13 passed / 0 failed, all mandatory cases executed; total recorded case time610.3s. Fresh independent VM/database/runtime invocation, not reuse of the earlier owner result. Root /tmp/opencode/task502-upgrade-root -> /nix/store/6s00bp1z2fkg5cbkv9d089j5nx7cl941-vm-test-run-cf-it-upgrade-version-transition-run. Result records genuine predecessor tagv0.3.1 revisionf9dd6b9c79530d3f911203645207bf6251552aba and its exact package executables (manifest0.3.0), preserved identity/history/credentials, actual old-agent/current-server activation, deliberate agent rollout and fail-closed publication. Immutable input/package closures were reused; runtime evidence for required baseline5, fixed Delivery and Upgrade was freshly executed serially. All owner build commands used normal packages/ci/integration-owner-build.sh with max-jobs1 cores2.

Committed narrow repair as e533859bf222a994d3da431249f452568fab2c5a (`TASK-502: Preserve private credentials through activation`). Dedicated worktree clean after commit. No push/MR/merge/deploy. Post-commit source proof confirms every committed changed file equals the actual fixed Delivery/Upgrade input snapshots; tested source metadata is879a7210-dirty with exact patchSHA256 0d2d916538e073ab814d080c04ad91dc74ead09c410aa652ebeb3d33a7c7e1c4. Baseline production module unchanged; current module blob f2fe2fd34bb599c1d4606edd3b7f34f5dfd1f78e -> 7ebe28020a163a308e524ef83df87b371b2523ee. Flake inputs/lock; production packages/default; Upgrade scenarios and fixtures; shared integration harness all unchanged. Source-documentation diff reviewed: permission invariant and explicit generated-file exceptions documented; operator guidance separates prevention/remediation/exposure assessment, no actual compromise claim or automatic identity/key replacement.

Evidence archive /tmp/opencode/task502-evidence.tar.gz (3266018bytes), SHA256 bb0616c4441621ce6719d95ba91771764f738b3fc0b506dc8acb59271731fcc4. Contains source snapshots/patches, original exit statuses/logs, rooted results/journals/browser evidence, compact metadata/digest proof and earlier failed preparation attempts (separate from required baseline5). No runtime private key files copied; archive rejects private PEM material. /tmp/opencode/task502-archive.json maps all original rooted outputs. Known cf-it test invocations/VMs were absent after completion. Acceptance criteria locally proved; task remains In Progress under no-MR instruction. Remaining verification: retained pre-existing okf-knowledge manifest failure (confirmed cleanbase; TASK-478 comment), coordinator-owned full-flake/all-owner and exact-commit hosted CI. No full-flake/hosted pass is claimed.
<!-- SECTION:NOTES:END -->
