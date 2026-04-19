---
id: TASK-276
title: Add systemd hardening scanner and dashboard view
status: Backlog
assignee: []
created_date: '2026-04-19 02:43'
updated_date: '2026-04-19 03:24'
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
priority: high
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
- `hardening_scan_justifications` - suppress false positives (similar to CVE justifications)

**Integration points:**
- Reuse scan status tracking pattern (pending/running/completed/failed)
- Link to existing `systems` and `commits` tables
- Add security posture to `view_system_detail` aggregation

## Scan Triggers

**On-demand scanning:**
- Admin endpoint to trigger scans for a specific system/commit
- Similar to existing CVE scan trigger pattern

**Automatic scanning:**
- Trigger hardening scan when new commits are processed
- Hook into existing commit processing pipeline
- Only scan if NixOS configurations changed (optimization)

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

## Justification/Suppression System

Some services legitimately require elevated privileges (e.g., VPN needs CAP_NET_ADMIN).

**Features:**
- Allow marking specific service findings as "justified" with reason
- Justified findings excluded from risk calculations
- Track who justified and when
- Model after existing CVE justification system

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
- Justify/suppress button with reason input

## Integration with Existing Infrastructure

Leverage:
- **NixOS config discovery:** `list_nixos_configurations_from_commit()` to find systems
- **Flake evaluation:** Existing Nix eval infrastructure
- **Dashboard components:** Reuse `BuildSummaryPanel`, donut charts, security info cards
- **Scan trigger pattern:** Admin endpoint to trigger scans (like CVE scans)
- **Commit processing:** Hook into pipeline for automatic scanning
- **Database views:** Extend `view_system_detail` to include hardening posture

## Test Strategy

**Validation approach:**
- Test against Crystal Forge's own NixOS configurations
- Use existing CF systems as real-world test cases for scoring algorithm
- Verify scoring produces sensible results for known service configurations

## Out of Scope (Initial Implementation)

- Automatic remediation/fixing of hardening issues
- Custom hardening profiles per service type (e.g., database vs web server baselines)
- Historical tracking of hardening score over time (future: per-commit trending)
- Runtime validation via systemd-analyze security (future enhancement)
- Auto-generated NixOS config snippets to fix issues (future enhancement)
- CI integration to block merges on hardening regressions (future enhancement)
- Service categorization (network-facing vs internal) with risk weighting (future enhancement)
- Scheduled/periodic re-scans (future enhancement)

## Key Challenges

1. **Baseline calibration:** Avoid flagging everything as vulnerable; define sensible defaults
2. **NixOS indirection:** Modules may set options indirectly; need fully merged config evaluation
3. **False positives:** Some services legitimately need privileges
   - Solution: Justification/suppression system included in this task
4. **Performance:** Full config evaluation can be slow
   - Solution: Cache results, only re-scan on config changes
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Static analysis successfully extracts systemd service configurations via nix eval from NixOS flake outputs
- [ ] #2 Database migrations create hardening_scans, service_hardening_results, and hardening_scan_justifications tables
- [ ] #3 Hardening score (0-100) calculated for each service based on presence/absence of 10-15 key security directives
- [ ] #4 Fleet-level dashboard displays overall hardening posture with score distribution and top 10 vulnerable services
- [ ] #5 System-level view shows per-service hardening breakdown with sortable/filterable table
- [ ] #6 Service detail view displays specific enabled/disabled directives and missing critical options
- [ ] #7 Scan can be triggered on-demand via admin endpoint for a specific system/commit
- [ ] #8 Automatic hardening scan triggers when new commits are processed
- [ ] #9 Justification system allows marking service findings as acceptable with reason text
- [ ] #10 Results integrate with existing system detail views (view_system_detail extended)
- [ ] #11 Color-coded risk indicators (green/yellow/orange/red) based on hardening score ranges
- [ ] #12 Scoring algorithm validated against Crystal Forge's own NixOS configurations
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Verification Plan

