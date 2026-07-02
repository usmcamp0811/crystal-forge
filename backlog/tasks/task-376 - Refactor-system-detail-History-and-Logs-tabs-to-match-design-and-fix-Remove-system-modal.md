---
id: TASK-376
title: >-
  Refactor system detail History and Logs tabs to match design and fix Remove
  system modal
status: To Do
assignee: []
created_date: '2026-07-02 00:55'
updated_date: '2026-07-02 00:56'
labels:
  - ui
  - system-detail
  - history
  - logs
  - modal
  - bug-fix
milestone: ui-views-system-detail
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
  - >-
    /home/mcamp/code/crystal-forge/dev/packages/web-ui/src/views/system_detail.rs
  - >-
    /home/mcamp/code/crystal-forge/dev/packages/web-ui/src/components/system/edit_system_modal.rs
priority: high
ordinal: 321000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The system detail view's History and Logs tabs do not match the polished design reference at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx`. Additionally, when a user clicks "Remove system from registry" in the Edit System modal, no confirmation modal appears, making it impossible to delete a system.

## Current State

**History Tab Issues:**
- Current implementation may lack the sophisticated timeline grouping seen in the design reference
- Missing features like collapsible restart clusters
- Deployment events and local rebuilds may not have the rich visual treatment (badges, reconciliation status, out-of-band indicators)
- "View logs" jump functionality from history events to the logs tab may be incomplete

**Logs Tab Issues:**
- May lack the live tail feature with auto-scroll
- Missing timezone toggle (UTC vs local time)
- Log level filtering may not match design
- Day separators and timestamp formatting may differ
- Jump-to-event highlighting from history tab may not work
- Missing download and clear functionality

**Remove System Modal:**
- The "Remove system from registry" button in `EditSystemModal` (`edit_system_modal.rs`) shows a confirmation prompt inline but does NOT trigger an actual deletion
- No modal confirmation dialog exists
- Users cannot remove systems from the registry

## Goal

1. Refactor the **History tab** to match the design reference:
   - Implement collapsible restart clusters for consecutive system reboots at the same generation
   - Add rich deployment event cards showing generation transitions, commit info, reconciliation status
   - Display out-of-band local rebuilds with drift indicators
   - Add "view logs" action that jumps to the corresponding log entry
   - Implement rollback action from history entries
   - Include infinite scroll pagination for deep history

2. Refactor the **Logs tab** to match the design reference:
   - Implement live tail mode with auto-scroll
   - Add timezone toggle (local vs UTC) with timezone abbreviation display
   - Implement log level filtering (all, info, warn, error)
   - Add day separators in the log stream
   - Implement jump-to-event from history tab with highlighting
   - Add clear and download buttons
   - Display live heartbeat and agent events when tailing

3. Fix the **Remove system modal**:
   - Create a proper confirmation modal component that appears when "Remove system from registry" is clicked
   - Wire up the modal to call the system deletion API endpoint
   - Include hostname confirmation for production systems (matching rollback modal pattern)
   - Show loading state during deletion
   - Navigate back to systems list on successful deletion
   - Show error toast on failure

## Explicit Non-Goals

- Do not change the Overview, Deploy, CVEs, Hardening, Config, or Compliance tabs
- Do not change tab ordering or navigation
- Do not modify the API endpoints (use existing endpoints for history, events, logs, and deletion)
- Do not implement new backend logging features beyond what's already exposed
- Do not add new icon types (reuse existing icon names)
- Do not change the overall system detail layout or header

## Acceptance Criteria
<!-- AC:BEGIN -->
### History Tab
- [ ] #1 History tab: Deployment events render as rich cards with generation transitions
- [ ] #2 History tab: Out-of-band rebuilds show reconciliation status badges
- [ ] #3 History tab: Consecutive restarts collapse into expandable clusters
- [ ] #4 History tab: Rollback and view-logs buttons work on deployment events
- [ ] #5 History tab: Infinite scroll loads older history
- [ ] #6 Logs tab: Live tail mode auto-scrolls to bottom
- [ ] #7 Logs tab: Timezone toggle switches between local and UTC
- [ ] #8 Logs tab: Log level filtering works (all/info/warn/error)
- [ ] #9 Logs tab: Day separators appear on date boundaries

