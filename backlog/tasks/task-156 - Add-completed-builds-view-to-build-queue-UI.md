---
id: TASK-156
title: Add completed builds view to build queue UI
status: To Do
assignee: []
created_date: '2026-03-02 04:41'
labels:
  - ui
  - build-queue
  - enhancement
  - feature
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The current build queue UI only shows active/queued builds. We need a way to view historical completed builds.

## Current Limitation

- Build queue view only shows `status IN ('queued', 'building')`
- No way to see:
  - Successfully completed builds
  - Failed builds (after max retries)
  - Build history over time
  - Build duration/performance metrics

## Proposed Solution

Add a tab/view switcher on the builds page to toggle between:

1. **Active Queue** (current view)
   - Shows queued + building jobs
   - Real-time updates
   - Drag-and-drop reordering (from TASK-155)

2. **Completed Builds** (new view)
   - Shows completed builds (success + failed)
   - Filterable by:
     - Status (success/failed)
     - Environment
     - System/hostname
     - Date range
   - Sortable by:
     - Completion time
     - Duration
     - Status

## UI Considerations

### View Options to Explore

**Option A: Tabs**
```
[ Active Queue ] [ Completed Builds ]
```

**Option B: Table View for Completed**
- More information dense
- Better for historical data
- Sortable columns: System, Environment, Status, Duration, Completed At
- Row click to expand details (logs, error messages)

**Option C: Timeline View**
- Visual timeline of build activity
- Color-coded by status
- Good for identifying patterns/issues

### Recommended Approach

Start with tabs + table view for completed builds:
- Familiar UX pattern
- Table format works well for historical data
- Can add timeline/chart views later

## API Requirements

- Endpoint to fetch completed builds: `GET /api/v1/build-jobs?status=completed&limit=100`
- Support pagination (100-500 results at a time)
- Filter parameters: status, environment_id, system, date_range

## Data to Display

Each completed build should show:
- System/hostname
- Environment
- Status (success/failed with visual indicator)
- Started at / Completed at
- Duration
- Logs (link or expandable)
- Error message (if failed)
- Retry count (if applicable)

## Future Enhancements

- Export build history as CSV/JSON
- Build analytics dashboard (success rate, avg duration)
- Build log viewer with syntax highlighting
- Rebuild button for failed builds

## Notes

This requires professional UI/UX design input (see TASK-156) to determine the best visualization approach for completed builds. The table view is a safe starting point but may not be optimal.
<!-- SECTION:DESCRIPTION:END -->