**Tier 2: nix flake check (Required)**

### Pre-Implementation
```bash
nix develop
cargo fmt -- --check
cargo clippy -- -D warnings
```

### During Implementation
```bash
# After schema changes
sqlx-refresh  # or sqlx database reset -y && cargo sqlx prepare
cargo check

# After Rust changes
cargo test --package cf-server
cargo test --package cf-core
```

### Final Verification
```bash
nix flake check
```

### Manual Validation
1. Start full stack: `full-stack up`
2. Navigate to admin dashboard
3. Trigger hardening scan for a test system
4. Verify fleet-level dashboard renders
5. Drill down to system view and verify service table
6. Click into service detail and verify directive display
7. Test justification flow (mark a finding as justified)
8. Verify automatic scan triggers on new commit processing

### Regression Check
- Existing CVE scanning must continue to work
- Existing system detail views must not break
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Notes

### Suggested File Structure
```
crates/cf-core/src/
├── hardening/
│   ├── mod.rs
│   ├── scanner.rs          # Nix eval + JSON parsing
│   ├── scoring.rs          # Score calculation logic
│   └── types.rs            # HardeningScan, ServiceResult, etc.

crates/cf-server/src/
├── routes/
│   └── hardening.rs        # API endpoints
├── handlers/
│   └── hardening.rs        # Request handlers

crates/cf-web/src/
├── views/
│   └── hardening/
│       ├── mod.rs
│       ├── fleet_dashboard.rs
│       ├── system_view.rs
│       └── service_detail.rs
```

### Migration Order
1. Create `hardening_scans` table
2. Create `service_hardening_results` table  
3. Create `hardening_scan_justifications` table
4. Add columns to or create view for `view_system_detail`

### Key Reference Files (Existing Patterns)
- CVE scan infrastructure: look at cve_scans table and related handlers
- Scan trigger pattern: look at existing CVE scan endpoints
- Dashboard components: BuildSummaryPanel, donut charts in admin views
- Justification pattern: system_cve_justifications table

### Nix Eval Command
```bash
nix eval .#nixosConfigurations.<system>.config.systemd.services --json 2>/dev/null
```

Parse output as JSON object where keys are service names and values contain:
- `serviceConfig.PrivateTmp`
- `serviceConfig.ProtectSystem`
- `serviceConfig.NoNewPrivileges`
- etc.

### Risk Areas
- Large JSON output from nix eval (may need streaming/chunking)
- Service filtering (ignore generated/internal services?)
- Score calibration (may need iteration based on real-world results)

## View Locations (Confirmed)

### Fleet-Level Hardening Dashboard
**Route:** `/hardening`
**Sidebar position:** Between "CVEs" and "Policies"

Shows:
- Overall fleet hardening score distribution (donut chart)
- Top 10 least-hardened services across all systems
- System comparison table
- Scan status indicators

### Per-System Hardening Tab
**Route:** `/systems/:id` → new "Hardening" tab
**Tab order:** Overview | History | Policy | CVEs | **Hardening** | Logs

Shows:
- Per-service hardening breakdown table (sortable/filterable)
- Service name, score, risk level, missing critical options
- Drill-down links to service detail

### Service Detail View
**Route:** Modal or `/systems/:id/hardening/:service_name`

Shows:
- Specific systemd directives enabled/disabled
- Missing options and their impact
- Justify/suppress button with reason input

### Navigation Flow
```
/hardening (fleet) → click system → /systems/:id#hardening → click service → detail modal
```
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All database migrations pass and sqlx metadata is in sync
- [ ] #2 nix flake check passes
- [ ] #3 cargo fmt -- --check passes
- [ ] #4 cargo clippy -- -D warnings passes
- [ ] #5 Unit tests cover scoring algorithm logic
- [ ] #6 Integration tests verify scan trigger endpoints
- [ ] #7 Dashboard views render correctly with test data
- [ ] #8 No regressions in existing CVE scanning functionality
<!-- DOD:END -->
