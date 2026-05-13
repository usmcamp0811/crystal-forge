---
id: TASK-297
title: Rebuild Flakes View to Match JSX Design Mockup Exactly
status: In Progress
assignee: []
created_date: '2026-05-13 02:56'
updated_date: '2026-05-13 03:30'
labels:
  - ui
  - web-ui
  - flakes
  - design-system
  - mockup-alignment
dependencies: []
references:
  - 'https://example.com/flakes-view-jsx'
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - packages/web-ui/src/views/flakes_list.rs
priority: high
ordinal: 252000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Overview

The current Dioxus implementation of the flakes view must be completely rebuilt to achieve pixel-perfect alignment with the design mockup at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx`.

This is a complete UI/UX overhaul requiring table/cards toggle view, side-tray commit explorer with timeline, file diff viewer, and pipeline status visualization.

## Current State vs Target State

### Current Implementation
- Basic flake list view
- Timeline cards in main view
- Limited commit history display
- No file-level diff viewer
- Performance issues (10s+ load times)

### Target Design (FlakesView.jsx - 567 lines)
- **Dual view modes:** Table view and Cards view with segmented toggle
- **Side tray commit explorer:** Slide-out panel with commit timeline, grouped by time buckets
- **File diff viewer:** Grid of changed files with add/del stats, click to view full diff in modal
- **Pipeline visualization:** Eval → Build → Rollout flow with status pills
- **Search and filtering:** Search flakes, filter commits within tray
- **Design system classes:** All fl-* prefixed classes from styles.css

## Key Design Features

### 1. Main View Structure
- Page header with stats subtitle: "N tracked · M systems · P synced"
- Action buttons: "Sync all" (ghost), "Add flake" (primary)
- Filter bar with search input, view mode toggle (Table/Cards), count display
- Table or Cards view based on selection
- Side tray overlay for selected flake

### 2. Side Tray (fl-tray)
- **Backdrop:** Click-to-close overlay
- **Header:** Flake name, branch chip, sync status, URL, Sync button, Close button
- **Two-pane body:**
  - **Left:** Commit list with search, time-bucketed groups (Today/This week/Earlier), timeline rail with dots/stems
  - **Right:** Selected commit detail with pipeline strip, files changed grid

### 3. Commit Timeline (fl-tray-commits)
- Search input with result count
- Time buckets: "Today", "This week", "Earlier"
- Rail visualization: dots and stems connecting commits
- Each commit shows: SHA, message, timestamp, author, eval/build status dots
- Active selection with purple highlight

### 4. Commit Detail Panel (fl-tray-detail)
- Commit header: SHA (purple), message, author, timestamp, +/- stats
- Pipeline strip: Eval pill → Arrow → Build pill → Arrow → Rollout pill
- Files changed section: Grid of file cards with diff stats and bars
- Click file card opens DiffModal

### 5. File Cards (fl-file-card)
- File icon, filename (truncated), path (muted, truncated)
- Stats: +add (green), -del (red), visual bar showing proportion
- Hover and focus states

### 6. Diff Modal (DiffModal component)
- Full-screen overlay with header (filename, close button)
- Line-by-line diff display with syntax highlighting
- Line numbers, add/del backgrounds

### 7. Pipeline Components
- **PipelineDot:** Small status indicator (eval/build) with colored dot
- **PipelinePill:** Larger pill with icon, label, status color
- **PipelineArrow:** Connecting arrow between stages
- **RolloutPill:** Shows deployed count vs total with progress indication

## CSS Classes to Implement

All classes must match the design system in `/home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css`:

- `.fl-tray-backdrop` - Dark overlay
- `.fl-tray` - Side panel slide-in
- `.fl-tray-head` - Tray header
- `.fl-tray-body` - Two-pane layout
- `.fl-tray-commits` - Commit list pane
- `.fl-tray-commits-search` - Search input container
- `.fl-commits-bucket` - Time bucket header
- `.fl-commit-item` - Individual commit in timeline
- `.fl-rail` - Timeline rail container
- `.fl-dot` - Timeline dot
- `.fl-stem` - Connecting line
- `.fl-tray-detail` - Detail pane
- `.fl-tray-commit-h` - Commit header
- `.fl-pipeline` - Pipeline status strip
- `.fl-files-section` - Files changed section
- `.fl-files-grid` - File cards grid
- `.fl-file-card` - Individual file card
- `.fl-file-card-head` - File name/path
- `.fl-file-name` - Filename (truncated)
- `.fl-file-path` - Path (muted, truncated)
- `.fl-file-stats` - Add/del stats
- `.fl-file-bar` - Visual proportion bar

## Data Models Required

```rust
pub struct FlakeListItem {
    pub id: i32,
    pub name: String,
    pub url: String,
    pub branch: String,
    pub description: Option<String>,
    pub status: String, // "synced", "syncing", "error"
    pub system_count: i32,
    pub last_sync: Option<DateTime<Utc>>,
}

