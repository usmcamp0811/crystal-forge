---
id: TASK-158
title: Responsive Sidebar Navigation - Usable at All Screen Sizes
status: To Do
assignee: []
created_date: '2026-03-02 13:45'
updated_date: '2026-03-12 01:28'
labels:
  - frontend
  - ui
  - ux
  - responsive
  - web-ui
milestone: m-15
dependencies: []
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current sidebar navigation disappears entirely when the viewport width decreases (mobile/tablet), leaving users with no way to navigate the application. This violates basic UI/UX best practices for responsive design.

**Current Behavior:**
- Sidebar visible at desktop width (>1024px)
- Sidebar completely disappears at smaller widths (<1024px)
- No hamburger menu or alternative navigation appears
- Users on smaller screens cannot navigate between views

## Goal

Implement a fully responsive sidebar that remains accessible at all screen sizes using industry-standard patterns:

1. **Desktop (>1024px):** Full-width sidebar with labels + icons
2. **Tablet (768px-1024px):** Collapsed sidebar with icons only (icons-only mode)
3. **Mobile (<768px):** Hamburger menu with slide-out drawer navigation

**Key Requirements:**
- Smooth transitions between states
- Persistent navigation access at all breakpoints
- Maintain usability and accessibility
- Follow established design patterns (Material Design, Tailwind best practices)

## Scope

### Phase 1: Implementation

**Desktop (>1024px):** Keep existing behavior - full sidebar with icon + label
```toml
# Current: sidebar width ~240px
# Shows: icon + label for each nav item
```

**Tablet (768px-1024px):** Icons-only collapsed sidebar
```toml
# Sidebar width ~64px
# Shows: icon only (no labels)
# Hover: tooltip with label
# Keep: top logo, user menu
```

**Mobile (<768px):** Hamburger menu with drawer
```toml
# Top bar: hamburger icon (left) + logo (center)
# Tap hamburger: slide-out drawer from left
# Drawer: full navigation list with labels
# Overlay: tap outside to close
# Close: X button or swipe
```

### Phase 2: Additional Features

1. **Sidebar toggle button** - Allow users to collapse/expand at will
2. **Remember preference** - Store sidebar state in localStorage
3. **Keyboard shortcuts** - Alt+S to toggle sidebar (desktop)
4. **Touch-friendly** - Larger tap targets on mobile

### Phase 3: Styling

- Consistent with existing design system
- Smooth CSS transitions (200-300ms)
- Proper spacing and typography at all sizes
- Dark/light mode compatible

## Technical Implementation

### CSS/Tailwind Approach

```rust
// Desktop
w-64 (full sidebar)

// Tablet - use responsive classes
md:w-16 (icons only)
md:hover:w-64 (expand on hover - optional)

// Mobile
hidden (hide sidebar, show hamburger)
```

### Component Structure

```rust
// AppShell.rsx structure
<AppShell>
  <TopBar>
    <HamburgerButton />  // visible on mobile/tablet
    <Logo />
    <UserMenu />
  </TopBar>
  
  <Sidebar>  // visible on desktop, icons-only tablet
    <NavItems />
  </Sidebar>
  
  <MobileDrawer>  // visible on mobile tap
    <NavItemsWithLabels />
  </MobileDrawer>
  
  <MainContent>
    <Route />
  </MainContent>
</AppShell>
```

## Non-Goals

- ❌ Changing color scheme or visual design
- ❌ Adding new navigation items
- ❌ Desktop sidebar collapse animation (keep simple)
- ❌ Complex gesture navigation (keep simple taps)

## Design Best Practices

1. **Never hide navigation** - Always provide a way to navigate
2. **Progressive disclosure** - Show more as screen allows
3. **Touch targets** - Minimum 44x44px on mobile
4. **Visual feedback** - Clear active/hover states
5. **Performance** - No layout shifts during transitions

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Desktop (>1024px): Full sidebar with icon + label visible
- [ ] #2 Tablet (768-1024px): Icons-only sidebar with tooltips on hover
- [ ] #3 Mobile (<768px): Hamburger button in top bar
- [ ] #4 Mobile: Slide-out drawer navigation on hamburger tap
- [ ] #5 Mobile drawer closes on X button, tap outside, swipe gesture
- [ ] #6 Sidebar toggle button works at all sizes
- [ ] #7 Smooth CSS transitions between states (200-300ms)
- [ ] #8 No horizontal scroll or overflow issues
- [ ] #9 Touch-friendly tap targets (44px minimum)
- [ ] #10 Active route clearly highlighted in all states
- [ ] #11 Works in both dark and light modes
- [ ] #12 cargo fmt and cargo clippy pass

## Risk Level

Low (UI/UX improvement, no backend changes)
<!-- SECTION:DESCRIPTION:END -->

- [ ] #13 Tested on responsive design mode or real devices
- [ ] #14 Sidebar and top bar junction is visually clean with no awkward visible lines or gaps
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Responsive sidebar tested on mobile, tablet, and desktop viewports
- [ ] #2 Navigation accessible at all screen sizes without page reload
- [ ] #3 No layout shifts during transitions
- [ ] #4 Visual design consistent with existing design system
<!-- DOD:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User request: Refine the look of the sidebar and top bar where they come together - there's currently a visible line or something that doesn't look good. This should be addressed as part of the responsive implementation.
<!-- SECTION:NOTES:END -->