### Logs Tab
- [ ] #10 Logs tab: Jump from history highlights target event for 2-3 seconds
- [ ] #11 Logs tab: Clear button removes tail lines
- [ ] #12 Remove modal: Opens when 'Remove system from registry' clicked
- [ ] #13 Remove modal: Requires hostname confirmation for production
- [ ] #14 Remove modal: Calls deletion API and navigates to systems list on success
- [ ] #15 Remove modal: Shows error toast on failure

### Remove System Modal

## Architecture & Constraints

- Use existing Dioxus components and styling classes
- Reuse `Icon`, `Card`, `Toast` components
- Match CSS class naming from the design reference where possible (`tl-row`, `tl-card`, `tl-node`, `sd-log-line`, etc.)
- Use existing API client functions from `api::client`
- Leverage existing `SystemHistoryEntry`, `SystemAgentEvent`, and `DeploymentLogEntry` models
- Create new modal component in `components/modals.rs` (where `RollbackConfirmDialog` lives)
- History and Logs tab components should live in `views/system_detail.rs` or be extracted to `components/system/` if they become large
- Use `use_signal` and `use_effect` for state management
- Ensure accessibility with proper ARIA labels and keyboard navigation

## Verification Plan

**Local dev server:**
```bash
cd packages/web-ui
nix develop -c cargo check
nix develop -c cargo clippy --all-targets -- -D warnings
nix develop -c trunk serve
```

**Manual testing:**
1. Navigate to a system detail page
2. Switch to History tab:
   - Verify deployment events show commit, author, generation transition
   - Verify restart clusters collapse and expand correctly
   - Click "view logs" and verify jump to logs tab works
   - Click rollback button and verify generation rollback modal opens
3. Switch to Logs tab:
   - Enable tail mode and verify auto-scroll
   - Toggle timezone and verify timestamps update
   - Filter by log level and verify filtering works
   - Jump from history and verify highlighting and scroll position
   - Click clear and verify simulated logs are cleared
4. Open Edit System modal:
   - Click "Remove system from registry"
   - Verify confirmation modal appears
   - For production system, verify hostname confirmation is required
   - Cancel and verify modal closes
   - Confirm deletion and verify navigation to systems list
5. Screenshot the History tab and Logs tab for MR attachment

**Visual regression:**
- Compare rendered History tab with design reference screenshot
- Compare rendered Logs tab with design reference screenshot
- Compare Remove confirmation modal with Rollback modal styling
<!-- AC:END -->



## Implementation Notes

**History Tab:**
- Build event model from `SystemHistoryEntry` and `SystemAgentEvent`
- Group consecutive `startup` events by generation
- Render deployment events with commit info from history entries
- Use `onLogsJump` callback to switch tabs and pass event ID
- Use existing rollback modal trigger

**Logs Tab:**
- Synthesize log lines from deployment history (as in the design reference)
- Implement live tail with interval timer adding new heartbeat/agent events
- Store timezone preference in component state
- Implement day separator logic based on date boundary
- Accept `jump` prop with event ID and scroll to `data-ev` attribute
- Use `scrollRef` for programmatic scrolling

**Remove Modal:**
- Create `DeleteSystemConfirmDialog` component
- Accept system hostname, environment, and on_confirm/on_close callbacks
- Call `DELETE /api/systems/:id` endpoint (check if this exists; if not, note in implementation)
- Show spinner during deletion
- Use navigator to push to systems list route on success
- Use toast to show error on failure

## Related Tasks & Dependencies

- History tab implementation depends on existing `SystemHistoryEntry` and `SystemAgentEvent` APIs
- Logs tab may depend on historical deployment logs API (check if `/api/systems/:id/logs` exists)
- Delete system modal depends on DELETE system API endpoint (verify endpoint exists)

## Risk Assessment

**Medium Risk:**
- Log synthesis from history may not produce realistic log output without real agent logs
- Delete endpoint may not exist yet (may need backend task)
- Timezone handling on WASM may have edge cases

**Mitigation:**
- Use design reference's log synthesis approach as a proven pattern
- Check for delete endpoint early; create backend task if missing
- Test timezone toggle in browser with different system locales
<!-- SECTION:DESCRIPTION:END -->
