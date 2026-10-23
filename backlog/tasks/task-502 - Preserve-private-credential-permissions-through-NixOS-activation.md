---
id: TASK-502
title: Preserve private credential permissions through NixOS activation
status: Backlog
assignee: []
created_date: '2026-10-23 05:20'
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
