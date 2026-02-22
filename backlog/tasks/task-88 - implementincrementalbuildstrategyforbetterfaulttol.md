# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 171451464\n- **Issue IID:** 88\n- **Title:** Implement Incremental Build Strategy for Better Fault Tolerance\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-08-02T20:36:20.686Z\n- **Updated at:** 2025-08-04T03:06:46.489Z\n\n# Description\n\n## Problem

The current build process for NixOS system evaluations frequently fails when running as a systemd service. Long-running `nix build` commands for full systems appear to be getting killed, likely due to:

- Extended build times (30+ minutes for complex systems)
- Systemd resource limits or watchdog timeouts
- Process cleanup killing subprocesses

This results in many builds failing that would otherwise succeed, particularly impacting reliability in production deployments.

## Proposed Solution

Replace monolithic system builds with an incremental approach:

1. **Discovery Phase**: Use `nix build --dry-run --print-out-paths` to get list of derivations needed
2. **Individual Builds**: Build each derivation separately using `nix-store --realise`
3. **Granular Scanning**: Run vulnix scans on individual components as they complete
4. **System Assembly**: Final system-level scan once all components are built

## Implementation Plan

### Database Changes
- Create individual `evaluation_targets` records for each derivation
- Track build status per derivation rather than per system
- Maintain relationships between component derivations and parent systems

### Build Process Changes
- Modify dry-run parsing to extract individual store paths
- Update build loop to process single derivations via `nix-store --realise`
- Add logic to detect heavy builds (Firefox, etc.) and adjust `--max-jobs=1`

### Scanning Changes
- Scan individual derivations as they complete
- Aggregate vulnerability results at system level
- Maintain current system-level reporting

## Benefits

- **Fault Tolerance**: Individual derivation failures don't kill entire system builds
- **Progress Visibility**: Track which components are built vs pending
- **Resource Control**: Tune build parameters per component type
- **Restart Resilience**: Service restarts don't lose all progress
- **Better Monitoring**: Individual component build times and success rates

## Acceptance Criteria

- [x] Parse `nix build --dry-run` output to extract derivation list
- [x] Create evaluation targets for individual derivations
- [x] Build derivations individually using `nix-store --realise`
- [x] Scan individual components with vulnix
- [x] Aggregate results to system-level views
- [x] Maintain existing API compatibility for system status queries

## Files to Modify

- `src/models/evaluation_targets.rs` - Add derivation discovery logic
- `src/services/mod.rs` - Update build loop for incremental processing
- Database queries for handling component-level tracking\n\n# Milestone\n\n{
  "id": 6040389,
  "iid": 5,
  "group_id": 0,
  "project_id": 70402481,
  "title": "v0.4.0 - Enterprise Security",
  "description": "**Goal**: Prepare for LAN or production deployment.\r\n\r\n* [x] Shared secret or token auth for agents\r\n* [ ] Harden webhook input (validate source repo)\r\n* [ ] Limit derivation evaluation to allowlist\r\n* [ ] Add systemd service definition + journald config\r\n* [ ] Write tests for all HTTP handlers + flake logic",
  "start_date": null,
  "due_date": null,
  "state": "active",
  "web_url": "https://gitlab.com/crystal-forge/crystal-forge/-/milestones/5",
  "updated_at": "2025-06-28T02:47:15.836Z",
  "created_at": "2025-06-14T03:46:26.876Z",
  "expired": false
}\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n