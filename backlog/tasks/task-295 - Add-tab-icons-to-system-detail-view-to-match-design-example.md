---
id: TASK-295
title: Add tab icons to system detail view to match design example
status: Backlog
assignee: []
created_date: '2026-05-10 13:28'
labels:
  - ui
  - design-system
  - icons
dependencies: []
priority: medium
ordinal: 250000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The system detail view tabs are currently using inline SVG icons directly in the component code, but they should use the Icon component system to match our design example at `/home/mcamp/code/crystal-forge/crystal-forge/project/components/SystemDetail.jsx`.

## Current State

The tabs in `packages/web-ui/src/views/system_detail.rs` (lines 883-975) have inline SVG definitions for each tab icon:
- Overview: house/home icon (inline SVG)
- Deploy: deploy icon (inline SVG)
- History: clock icon (inline SVG)
- CVEs: shield icon (inline SVG)
- Hardening: shield with minus icon (inline SVG)
- Logs: terminal icon (inline SVG)
- Config: file icon (inline SVG)

## Design Example

The JSX design example at `/home/mcamp/code/crystal-forge/crystal-forge/project/components/SystemDetail.jsx` (lines 75-96) shows the expected pattern:

```jsx
{[
  { k: "overview",   l: "Overview",  i: "dashboard" },
  { k: "deploy",     l: "Deploy",    i: "deploy" },
  { k: "history",    l: "History",   i: "history" },
  { k: "logs",       l: "Logs",      i: "terminal" },
  { k: "config",     l: "Config",    i: "file" },
  { k: "cves",       l: "CVEs",      i: "shield", badge: ... },
  { k: "hardening",  l: "Hardening", i: "key" },
].map(t => (
  <button ...>
    <Icon name={t.i} size={13} /> {t.l}
    {t.badge != null && <span className="sd-tab-badge">{t.badge}</span>}
  </button>
))}
```

Note: The design uses size={13} for tab icons.

## Required Changes

### 1. Extend Icon Component

Add missing icon variants to `packages/web-ui/src/components/icon.rs`:
- `Dashboard` (for Overview tab)
- `Deploy` (for Deploy tab)
- `History` (for History tab)
- `Shield` (for CVEs tab - already exists inline, extract it)
- `Key` (for Hardening tab)
- `File` (for Config tab)

The Terminal icon already exists in the Icon component and can be reused.

### 2. Update Tab Rendering

Replace the inline SVG definitions in `system_detail.rs` with Icon component calls:

```rust
match tab {
    Tab::Overview => rsx!(Icon { name: IconName::Dashboard, size: 13 }),
    Tab::Deploy => rsx!(Icon { name: IconName::Deploy, size: 13 }),
    Tab::History => rsx!(Icon { name: IconName::History, size: 13 }),
    Tab::Cves => rsx!(Icon { name: IconName::Shield, size: 13 }),
    Tab::Hardening => rsx!(Icon { name: IconName::Key, size: 13 }),
    Tab::Logs => rsx!(Icon { name: IconName::Terminal, size: 13 }),
    Tab::Config => rsx!(Icon { name: IconName::File, size: 13 }),
}
```

### 3. Icon SVG Paths

Extract the SVG path data from the existing inline icons or match them to the design system:
- **Dashboard**: Use a grid/squares icon for Overview
- **Deploy**: Keep existing deploy icon (up/down arrows)
- **History**: Keep existing clock icon
- **Shield**: Keep existing shield icon
- **Key**: Use a key icon for Hardening
- **File**: Use a document/file icon for Config
- **Terminal**: Already exists, reuse it

## Design System Consistency

- Icon size must be 13px (as shown in design example)
- Icons must use `currentColor` for stroke
- Icons must be consistent with the rest of the application
- Remove the custom `class: "w-3.5 h-3.5"` styling (currently 14px) and use explicit size={13}

## Technical Notes

**Icon Component Location**: `packages/web-ui/src/components/icon.rs`
**System Detail View**: `packages/web-ui/src/views/system_detail.rs`
**Design Reference**: `/home/mcamp/code/crystal-forge/crystal-forge/project/components/SystemDetail.jsx`

The Icon component already has the correct structure (SVG with view_box, stroke, fill, etc). We just need to add the new icon variants and update the tab rendering to use the Icon component instead of inline SVG.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Icon component has Dashboard variant added
- [ ] #2 Icon component has Deploy variant added
- [ ] #3 Icon component has History variant added
- [ ] #4 Icon component has Shield variant added
- [ ] #5 Icon component has Key variant added
- [ ] #6 Icon component has File variant added
- [ ] #7 All tab icons use Icon component instead of inline SVG
- [ ] #8 Tab icons are size={13} matching design example
- [ ] #9 Overview tab uses Dashboard icon
- [ ] #10 Deploy tab uses Deploy icon
- [ ] #11 History tab uses History icon
- [ ] #12 CVEs tab uses Shield icon
- [ ] #13 Hardening tab uses Key icon
- [ ] #14 Logs tab uses Terminal icon
- [ ] #15 Config tab uses File icon
- [ ] #16 Visual appearance matches design example
- [ ] #17 Icons render correctly in all tabs
<!-- AC:END -->
