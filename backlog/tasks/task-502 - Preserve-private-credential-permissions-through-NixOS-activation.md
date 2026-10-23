---
id: TASK-502
title: Preserve private credential permissions through NixOS activation
status: To Do
assignee:
  - '@openai'
created_date: '2026-10-23 05:20'
updated_date: '2026-10-23 13:03'
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
- [ ] #1 Existing private cache credentials retain owner-only file permissions and their contents across normal NixOS activation and server or builder restart.
- [ ] #2 A configured authenticated cache remains usable after activation without an operator manually repairing file permissions.
- [ ] #3 Focused regression evidence covers restrictive credential files within the service state directory and preserves generated agent or builder key handling.
- [ ] #4 Applicable credential-storage and upgrade guidance describes the verified permission contract.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Review clarification requested during TASK-490.23: this permission observation is NOT the cause of the old-server activation failure. Unmodified f9dd6b9c deploy_system stores a commit SHA in systems.desired_target; its heartbeat handler returns that value unchanged; its root agent rejects it before cache copy with 'This is not a store path'. The same old agent accepts a resolved /nix/store path from the current server and activates successfully. Do not request current-agent or deployment-planner changes to repair this old-pair behavior.

The original cache-key observation was an operator-created, crystal-forge-owned cache SSH identity under /var/lib/crystal-forge/cache-ssh. Both the predecessor and current NixOS module declare a recursive Z /var/lib/crystal-forge/ 0755 crystal-forge crystal-forge rule. The released agent has a separate root-owned /var/lib/crystal-forge-agent state directory (0750), /run/crystal-forge-agent runtime directory (0700), and a caller-supplied client.private_key path. Upgrade's agent key is root-owned at /var/lib/upgrade-identity/agent.key; cache credentials now start in /var/lib/upgrade-credentials and retain mode 0600 without post-activation chmod. New fixture diagnostics record actual effective UID/GID, configured path sources and stat metadata only, not key contents.

Before any production implementation, verify the supported credential-placement contract. The bounded current-production component implicated by the original 0755 observation is modules/nixos/crystal-forge/default.nix tmpfiles permission handling. Any repair authority should be limited to preserving configured private-file ownership/modes through activation plus regression/docs; no old-source backport, protocol change, builder DB exception, or unrelated queue/retry changes are authorized by this discovery.

Owner explicitly authorized the narrow current-module permission correction, runtime-generated inside-state-tree credential reproduction, real activation/cache use/restart regression, external/symlink control and bounded remediation/exposure guidance. No predecessor patch, new secret system, blanket recursive mode/ownership operation, root service, credential relocation as production fix, protocol/DB-policy change or unrelated permissions audit. The outside-directory Upgrade pass is independent evidence, not TASK502 proof.
<!-- SECTION:NOTES:END -->
