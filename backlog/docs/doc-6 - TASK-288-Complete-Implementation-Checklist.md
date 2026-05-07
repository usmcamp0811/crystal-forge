---
id: doc-6
title: 'TASK-288: Complete Implementation Checklist'
type: specification
created_date: '2026-05-04 01:21'
tags:
  - task-288
  - implementation-guide
  - checklist
---
# Evaluations View Rebuild - Complete Implementation Checklist

This document contains the full detailed checklist for TASK-288: Rebuild Evaluations View to Match JSX Mockup Design Exactly.

## 15 Major Structural Differences

1. **Page layout** - Single-column vs. two-column split
2. **Header structure & buttons** - Different buttons and subtitle
3. **Stat strip position & content** - Top-level vs. sidebar, 5 stats vs. 3
4. **Tab bar styling & badge** - Card wrapper, badge count, classes
5. **Active queue** - HTML table vs. card list
6. **Active queue row content** - 8 columns vs. stacked card content
7. **Empty state presentation** - Styled container vs. plain text
8. **History filter bar** - Segmented control + select vs. chips + input
9. **History table structure** - Different column order and combination
10. **History row content** - Different cell content and styling
11. **Log modal vs. inline** - Modal-only vs. inline card + modal
12. **Icon usage** - Icon component vs. text/unicode
13. **CSS class system** - Design system vs. Tailwind
14. **Data display formatting** - Different structures and labels
15. **Interaction patterns** - Different click behaviors

---

## Section A: Page Structure & Layout

- [ ] Change outer container from `class: "space-y-6"` to inline `style="display:flex; flex-direction:column; gap:16px"`
- [ ] Remove `div { class: "cf-builds-split" }` two-column wrapper
- [ ] Make all content (header, stat strip, tabs, content) single-column in one container
- [ ] Remove right sidebar column entirely

## Section B: Page Header

- [ ] Replace `header` tag with `div className="page-head"`
- [ ] Inside page-head, create two child divs with flex justify-between
- [ ] First child: title block
  - [ ] Change h1 to use `className="page-title"` (remove Tailwind classes)
  - [ ] Remove MOCK MODE badge or keep only in dev mode
  - [ ] Replace subtitle with stats summary
  - [ ] Calculate and display: `{active} active · {completed} completed · {failed} failed`
  - [ ] Use `className="page-subtitle"` for subtitle
- [ ] Second child: action buttons
  - [ ] Wrap in `div style="display:flex; gap:8px"`
  - [ ] Add "Sync flakes" button with sync icon
  - [ ] Change "Refresh" to "Queue eval" with plus icon

## Section C: Stat Strip

