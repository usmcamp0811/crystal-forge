---
id: TASK-155
title: Improve build queue UI with drag-and-drop reordering and better card design
status: Backlog
assignee: []
created_date: '2026-03-02 04:41'
labels:
  - ui
  - ux
  - build-queue
  - enhancement
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The build queue UI currently has several usability and visual issues that need to be addressed:

## Current Problems

1. **No Manual Reordering**: Cannot manually adjust build priority by dragging cards
2. **Poor Card Layout**: 
   - Target hostname in bold white text flows off the card
   - Same information duplicated twice on each card
   - Not informationally dense enough
   - Lacks clear at-a-glance status indicators
3. **Missing Key Info**: Hard to quickly identify:
   - Which system/hostname
   - Which environment
   - Current status (queued/building/success/failed)
   - Priority level

## Desired Improvements

### Drag-and-Drop Reordering
- Allow dragging build job cards to manually adjust queue priority
- Update `priority_weight` in database when order changes
- Visual feedback during drag (ghost card, drop zones)
- Persist changes to backend via API

### Better Card Design
- **Information Hierarchy**: Clear visual distinction between:
  - Primary: System hostname/target
  - Secondary: Environment, derivation path
  - Status: Queued/Building/Success/Failed with color coding
- **Remove Duplication**: Show each piece of info once
- **Status Indicators**: 
  - Color-coded badges (queued=blue, building=yellow, success=green, failed=red)
  - Build progress indicator for active builds
  - Time elapsed/estimated
- **Compact Layout**: Fit more information in less space
- **Responsive**: Cards should adapt to container width

### Example Card Layout
```
┌─────────────────────────────────────┐
│ 🟦 QUEUED          Priority: 20.0  │
│ nixos-server-01    prod            │
│ /nix/store/abc...xyz-config        │
│ Queued: 5m ago                      │
└─────────────────────────────────────┘
```

## Technical Considerations

- Use drag-and-drop library (dnd-kit, react-beautiful-dnd, or similar for Dioxus)
- API endpoint to update job priority: `PATCH /api/v1/builders/jobs/{id}/priority`
- Optimistic UI updates with rollback on failure
- Consider accessibility (keyboard navigation for reordering)

## Related
- This is part of the build queue feature from TASK-143
- May benefit from professional UI/UX design review (see separate issue)
<!-- SECTION:DESCRIPTION:END -->
