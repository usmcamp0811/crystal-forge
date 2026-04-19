---
id: TASK-276
title: Add systemd hardening scanner and dashboard view
status: Backlog
assignee: []
created_date: '2026-04-19 02:43'
labels:
  - feature
  - security
  - systemd
  - dashboard
  - nixos
dependencies: []
references:
  - >-
    https://www.reddit.com/r/homelab/comments/1spgay2/is_anyone_else_a_stickler_for_systemd_hardening/
  - 'https://www.freedesktop.org/software/systemd/man/systemd.exec.html#Security'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a feature to scan NixOS system configurations for systemd service hardening options and display them in a dashboard view.

## Context

Inspired by: https://www.reddit.com/r/homelab/comments/1spgay2/is_anyone_else_a_stickler_for_systemd_hardening/

systemd provides numerous security hardening options (PrivateTmp, ProtectHome, ProtectSystem, NoNewPrivileges, CapabilityBoundingSet, etc.) that can significantly improve system security. Many NixOS configurations may not fully utilize these options.

## Desired Behavior

The system should:
1. Scan NixOS configurations for systemd service definitions
2. Analyze each service for enabled/disabled hardening options
3. Present a dashboard view showing:
   - Which services are well-hardened vs. vulnerable
   - Specific hardening options enabled/disabled per service
   - Recommendations for improvement
   - Visual indicators (colors, scores) for hardening status

## Technical Considerations

- May need to parse NixOS configuration files or introspect running systemd services
- Dashboard should be filterable/sortable by service, hardening score, or specific options
- Consider integration with existing Crystal Forge config management features
- Reference systemd security directives documentation: https://www.freedesktop.org/software/systemd/man/systemd.exec.html#Security

## Out of Scope (for initial implementation)

- Automatic remediation/fixing of hardening issues
- Custom hardening profiles per service type
- Historical tracking of hardening improvements
<!-- SECTION:DESCRIPTION:END -->
