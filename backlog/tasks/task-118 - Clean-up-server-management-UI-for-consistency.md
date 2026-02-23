---
id: TASK-118
title: Clean up server management UI for consistency
status: Done
assignee: []
created_date: '2026-02-22 23:40'
updated_date: '2026-02-23 00:01'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make systems_list.rs and system_detail.rs more consistent with builds.rs and flakes_list.rs in terms of density, layout, spacing, and visual hierarchy
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User reported additional UI issues with admin user list view:
- Buttons have white outline with no fill (incorrect styling)
- Button padding is missing (text touching borders)
- Need password reset functionality via modal
- Table should be sortable like flakes view
- Search/filter should not be full-width
- Table background cuts off after actions column
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Improved UI consistency between server management views and builds/flakes views by:
- Matching table styling (rounded borders, consistent hover states)
- Simplifying filter bar layout
- Consolidating action buttons
- Flattening header layouts
- Improving density and spacing throughout

Fixed all admin user list UI issues:
- Button styling now has proper gray backgrounds and padding
- Added password reset modal with strength indicator
- Table structure matches flakes view pattern
- Search/filter layout is 4-column on large screens
- Table rows have hover states
- Delete button has proper red background styling
<!-- SECTION:FINAL_SUMMARY:END -->
