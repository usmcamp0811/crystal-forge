---
id: TASK-427
title: >-
  Fix Anduril NixOS STIG policy data: services.sshd.enable is not a real NixOS
  option
status: Backlog
assignee: []
created_date: '2026-08-17 04:05'
updated_date: '2026-08-17 04:05'
labels:
  - compliance
  - nixos
  - data
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/317'
  - policies.json
priority: medium
type: bug
ordinal: 422000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while testing TASK-425 (cf-nixos-module) against the real exported Anduril NixOS STIG policies (policies.json).

The "NixOS must protect the confidentiality and integrity of transmitted information." policy asserts `config.services.sshd.enable == true` (parsed as `services.sshd.enable`). There is no such NixOS option; the real one is `services.openssh.enable`.

Impact under the new architecture: the generated NixOS module now fails at NixOS evaluation with a clear "option does not exist" error (correct behavior, surfaced loudly instead of silently doing nothing). But the policy data itself is wrong and must be fixed in Crystal Forge (the policy definition / export pipeline), not in the generator.

Action: fix the policy assertion to a real NixOS option (`services.openssh.enable`) or to an option that actually exists for the intended hardening control, then re-export and re-test with cf-nixos-module.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The offending policy assertion no longer references services.sshd.enable
- [ ] #2 Re-export from Crystal Forge produces a policies.json whose NixOS assertions validate against real NixOS options
- [ ] #3 cf-nixos-module output for the fixed export evaluates successfully through NixOS eval-config
<!-- AC:END -->
