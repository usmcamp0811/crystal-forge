---
id: TASK-276
title: Add systemd hardening scanner and dashboard view
status: Backlog
assignee: []
created_date: '2026-04-19 02:43'
updated_date: '2026-04-19 03:17'
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

This feature extends Crystal Forge's existing security analysis capabilities (CVE scanning) by adding systemd hardening posture visibility across the fleet.

## Implementation Approach

**Static Analysis via Nix Evaluation** (Required)

Use `nix eval` to extract systemd service configurations from NixOS flake outputs:

```bash
nix eval .#nixosConfigurations.<system>.config.systemd.services --json
```

This approach:
- Analyzes configurations without needing access to running systems
- Works with any commit/branch in git history
- Fits Crystal Forge's existing flake evaluation infrastructure
- Can be scanned on-demand or triggered per commit

**NOT in scope for this task:**
- Runtime analysis via `systemd-analyze security` (future enhancement)
- SSH access to deployed systems (not required)

## Database Schema Design

Model after existing CVE scan infrastructure:

**Tables:**
- `hardening_scans` - scan metadata (system_id, commit_sha, status, scan_time, overall_score)
- `service_hardening_results` - per-service results (scan_id, service_name, hardening_score, options_json)
- Optional: `hardening_scan_justifications` - suppress false positives (similar to CVE justifications)

**Integration points:**
- Reuse scan status tracking pattern (pending/running/completed/failed)
- Link to existing `systems` and `commits` tables
- Add security posture to `view_system_detail` aggregation

## Key Hardening Options to Track

Track ~10-15 critical systemd security directives per service:

**Namespace Isolation:**
- PrivateTmp, PrivateDevices, PrivateNetwork, PrivateUsers
- ProtectHome, ProtectSystem, ProtectKernelTunables, ProtectKernelModules

**Capability Restrictions:**
- CapabilityBoundingSet, AmbientCapabilities
- NoNewPrivileges

**Syscall Filtering:**
- SystemCallFilter, SystemCallArchitectures

**Resource/Access Controls:**
- MemoryDenyWriteExecute, LockPersonality
- RestrictRealtime, RestrictSUIDSGID, RestrictNamespaces
- RestrictAddressFamilies
- ReadWritePaths, ReadOnlyPaths, InaccessiblePaths

Reference: https://www.freedesktop.org/software/systemd/man/systemd.exec.html#Security

## Scoring Algorithm

Calculate a hardening score (0-100) per service based on:

1. Presence/absence of key hardening options (weighted)
2. Option values (e.g., ProtectSystem=strict scores higher than ProtectSystem=true)
3. Option combinations (synergistic hardening)

**Initial weights (can be refined):**
- Namespace isolation options: 30% of score
- Capability restrictions: 25% of score
- Syscall filtering: 20% of score
- Resource/access controls: 25% of score

**Risk categorization:**
- 80-100: Well-hardened (green)
- 60-79: Moderately hardened (yellow)
- 40-59: Poorly hardened (orange)
- 0-39: Vulnerable (red)

## Dashboard Views

**Fleet-Level Dashboard (Admin):**
- Overall hardening score distribution (donut chart, reuse CVE dashboard pattern)
- Top 10 least-hardened services across fleet
- System comparison table (which NixOS configs are most secure)
- Scan status indicators per system

**System-Level View:**
- Per-service hardening breakdown table (sortable/filterable)
- Service name, hardening score, risk level, missing critical options
- Drill-down to service detail
- Integration with existing system detail page

**Service Detail View:**
- Specific systemd directives enabled/disabled
- Which options are missing and their impact
- Link to NixOS config source (if available)
- Option to justify/suppress findings (similar to CVE justifications)

## Integration with Existing Infrastructure

Leverage:
- **NixOS config discovery:** `list_nixos_configurations_from_commit()` to find systems
- **Flake evaluation:** Existing Nix eval infrastructure
- **Dashboard components:** Reuse `BuildSummaryPanel`, donut charts, security info cards
- **Scan trigger pattern:** Admin endpoint to trigger scans (like CVE scans)
- **Database views:** Extend `view_system_detail` to include hardening posture

## Out of Scope (Initial Implementation)

- Automatic remediation/fixing of hardening issues
- Custom hardening profiles per service type (e.g., database vs web server baselines)
- Historical tracking of hardening score over time (future: per-commit trending)
- Runtime validation via systemd-analyze security (future enhancement)
- Auto-generated NixOS config snippets to fix issues (future enhancement)
- CI integration to block merges on hardening regressions (future enhancement)
- Service categorization (network-facing vs internal) with risk weighting (future enhancement)

## Key Challenges

1. **Baseline calibration:** Avoid flagging everything as vulnerable; define sensible defaults
2. **NixOS indirection:** Modules may set options indirectly; need fully merged config evaluation
3. **False positives:** Some services legitimately need privileges (VPN needs CAP_NET_ADMIN)
   - Solution: Justification/suppression system (like CVE justifications)
4. **Performance:** Full config evaluation can be slow
   - Solution: Cache results, only re-scan on config changes
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Static analysis successfully extracts systemd service configurations via nix eval from NixOS flake outputs
- [ ] #2 Database schema created for hardening_scans and service_hardening_results tables
- [ ] #3 Hardening score (0-100) calculated for each service based on presence/absence of 10-15 key security directives
- [ ] #4 Fleet-level dashboard displays overall hardening posture with score distribution and top 10 vulnerable services
- [ ] #5 System-level view shows per-service hardening breakdown with sortable/filterable table
- [ ] #6 Service detail view displays specific enabled/disabled directives and missing critical options
- [ ] #7 Scan can be triggered on-demand for a specific system/commit
- [ ] #8 Results integrate with existing system detail views
- [ ] #9 Color-coded risk indicators (green/yellow/orange/red) based on hardening score ranges
<!-- AC:END -->