pub struct FlakeCommitHistory {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub additions: i32,
    pub deletions: i32,
    pub files_changed: i32,
    pub eval_status: Option<String>,
    pub build_status: Option<String>,
    pub rollout_count: Option<i32>,
}

pub struct CommitFileChange {
    pub filename: String,
    pub additions: i32,
    pub deletions: i32,
    pub patch: Option<String>, // Full diff content
}
```

## Implementation Sections

This task requires rebuilding the entire flakes view from scratch:

### Phase 1: Main View Structure
- [ ] Page header with subtitle stats
- [ ] Filter bar with search, view toggle, count
- [ ] Table view component
- [ ] Cards view component
- [ ] View mode state management

### Phase 2: Side Tray Foundation
- [ ] Tray backdrop and slide-in animation
- [ ] Tray header with flake info
- [ ] Two-pane body layout
- [ ] ESC key to close
- [ ] Click backdrop to close

### Phase 3: Commit Timeline (Left Pane)
- [ ] Search/filter input with count
- [ ] Time bucketing logic (Today/This week/Earlier)
- [ ] Timeline rail with dots and stems
- [ ] Commit item component
- [ ] Active selection state
- [ ] Pipeline status dots

### Phase 4: Commit Detail (Right Pane)
- [ ] Commit header with SHA, message, stats
- [ ] Pipeline strip with pills and arrows
- [ ] Files changed section header
- [ ] File cards grid
- [ ] File card component with stats and bar

### Phase 5: Diff Modal
- [ ] Modal backdrop and container
- [ ] Diff header with filename
- [ ] Line-by-line diff rendering
- [ ] Syntax highlighting
- [ ] Add/del line backgrounds
- [ ] Line numbers
- [ ] Close button and ESC key

### Phase 6: Pipeline Components
- [ ] PipelineDot component
- [ ] PipelinePill component
- [ ] PipelineArrow component
- [ ] RolloutPill component
- [ ] Status color mapping

### Phase 7: Table/Cards Views
- [ ] FlakeTable component (8 columns)
- [ ] FlakeCards component (grid layout)
- [ ] Click handling to open tray
- [ ] Selection highlighting

### Phase 8: Integration
- [ ] Wire up API endpoints
- [ ] State management
- [ ] Loading states
- [ ] Empty states
- [ ] Error handling

## Reference Files

**Design mockup:** `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx` (567 lines)
**CSS design system:** `/home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css`
**Current implementation:** `packages/web-ui/src/views/flakes_list.rs`

## Non-Goals

- Performance optimization (that's TASK-215 and TASK-222)
- Backend API changes (unless required for new data fields)
- Database schema changes
- Git sync implementation changes

## Scope

**Files to modify:**
- `packages/web-ui/src/views/flakes_list.rs` - Complete rewrite
- Potentially create new components:
  - `packages/web-ui/src/components/flake_tray.rs`
  - `packages/web-ui/src/components/pipeline_status.rs`
  - `packages/web-ui/src/components/diff_modal.rs`

## ⚠️ CRITICAL REQUIREMENT ⚠️

**The UI/UX implementation MUST be EXACTLY as designed in FlakesView.jsx.**

- Every CSS class must match the design system
- All spacing, colors, typography, borders, shadows must be pixel-perfect
- All interactions (hover, active, focus) must match
- Layout must be identical
- Component structure must follow the design

**DEVIATION FROM THIS DESIGN WILL RESULT IN TASK REJECTION.**
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Main view has table/cards toggle matching design exactly
- [ ] #2 Filter bar with search, view mode segmented control, and count display
- [ ] #3 Side tray slides in from right with backdrop on flake selection
- [ ] #4 Tray header shows flake name, branch chip, sync status, URL, and action buttons
- [ ] #5 Commit timeline (left pane) has search input with result count
- [ ] #6 Commits grouped by time buckets: Today, This week, Earlier
- [ ] #7 Timeline rail with dots and connecting stems matches design
- [ ] #8 Each commit shows SHA (purple when selected), message, timestamp, author, pipeline dots
- [ ] #9 Active commit highlighted with purple accent
- [ ] #10 Commit detail (right pane) shows SHA, message, author, timestamp, +/- stats
- [ ] #11 Pipeline strip shows Eval pill → Build pill → Rollout pill with status colors
- [ ] #12 Files changed section displays grid of file cards
- [ ] #13 File cards show filename, path, +add (green), -del (red), visual proportion bar
- [ ] #14 Clicking file card opens DiffModal with full diff display
- [ ] #15 DiffModal shows line-by-line diff with syntax highlighting and line numbers
- [ ] #16 All fl-* CSS classes used correctly matching styles.css
- [ ] #17 Table view (when selected) shows flakes in table format
- [ ] #18 Cards view (when selected) shows flakes in card grid
- [ ] #19 ESC key closes tray and modals
- [ ] #20 Click backdrop closes tray
- [ ] #21 All typography (sizes, weights, colors) matches design exactly
- [ ] #22 All spacing and gaps match design exactly
- [ ] #23 All hover states, focus states, active states match design
- [ ] #24 Responsive behavior matches design patterns
- [ ] #25 Empty states for no flakes, no commits, no file changes
- [ ] #26 Loading states while fetching data
- [ ] #27 Visual appearance is pixel-perfect match to FlakesView.jsx
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## CRITICAL WARNING FOR IMPLEMENTING AGENT

The UI/UX design in `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx` is FINAL and MANDATORY.

DO NOT:
- Deviate from the design
- 'Improve' the design
- Use different CSS classes
- Modify spacing/colors/layout
- Make subjective design choices

DO:
- Study the design file thoroughly before writing any code
- Extract and replicate the exact CSS classes and structure
- Match every visual detail pixel-by-pixel
- Test against the design file to ensure perfect match

## Verification Plan

**Tier 1: Fast Local Confidence (Primary)**

```bash
# 1. Start dev environment
nix develop
db-only up
server-only up

