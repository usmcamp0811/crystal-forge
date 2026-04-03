---
id: TASK-202
title: 'Dashboard Recent Deployments widget cuts off fifth item, needs scroll'
status: To Do
assignee: []
created_date: '2026-03-20 13:40'
updated_date: '2026-04-02 00:05'
labels:
  - frontend
  - dashboard
  - ux
  - bug
  - high-priority
dependencies: []
references:
  - packages/web-ui/src/components/dashboard/recent_deployments.rs
priority: high
ordinal: 3000
---

# Dashboard Recent Deployments widget cuts off fifth item, needs scroll

---

# Problem Statement

When the Recent Deployments widget on the Dashboard has 5 or more items, the fifth deployment gets cut off (partially obscured or not fully visible). The widget does not provide scrolling, making it impossible to view all recent deployments.

---

# Goal

Recent Deployments widget properly displays all deployment entries with a scrollable content area when there are more than 5 items.

---

# Non-Goals

- Redesigning the Recent Deployments widget
- Changing deployment data structure or API
- Adding pagination (scroll is sufficient for recent items)
- Adding filters or search to deployments
- Changing dashboard layout

---

# Acceptance Criteria

- [ ] Widget has a maximum height (e.g., 400px)
- [ ] When content exceeds max height, vertical scroll appears
- [ ] All deployment items are fully visible (none cut off)
- [ ] Scrollbar styled to match design system
- [ ] Widget header/title remains fixed (does not scroll)
- [ ] Smooth scroll behavior
- [ ] Touch-friendly scroll area for mobile/tablet
- [ ] Empty state displayed when no deployments
- [ ] Loading state shown while fetching
- [ ] Tested with 3, 5, 10, and 20 deployments
- [ ] Responsive behavior maintained on different screen sizes

---

# Architectural Constraints

- CSS-only solution preferred (no JavaScript scroll libraries)
- Use existing theme tokens for scrollbar styling
- Follow existing widget component patterns
- No changes to deployment data fetching logic
- Widget container must have fixed or max height
- Content area must have `overflow-y: auto` or `scroll`

---

# Verification Plan

Automated:
- UI build: `nix build .#web-ui`
- `nix develop -c cargo fmt -- --check`

Manual:
- Start dev stack
- Create multiple test deployments (10+)
- Navigate to Dashboard
- Verify Recent Deployments widget:
  - Shows scrollbar when >5 items
  - All items fully visible (not cut off)
  - Smooth scroll with mouse wheel
  - Header stays fixed during scroll
- Test with different deployment counts:
  - 0 deployments: empty state shown
  - 3 deployments: no scroll, all visible
  - 5 deployments: borderline, all visible
  - 10 deployments: scroll appears, all accessible
- Test responsive behavior:
  - Desktop (1920x1080)
  - Laptop (1366x768)
  - Tablet (768x1024)
- Check browser console for CSS errors

---

# Impact Areas

UI

- Recent Deployments widget component
- Widget CSS/styling
- Dashboard layout (ensure no grid breaking)

---

# Risk Level

Low

This is a simple CSS fix to add overflow scrolling. Very low risk of breaking functionality. The only potential issue is ensuring the scroll area is large enough to be useful but small enough to not dominate the dashboard.

Risks:
- Setting max height too small (not enough visible items)
- Setting max height too large (defeats purpose of widget grid)

Mitigations:
- Use a reasonable max height (400px shows ~5-6 items)
- Test with various deployment counts
- Ensure design consistency with other widgets

---

# Dependencies

None

---

# Follow-Up Tasks

- Add similar scroll fix to other dashboard widgets if needed (CVE Summary, Build Queue)
- Add "View All" link to full Deployments page
- Add configurable widget heights (user preference)
