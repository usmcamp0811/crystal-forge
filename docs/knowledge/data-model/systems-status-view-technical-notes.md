---
type: Data Model
title: "Systems Status View - Technical Notes"
description: "Records why view_systems_status_table showed every system as Unknown State, why derivation_path and nix hash matching failed, and the derivation-name matching solution with its schema dependencies and future considerations."
tags:
  - crystal-forge
  - system-status
  - view-systems-status-table
  - derivation-matching
  - developer-notes
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/dev-notes.md at commit 3b23d36f"
    title: "Developer notes (dev-notes.md)"
---
# Systems Status View - Technical Notes

> **Status:** The status rules in this part (`Offline`, `Unknown State`, `Up to Date`, `Outdated`) are the older evaluation-pipeline logic. The historical note in [Legacy system status determination logic](../concepts/legacy-system-status-determination.md) says migrations 0287 to 0291 replaced that logic with the store-path contract in [System Deployment Status View](views/view-system-deployment-status.md). The view `view_systems_status_table` is created by `packages/default/crates/cf-server/migrations/0076_restore_dashboard_views_after_cascade_drop.sql`. This migration did not read its current definition.

## Problem Statement

The `view_systems_status_table` was incorrectly showing all systems as "Unknown State" despite having successful deployments. The view determines whether systems are running the latest configuration by comparing deployed systems against available commits.

## Root Cause Analysis

### Initial Approach (Broken)

The original view attempted to join `system_states.derivation_path` with `derivations.derivation_path`:

- **Deployed paths** (from `system_states`): `/nix/store/c2caz4arvnsslgygc4lylxj1byy9bd1p-nixos-system-butler-25.05.20250806.077ce82`
- **Derivation paths** (from `derivations`): `/nix/store/kj9l2mqq7laanvm8sryzq6dny2qrs47f-nixos-system-butler-25.05.20250806.077ce82.drv`

**Key Issue**: These paths are fundamentally different:

- The deployed path is the **built result**
- The derivation path is the **build recipe** (`.drv` file)
- They have different store hashes and the derivation path includes `.drv` extension

### Hash Extraction Attempt (Failed)

Attempted to extract commit hashes from deployed paths, but discovered:

- Short hashes in paths (e.g., `077ce82`, `b6bab62`) are **nixpkgs commit hashes**
- Our git commit hashes are different (e.g., `c881116`, `32d71b8`)
- No reliable way to correlate these hashes

## Solution

### Correct Approach

Match systems by **derivation name** and compare the most recent successful evaluation against the latest available commit:

1. **Current State**: Get latest deployment timestamp and system info per hostname
2. **Latest Successful Derivation**: For each system name, find the most recent successful derivation evaluation (`dry-run-complete`, `build-complete`, or `complete`)
3. **Latest Available Commit**: Get the newest commit for each system's flake
4. **Status Logic**:
   - **Offline**: No deployment recorded
   - **Unknown State**: Deployed but no successful derivation found
   - **Up to Date**: Latest successful derivation matches latest commit
   - **Outdated**: Latest successful derivation is behind latest commit

### Key Insight

The relationship is: `systems.hostname` → `derivations.derivation_name` → `commits.git_commit_hash`

This approach works because:

- Derivation names correspond to system hostnames
- We can reliably track which commits have successful evaluations
- We compare evaluation commits against available commits, not deployed paths

## Implementation Notes

- Use `DISTINCT ON` with `ORDER BY timestamp DESC` to get latest records
- Filter derivations to `derivation_type = 'nixos'` and successful statuses
- Handle NULL cases appropriately for offline/unknown systems
- Sort results to show up-to-date systems first

## Database Schema Dependencies

- `system_states`: Current deployment state per hostname
- `derivations`: Build evaluations linked to commits
- `derivation_statuses`: Success/failure status of evaluations
- `commits`: Git commits in flakes
- `systems`: System registration with flake associations
- `flakes`: Repository configurations

## Future Considerations

- Consider adding deployment tracking that directly links `system_states` to `derivations.id`
- Investigate if NixOS provides a way to extract original commit info from deployed paths
- Monitor for cases where derivation names don't match hostnames exactly


> **Status:** The "Future Considerations" above are proposed. This migration did not verify them. Migration `packages/default/crates/cf-server/migrations/0155_system_events_timeline.sql` later created `system_events`; see [Authoritative system_events timeline](system-events-timeline.md).

## Related concepts

- [Legacy system status determination logic](../concepts/legacy-system-status-determination.md)
- [System Deployment Status View](views/view-system-deployment-status.md)
- [Commit Deployment Timeline View developer notes](commit-deployment-timeline-developer-notes.md)
