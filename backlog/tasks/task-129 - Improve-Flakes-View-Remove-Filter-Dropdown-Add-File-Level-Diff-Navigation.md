---
id: TASK-129
title: Improve Flakes View - Remove Filter Dropdown & Add File-Level Diff Navigation
status: In Progress
assignee: []
created_date: '2026-02-25 22:45'
updated_date: '2026-02-26 05:19'
labels:
  - ui
  - flakes-view
  - ux-improvement
dependencies:
  - TASK-124
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
### Current State

The flakes view currently has:
- A table/card view showing all flakes
- A separate "Git Commit History" section with:
  - A dropdown to select which flake's history to view
  - A timeline of commits for the selected flake
  - A diff viewer showing the full unified diff in continuous scroll

### Desired State

1. **Remove the flake filter dropdown** from the git history section
   - When viewing flakes in table or card mode, clicking on a flake should show its git history and diff
   - The git history should be contextual to the selected flake

2. **Add file-level diff navigation**
   - Show a list of files that were changed in the selected commit
   - Each file entry shows: filename, lines added, lines deleted
   - Clicking a file shows only that file's diff
   - Add ability to jump between changed files
   - Show file count summary (e.g., "5 files changed")

### Visual Mockup

```
┌─────────────────────────────────────────────────────────────────┐
│ Flake Registry                                                   │
│ [Search...] [Filter by env ▼] [Filter by commit ▼] [Filter by size ▼]  [+ Add Flake] │
├──────────────────────┬──────────────────────────────────────────┤
│ Flake Name │ Systems │ Latest Commit                            │
├──────────────────────┼──────────────────────────────────────────┤
│ production  │   12    │ abc1234  (2 hours ago)                  │
│ staging    │    5    │ def5678  (1 day ago)                     │
│ dev        │    3    │ ghi9012  (3 days ago)                    │
└──────────────────────┴──────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│ Git Commit History - production                                  │
├─────────────────────────────────────────────────────────────────┤
│ Timeline: ○──○──○──● (selected: abc1234)                       │
│ Committed: 2 hours ago by john@example.com                      │
├─────────────────────────────────────────────────────────────────┤
│ Files Changed (5):                                              │
│ [+] hosts/production/default.nix  (+12, -4)                   │
│ [+] modules/networking.nix          (+8, -2)                   │
│ [+] flake.nix                       (+2, -0)                    │
│ [+] .github/workflows/ci.yml       (+15, -1)                   │
│ [+] README.md                       (+3, -1)                   │
├─────────────────────────────────────────────────────────────────┤
│ ▼ hosts/production/default.nix (12 additions, 4 deletions)      │
├─────────────────────────────────────────────────────────────────┤
│ diff --git a/hosts/production/default.nix...                   │
│ @@ -18,8 +18,12 @@ in {                                        │
│    services.openssh.enable = true;                              │
│ -  services.openssh.settings.PasswordAuthentication = true;     │
│ +  services.openssh.settings.PasswordAuthentication = false;    │
│ +  services.openssh.settings.KbdInteractiveAuthentication =... │
│ +  services.openssh.ports = [ 22 2222 ];                       │
│                                                                    │
│    environment.systemPackages = with pkgs; [                    │
│      git                                                        │
│ +    htop                                                       │
│    ];                                                           │
└─────────────────────────────────────────────────────────────────┘
```

## Non-Goals

- Adding new backend APIs (the existing flake timeline API is sufficient)
- Changing how commits are stored or synced
- Adding diff syntax highlighting (already exists)
- Adding search within diffs

## Acceptance Criteria

1. **Flake Selection**
   - [ ] Clicking a flake in the table/card view navigates to its git history
   - [ ] The git history section title shows the selected flake name
   - [ ] The filter dropdown for flake selection is removed from the git history section
   - [ ] State is preserved when switching between table/card views

2. **File Navigation**
   - [ ] When a commit is selected, the diff shows a file list at the top
   - [ ] File list shows: filename, additions (+N), deletions (-N)
   - [ ] Clicking a file scrolls to/shows only that file's diff
   - [ ] Current file is highlighted in the file list
   - [ ] Can navigate between files using keyboard (arrow keys) or click

3. **Visual**
   - [ ] Summary shows "X files changed" with total additions/deletions
   - [ ] File list is collapsible/expandable
   - [ ] Empty states handled gracefully

4. **Behavior**
   - [ ] Works with real data from the database (not mock)
   - [ ] Falls back to mock data if API unavailable
   - [ ] Loading states shown while fetching diff

## Implementation Notes

- Reuse existing `fetch_commit_diff` API endpoint
- The diff parsing logic already exists in `parse_unified_diff`
- May need to add a component for the file list
- Consider using Dioxus signals for file selection state

## Technical Approach

1. Modify `FlakesListView` to track selected flake for history view
2. Update the FlakeHistoryExplorer component:
   - Remove the flake dropdown
   - Accept selected flake as prop instead
3. Create new `FileDiffList` component:
   - Parse diff to extract file list
   - Show file entries with stats
   - Handle file selection
4. Update diff viewer to filter by selected file
<!-- SECTION:DESCRIPTION:END -->

## Problem

The current flakes view has two UX issues:

1. **Filter dropdown for flake selection** - There's a dropdown in the git history section that lets you select which flake's history to view. This is awkward and not intuitive.

2. **Continuous scroll diff** - The git diff shows all files in one long continuous scroll, which makes it hard to navigate when many files are changed.

## Goal

Improve the flakes view by:
1. Making the flake selection intuitive (click flake in table/cards to see its git history)
2. Adding file-level navigation for diffs (show list of changed files, click to drill down)

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-sonnet-4-6 on gray in ~/code/crystal-forge/TASK-129-improve-flakes-view-file-level-diff
<!-- SECTION:NOTES:END -->