# 2. Test main view
# - Navigate to /flakes
# - Verify page header with stats subtitle
# - Verify filter bar with search, toggle, count
# - Test table/cards view toggle
# - Verify layout matches design

# 3. Test side tray
# - Click a flake to open tray
# - Verify tray slides in from right
# - Verify backdrop appears
# - Test ESC key closes tray
# - Test click backdrop closes tray
# - Verify tray header matches design

# 4. Test commit timeline
# - Verify commits grouped by time buckets
# - Test search/filter functionality
# - Verify timeline rail with dots/stems
# - Test commit selection
# - Verify active state highlighting

# 5. Test commit detail
# - Verify commit header displays correctly
# - Verify pipeline strip shows eval→build→rollout
# - Verify files changed grid
# - Test file card click opens diff modal

# 6. Test diff modal
# - Verify modal opens on file card click
# - Verify diff display with line numbers
# - Verify syntax highlighting
# - Test ESC key closes modal
# - Test close button

# 7. Visual verification
# - Side-by-side browser comparison with design
# - Check all spacing, colors, typography
# - Verify all hover/focus/active states
# - Check responsive behavior

# 8. Run targeted tests
cargo test flakes
nix build .#checks.x86_64-linux.web-ui
```

**Tier 2: Full Integration (if making backend changes)**
- Only needed if API endpoints are modified
- Test with real data from database
- Verify performance under load

**Do NOT run nix flake check** unless:
- Backend API changes are required
- Database queries are modified
- New dependencies are added

## Architecture Constraints

**Frontend (Dioxus):**
- Component composition and reusability
- DTOs mirror server models
- State isolated from presentation
- No business logic in UI components
- Follow existing component patterns

**Files to Modify:**
- `packages/web-ui/src/views/flakes_list.rs` - Complete rewrite
- `packages/web-ui/src/components/flake_tray.rs` - New component
- `packages/web-ui/src/components/pipeline_status.rs` - New component
- `packages/web-ui/src/components/diff_modal.rs` - New component
- `packages/web-ui/src/api/models.rs` - Add new data models if needed

**Backend (Only if Required):**
- Add commit file changes endpoint if not exists
- Add diff content endpoint if not exists
- Use existing query patterns
- No unwrap in production paths

## Impact Areas

**High Impact:**
- Flakes view completely redesigned
- New side tray interaction pattern
- New diff viewer modal

**Medium Impact:**
- May need new API endpoints for file changes/diffs
- State management for tray/modal visibility

**Low Impact:**
- No database schema changes
- No git sync implementation changes

## Risk Level

**High** - Complete UI rewrite of critical user-facing view

## Dependencies

- Design file: `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx`
- CSS file: `/home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css`
- Icon component must support all required icons (sync, plus, search, git, user, file, x, rows, grid)

## Success Metrics

**Must pass before marking Done:**
1. Visual side-by-side comparison matches design pixel-perfect
2. All interactions work (tray open/close, modal open/close, view toggle)
3. ESC key closes tray and modals
4. Backdrop click closes tray
5. Timeline rail renders correctly with dots and stems
6. Pipeline visualization displays correctly
7. File diff modal shows diffs with syntax highlighting
8. web-ui checks pass
9. No console errors
10. No performance regressions (load within 5s)

LOCK: assistant on gray in ~/code/crystal-forge/TASK-297-rebuild-flakes-view

Started: 2026-05-13 03:00 UTC

Phase 1: Creating dedicated worktree and studying design file

## Implementation Learning Log - Session 1

Dioxus Component Patterns:

1. Icons are inlined as SVG - no Icon component with string name prop

2. Styles use inline style attribute with string values

3. Components use rsx! macro with HTML-like syntax

4. State uses use_signal hooks

5. Reference: builds.rs shows proper page-head structure

Approach: Start minimal PageHeader + FilterBar matching JSX exactly

Verify compilation after each component
<!-- SECTION:NOTES:END -->
