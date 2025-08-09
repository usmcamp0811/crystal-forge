# Systems Status View - Technical Notes

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
