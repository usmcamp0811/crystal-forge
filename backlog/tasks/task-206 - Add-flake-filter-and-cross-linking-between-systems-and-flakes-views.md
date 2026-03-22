---
id: TASK-206
title: Add flake filter and cross-linking between systems and flakes views
status: Backlog
assignee: []
created_date: '2026-03-20'
updated_date: '2026-03-20'
labels:
  - frontend
  - ux
  - filtering
  - navigation
priority: medium
ordinal: 1200
---

# Add flake filter and cross-linking between systems and flakes views

---

# Problem Statement

Users cannot easily see which systems are using a specific flake. There's no way to:
- Filter the systems list by flake
- See a list of systems from the flakes view
- Navigate between flakes and their associated systems

This makes it difficult to:
- Understand flake usage and impact
- Find all systems affected by a flake update
- Audit which systems are using deprecated or problematic flakes

---

# Goal

Provide clear visibility and navigation between flakes and the systems that use them.

Users should be able to:
- Filter systems by flake in the Systems view
- See a count of systems using each flake in the Flakes view
- Click through from a flake to see all systems using it
- Understand the relationship between flakes and systems

---

# Non-Goals

- Changing flake assignment for systems (that's a separate edit action)
- Showing historical flake usage (current state only)
- Multi-flake filtering (single flake filter is sufficient)
- Advanced query builder with AND/OR logic
- Filtering by flake attributes (version, commit, etc.)

---

# Acceptance Criteria

**Systems View**:
- [ ] Filter dropdown/input to filter systems by flake
- [ ] Filter shows list of available flakes with system count
- [ ] Selecting a flake filters the systems list
- [ ] Filter state visible in UI (e.g., "Showing systems using 'nixpkgs'")
- [ ] Clear filter button/action
- [ ] Filter persists in URL query params (shareable, bookmarkable)
- [ ] Works in both card view and table view

**Flakes View**:
- [ ] Each flake card/row shows count of systems using it
- [ ] "View Systems" link/button on each flake
- [ ] Clicking link navigates to Systems view with flake filter applied
- [ ] Zero-usage flakes clearly indicated (e.g., "0 systems")

**Navigation**:
- [ ] Breadcrumb or indicator shows you came from flakes view
- [ ] Back navigation returns to flakes view (not generic systems view)

**Performance**:
- [ ] Filter operates on client-side data (no backend API changes needed initially)
- [ ] Large system lists remain performant with filter active

---

# Architectural Constraints

- Filter logic should be client-side (systems data already loaded)
- Use existing System and Flake models (no schema changes)
- Follow existing view filter patterns (similar to existing view mode toggles)
- URL query params for filter state (e.g., `?flake=nixpkgs`)
- Maintain accessibility (keyboard navigation, screen readers)

---

# Verification Plan

Automated:
- `nix build .#web-ui`
- `cargo clippy -- -D warnings`
- `cargo fmt -- --check`

Manual:
1. Navigate to Flakes view
   - Verify system count shown on each flake card
   - Click "View Systems" on a flake with systems
   - Verify navigates to Systems view with filter applied
   - Verify only systems using that flake are shown
2. Navigate to Systems view
   - Open flake filter dropdown
   - Verify all flakes listed with system counts
   - Select a flake
   - Verify systems filtered correctly
   - Verify filter indicator shown
   - Click clear filter
   - Verify all systems shown again
3. URL state
   - Apply flake filter
   - Copy URL
   - Open in new tab
   - Verify filter state restored
4. Test with edge cases
   - Flake with 0 systems
   - Flake with 100+ systems
   - System with no flake assigned
   - Clear filter and reapply different flake

---

# Impact Areas

UI | Navigation

- Flakes view (add system count, "View Systems" link)
- Systems view (add flake filter dropdown)
- Routing/navigation (URL query params)
- Existing filter state management

---

# Risk Level

Low

This is purely additive UI functionality. No data model changes, no backend API changes (initially). Operates on already-loaded data.

Risks:
- Performance with large datasets (mitigated by client-side filtering)
- URL state complexity (mitigated by using existing patterns)
- UX clarity (filter might be missed - mitigated by clear UI placement)

---

# Dependencies

None

---

# Follow-Up Tasks

- Add backend API endpoint for filtered system queries (if client-side becomes slow)
- Add multi-select flake filter (filter by multiple flakes)
- Add flake version filter (filter by specific commit/version)
- Add "unused flakes" view (flakes with 0 systems)
- Add flake impact analysis (show systems + planned deployments)

---

# Design Notes

## UI Placement Options

**Systems View Filter**:
- Option 1: Dropdown in toolbar next to view mode toggle
- Option 2: Search/filter bar above system cards
- Option 3: Sidebar filter panel (more space for complex filters later)

**Recommendation**: Option 1 (toolbar dropdown) for consistency with existing view controls

**Flakes View System Count**:
- Option 1: Badge on flake card (e.g., "12 systems")
- Option 2: Dedicated info row in card
- Option 3: Column in table view

**Recommendation**: Option 2 (info row) with link: "12 systems using this flake →"

## Example URL Structure

```
/systems?flake=nixpkgs
/systems?flake=home-manager&view=table
```

## Example Filter UI

```
Systems View Toolbar:
┌──────────────────────────────────────────────────┐
│ [Grid] [Table]  |  Filter by: [Flake: nixpkgs ×] │
└──────────────────────────────────────────────────┘
```

```
Flake Card:
┌────────────────────────────────────┐
│ nixpkgs                            │
│ github:NixOS/nixpkgs/abc123        │
│                                    │
│ 24 systems using this flake →      │  ← clickable link
└────────────────────────────────────┘
```

## State Management

```rust
// In SystemsListView
let flake_filter = use_signal(|| {
    // Parse from URL query param
    query_params.get("flake").map(String::from)
});

let filtered_systems = systems.read()
    .iter()
    .filter(|sys| {
        if let Some(ref filter_flake) = *flake_filter.read() {
            sys.flake.as_ref().map(|f| f.name == *filter_flake).unwrap_or(false)
        } else {
            true
        }
    })
    .collect::<Vec<_>>();
```

## Accessibility

- Filter dropdown keyboard navigable
- Clear filter button has aria-label
- System count announced by screen readers
- Filter state communicated (e.g., "Filtered by nixpkgs, 24 results")