- [ ] Create `<div className="stat-strip">` container after page header
- [ ] Set explicit grid: 5 equal columns
- [ ] Create Stat 1: Active (#60a5fa)
- [ ] Create Stat 2: Completed (#34d399)
- [ ] Create Stat 3: Failed (#f87171)
- [ ] Create Stat 4: Total (var(--cf-text-secondary))
- [ ] Create Stat 5: Flakes tracked (#a78bfa, value: 5)
- [ ] Each stat needs: .stat-accent, .stat-label, .stat-value
- [ ] Remove current MetricsStrip component from sidebar
- [ ] Update data fetching to provide failed count

## Section D: Tab Bar

- [ ] Wrap tab bar AND tab content in card with overflow:hidden
- [ ] Change tab container to use .sd-tabs class with padding
- [ ] Replace Tailwind button classes with .sd-tab .focus-ring
- [ ] Add badge to Active Queue tab with count
- [ ] Change "Eval History" to "History"

## Section E: Active Queue Table

### E1: Create table structure
- [ ] Delete all card-based active queue code
- [ ] Create table with .sys-table class
- [ ] Add thead with 8 column headers
- [ ] Add tbody for rows

### E2-E9: Table columns (each item below is a separate column)

- [ ] Queue position column: queuePos, width:40, muted, fontSize:12
- [ ] Flake·commit column: stacked bold flake + mono commit
- [ ] Branch column: chip .chip-unknown
- [ ] Status column: chip with .chip-dot and translated label
- [ ] Systems column: "{count} hosts"
- [ ] Policy column: pass/fail chips with ✓/✗
- [ ] Started column: relative time
- [ ] Actions column: ↑↓ buttons, terminal icon, Cancel buttons
- [ ] Remove Selected Commit, Completed Evaluations, inline Logs cards

## Section F: Empty State

- [ ] Use .empty class with margin:24
- [ ] Add h3 "No active evaluations"
- [ ] Add div "All flake evaluations are complete."

## Section G: History Filter Bar

- [ ] Add padding and border-bottom
- [ ] Replace chip buttons with .seg segmented control
- [ ] Replace text input with select.filter-select dropdown
- [ ] Add .filter-count showing entry count
- [ ] Remove "Status:" prefix label

## Section H: History Table

- [ ] Reorder columns: Flake·commit | Branch | Status | Systems | Policy | Duration | Completed | Actions
- [ ] Combine commit+flake into one stacked cell
- [ ] Change branch to chip
- [ ] Add colored dot to status chip
- [ ] Add NEW Policy column with pass/fail chips
- [ ] Simplify Systems to just count
- [ ] Ensure Duration positioned before Completed
- [ ] Replace actions with icon buttons (terminal + sync)

## Section I: Log Modal

- [ ] Remove inline "Evaluation Logs" card
- [ ] Add logTarget signal state
- [ ] Create .modal-backdrop with onClick close
- [ ] Create .modal panel (width:min(800px,98vw))
- [ ] Build modal header with status chip, flake, metadata
- [ ] Add .seg for Concise/Verbose toggle
- [ ] Add close btn-icon with x icon
- [ ] Create .sd-log-stream with structured log lines
- [ ] Parse logs into .sd-log-t, .sd-log-lvl, .sd-log-m
- [ ] Add blinking .sd-log-caret for in_progress
- [ ] Create modal footer with line count, Download, Close
- [ ] Implement auto-scroll to bottom

## Section J: Icons

- [ ] Create or import Icon component
- [ ] Add sync icon to "Sync flakes" button
- [ ] Add plus icon to "Queue eval" button
- [ ] Add terminal icon to "View logs" buttons
- [ ] Add x icon to close buttons
- [ ] Add sync icon to "Re-evaluate" buttons
- [ ] Add download icon to modal footer
- [ ] Keep ↑↓ as text characters

## Section K: CSS Classes Migration

### K1: Replace button classes
- [ ] Use .btn .btn-primary .btn-ghost .btn-danger .btn-icon .focus-ring
- [ ] Remove all Tailwind button utilities

### K2: Replace chip classes
- [ ] Use .chip .chip-dot .chip-healthy .chip-critical .chip-warning .chip-unknown .chip-info

### K3: Replace layout classes
- [ ] Use .page-head .page-title .page-subtitle .card .stat-strip .stat .stat-accent .stat-label .stat-value

### K4: Replace table classes
- [ ] Use .sys-table .row-actions

### K5: Replace modal classes
- [ ] Use .modal-backdrop .modal .modal-head .modal-foot

### K6: Replace log classes
- [ ] Use .sd-tabs .sd-tab .sd-tab-badge .sd-logs-controls .sd-log-stream .sd-log-line .sd-log-t .sd-log-lvl .sd-log-m .sd-log-caret

### K7: Replace filter classes
- [ ] Use .seg .input .filter-select .filter-count

### K8: Replace utility classes
- [ ] Use .mono .empty

## Section L: Data Models

- [ ] Add queuePos: i64 to active queue items
- [ ] Add policyPass: i64 to active queue items
- [ ] Add policyFail: i64 to active queue items
- [ ] Add startedAt: String to active queue items
- [ ] Add canCancel: bool to active queue items
- [ ] Add canForceCancel: bool to active queue items
- [ ] Add meta: StatusMeta {label, color, cls} to items
- [ ] Add policyPass: i64 to history items
- [ ] Add policyFail: i64 to history items
- [ ] Create status → StatusMeta mapping for all statuses
- [ ] Ensure stats include failed_count
- [ ] Add flakes_tracked count

## Section M: Behavior

- [ ] Implement onMove(id: i32, direction: i32) function
- [ ] Call reorder API
- [ ] Update local state optimistically
- [ ] Implement onLog(item: &EvalQueueItem) function
- [ ] Set logTarget signal to open modal
- [ ] Implement onCancel(id: i32, force: bool) function
- [ ] Call cancel API endpoint
- [ ] Track in-flight cancellations
- [ ] Delete all drag-and-drop handlers
- [ ] Remove click-to-select behavior from cards
- [ ] Remove selected_commit_id from active queue

## Section N: Typography

- [ ] Set main text to 13px (flake names, status labels)
- [ ] Set secondary text to 11px (commit hashes, chip text)
- [ ] Set timestamps to 12px
- [ ] Apply font-weight:600 to flake names
- [ ] Apply .mono class to commit hashes
- [ ] Apply .mono class to durations

## Section O: Pagination

- [ ] Verify pagination uses design system classes
- [ ] Verify pagination works with filters
- [ ] Visual verification

---

## Verification Checklist

- [ ] Side-by-side comparison with mockup
- [ ] Pixel-perfect spacing measurements
- [ ] Color accuracy with color picker
- [ ] All buttons functional
- [ ] All filters functional
- [ ] Modal opens and closes correctly
- [ ] Reorder up/down works
- [ ] Cancel/Force Cancel works
- [ ] Real-time logs stream correctly
- [ ] Empty state displays correctly
- [ ] Responsive layout works
- [ ] All icons display correctly
- [ ] Typography matches exactly
- [ ] All 15 structural differences resolved

---

## Risk Areas & Notes

1. **Icon component**: May need creation from scratch
2. **API data shape**: Backend may need updates for new fields
3. **CSS availability**: Verify all classes exist in styles.css
4. **WebSocket integration**: Must work with new modal structure
5. **Mobile responsive**: Table may need special handling

**Estimated effort**: ~24 hours total
