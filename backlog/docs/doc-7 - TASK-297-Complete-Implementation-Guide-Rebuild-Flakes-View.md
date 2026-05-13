---
id: doc-7
title: TASK-297 Complete Implementation Guide - Rebuild Flakes View
type: guide
created_date: '2026-05-13 03:07'
tags:
  - implementation-guide
  - flakes-view
  - ui-redesign
  - task-297
---
# TASK-297 Complete Implementation Guide: Rebuild Flakes View

**Status:** Draft Implementation Guide  
**Task:** TASK-297 - Rebuild Flakes View to Match JSX Design Mockup Exactly  
**Design Reference:** `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx` (567 lines)  
**CSS Reference:** `/home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css`  
**Estimated Effort:** 12-16 hours

---

## Table of Contents

1. [Overview & Architecture](#overview--architecture)
2. [Design Analysis](#design-analysis)
3. [Component Breakdown](#component-breakdown)
4. [Implementation Phases](#implementation-phases)
5. [Phase 1: Main View Structure](#phase-1-main-view-structure)
6. [Phase 2: Side Tray Foundation](#phase-2-side-tray-foundation)
7. [Phase 3: Commit Timeline](#phase-3-commit-timeline)
8. [Phase 4: Commit Detail Panel](#phase-4-commit-detail-panel)
9. [Phase 5: Pipeline Components](#phase-5-pipeline-components)
10. [Phase 6: Diff Modal](#phase-6-diff-modal)
11. [Phase 7: Table & Cards Views](#phase-7-table--cards-views)
12. [Phase 8: Integration & Testing](#phase-8-integration--testing)
13. [CSS Classes Reference](#css-classes-reference)
14. [Data Models](#data-models)
15. [API Requirements](#api-requirements)
16. [Testing Checklist](#testing-checklist)

---

## Overview & Architecture

### Current State
- Basic flake list view in `packages/web-ui/src/views/flakes_list.rs`
- Timeline cards displayed in main view
- Limited commit history
- No file-level diff viewer
- Performance issues (10s+ load times)

### Target State
- Dual view modes (Table/Cards) with segmented toggle
- Side tray commit explorer (slides in from right)
- Time-bucketed commit timeline with visual rail
- File diff viewer in modal
- Pipeline status visualization (Eval → Build → Rollout)
- Complete design system alignment

### File Structure

```
packages/web-ui/src/
├── views/
│   └── flakes_list.rs              ← Complete rewrite
├── components/
│   ├── flake_tray.rs               ← NEW: Side tray component
│   ├── pipeline_status.rs          ← NEW: Pipeline pills/dots/arrows
│   ├── diff_modal.rs               ← NEW: File diff viewer modal
│   └── mod.rs                      ← Update exports
└── api/
    └── models.rs                   ← Add new data models
```

---

## Design Analysis

### Main Components from FlakesView.jsx

1. **FlakesView** (main container)
   - State: viewMode, query, trayFlake, addOpen
   - Page header with stats subtitle
   - Filter bar with search + toggle
   - Conditional rendering: Table or Cards
   - Side tray (when flake selected)

2. **FlakeTray** (side panel)
   - State: selCommit, selFile, commitQuery
   - Two-pane layout: commits list (left) + detail (right)
   - Time bucketing: Today / This week / Earlier
   - ESC key to close

3. **DiffModal** (full-screen overlay)
   - State: activeHunk, wrap
   - Keyboard navigation: j/k, Esc
   - Line-by-line diff rendering
   - Hunk navigation

4. **Pipeline Components**
   - PipelineDot: tiny status indicator (14x14px)
   - PipelinePill: larger chip with label
   - PipelineArrow: separator (→)
   - RolloutPill: deployed count with progress bar

5. **Table & Cards**
   - FlakeTable: 8 columns, sys-table class
   - FlakeCards: card grid with status rail

### Key Interactions

- **Flake selection** → Opens side tray
- **ESC key** → Closes tray
- **Backdrop click** → Closes tray
- **Commit click** → Updates detail pane
- **File card click** → Opens diff modal
- **ESC in modal** → Closes modal
- **j/k in modal** → Navigate hunks

---

## Component Breakdown

### 1. FlakesListView (Main Container)

**Responsibility:** Top-level view orchestration

**State:**
```rust
struct FlakesListState {
    view_mode: ViewMode,           // Table or Cards
    search_query: String,
    selected_flake: Option<Flake>,
    show_add_modal: bool,
    flakes: Vec<FlakeListItem>,
    loading: bool,
}
```

**Render Structure:**
```
<div style="display:flex; flex-direction:column; gap:16px">
  <PageHeader />
  <FilterBar />
  {if view_mode == Table then <FlakeTable /> else <FlakeCards />}
  {if selected_flake.is_some() then <FlakeTray />}
  {if show_add_modal then <AddFlakeModal />}
</div>
```

### 2. FlakeTray Component

**Responsibility:** Side panel with commit history and detail

**State:**
```rust
struct FlakeTrayState {
    selected_commit: Option<CommitInfo>,
    selected_file: Option<FileChange>,
    commit_query: String,
}
```

**Render Structure:**
```
<>
  <div class="fl-tray-backdrop" onclick={close} />
  <aside class="fl-tray">
    <header class="fl-tray-head">...</header>
    <div class="fl-tray-body">
      <nav class="fl-tray-commits">
        <CommitTimeline />
      </nav>
      <section class="fl-tray-detail">
        <CommitDetail />
      </section>
    </div>
  </aside>
  {if selected_file then <DiffModal />}
</>
```

### 3. DiffModal Component

**Responsibility:** Full-screen diff viewer

**State:**
```rust
struct DiffModalState {
    active_hunk: usize,
    wrap_lines: bool,
}
```

**Features:**
- Parse unified diff format
- Render line-by-line with old/new line numbers
- Keyboard shortcuts (j/k/Esc)
- Hunk navigation with scroll tracking
- Syntax highlighting (optional, can be added later)

---

## Implementation Phases

### Phase Timeline

| Phase | Component | Estimated Time | Dependencies |
|-------|-----------|----------------|--------------|
| 1 | Main View Structure | 2-3 hours | None |
| 2 | Side Tray Foundation | 2-3 hours | Phase 1 |
| 3 | Commit Timeline | 3-4 hours | Phase 2 |
| 4 | Commit Detail Panel | 2-3 hours | Phase 3 |
| 5 | Pipeline Components | 2-3 hours | Phase 4 |
| 6 | Diff Modal | 2-3 hours | Phase 2 |
| 7 | Table & Cards Views | 1-2 hours | Phase 1 |
| 8 | Integration & Testing | 2-3 hours | All phases |

**Total:** 16-24 hours (conservative estimate)

---

## Phase 1: Main View Structure

### Goal
Implement the main view container with page header, filter bar, and view mode toggle.

### Files to Create/Modify
- `packages/web-ui/src/views/flakes_list.rs`

### Implementation Steps

#### Step 1.1: Page Header

**Design Reference:** Lines 24-39 in FlakesView.jsx

```rust
fn PageHeader(
    flake_count: usize,
    total_systems: usize,
    synced_count: usize,
    on_sync_all: EventHandler<()>,
    on_add_flake: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "page-head",
            div {
                h1 { class: "page-title", "Flakes" }
                p { class: "page-subtitle",
                    "{flake_count} tracked · {total_systems} systems · {synced_count} synced"
                }
            }
            div { style: "display:flex; gap:8px",
                button {
                    class: "btn btn-ghost focus-ring",
                    onclick: move |_| on_sync_all.call(()),
                    Icon { name: "sync", size: 14 }
                    " Sync all"
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| on_add_flake.call(()),
                    Icon { name: "plus", size: 14 }
                    " Add flake"
                }
            }
        }
    }
}
```

**Key Details:**
- Stats subtitle format: `{count} tracked · {systems} systems · {synced} synced`
- Two buttons: "Sync all" (ghost), "Add flake" (primary)
- Icons: sync (14px), plus (14px)

#### Step 1.2: Filter Bar

**Design Reference:** Lines 42-52 in FlakesView.jsx

```rust
#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Table,
    Cards,
}

fn FilterBar(
    search_query: String,
    on_search: EventHandler<String>,
    view_mode: ViewMode,
    on_view_mode: EventHandler<ViewMode>,
    result_count: usize,
) -> Element {
    rsx! {
        div { class: "filterbar",
            div { class: "filter-search",
                Icon { name: "search" }
                input {
                    class: "input focus-ring",
                    placeholder: "Search flakes…",
                    value: "{search_query}",
                    oninput: move |evt| on_search.call(evt.value().clone())
                }
            }
            div { class: "seg",
                button {
                    class: if view_mode == ViewMode::Table { "active" } else { "" },
                    onclick: move |_| on_view_mode.call(ViewMode::Table),
                    Icon { name: "rows", size: 12 }
                    " Table"
                }
                button {
                    class: if view_mode == ViewMode::Cards { "active" } else { "" },
                    onclick: move |_| on_view_mode.call(ViewMode::Cards),
                    Icon { name: "grid", size: 12 }
                    " Cards"
                }
            }
            span { class: "filter-count", "{result_count} flakes" }
        }
    }
}
```

**Key Details:**
- `.filterbar` contains three sections: search, toggle, count
- `.filter-search` has icon + input
- `.seg` is the segmented control (active class on selected button)
- Icons: search (default), rows (12px), grid (12px)

#### Step 1.3: Main View Component

```rust
#[component]
pub fn FlakesListView() -> Element {
    let mut view_mode = use_signal(|| ViewMode::Table);
    let mut search_query = use_signal(String::new);
    let mut selected_flake = use_signal(|| None::<FlakeListItem>);
    let mut show_add_modal = use_signal(|| false);
    
    // Fetch flakes from API
    let flakes = use_resource(move || async move {
        // TODO: Fetch from API
        Vec::new()
    });
    
    let filtered_flakes = use_memo(move || {
        let query = search_query.read().to_lowercase();
        flakes.read().as_ref().map(|f| {
            f.iter()
                .filter(|flake| {
                    query.is_empty()
                        || flake.name.to_lowercase().contains(&query)
                        || flake.description.as_ref().map_or(false, |d| d.to_lowercase().contains(&query))
                })
                .cloned()
                .collect::<Vec<_>>()
        }).unwrap_or_default()
    });
    
    // ESC key handler for tray
    use_effect(move || {
        if selected_flake.read().is_some() {
            let handler = move |evt: Event<KeyboardData>| {
                if evt.key() == Key::Escape {
                    selected_flake.set(None);
                }
            };
            // TODO: Register global keydown listener
        }
    });
    
    let flake_count = filtered_flakes.len();
    let total_systems = filtered_flakes.iter().map(|f| f.system_count).sum::<i32>() as usize;
    let synced_count = filtered_flakes.iter().filter(|f| f.status == "synced").count();
    
    rsx! {
        div { style: "display:flex; flex-direction:column; gap:16px",
            PageHeader {
                flake_count,
                total_systems,
                synced_count,
                on_sync_all: move |_| {
                    // TODO: Trigger sync all
                },
                on_add_flake: move |_| show_add_modal.set(true),
            }
            
            FilterBar {
                search_query: search_query.read().clone(),
                on_search: move |q| search_query.set(q),
                view_mode: view_mode.read().clone(),
                on_view_mode: move |mode| view_mode.set(mode),
                result_count: flake_count,
            }
            
            match view_mode.read().clone() {
                ViewMode::Table => rsx! {
                    FlakeTable {
                        flakes: filtered_flakes.clone(),
                        selected: selected_flake.read().clone(),
                        on_select: move |f| selected_flake.set(Some(f)),
                    }
                },
                ViewMode::Cards => rsx! {
                    FlakeCards {
                        flakes: filtered_flakes.clone(),
                        selected: selected_flake.read().clone(),
                        on_select: move |f| selected_flake.set(Some(f)),
                    }
                }
            }
            
            if let Some(flake) = selected_flake.read().clone() {
                rsx! {
                    FlakeTray {
                        flake,
                        on_close: move |_| selected_flake.set(None),
                    }
                }
            }
            
            if show_add_modal.read().clone() {
                rsx! {
                    AddFlakeModal {
                        on_close: move |_| show_add_modal.set(false),
                    }
                }
            }
        }
    }
}
```

### Verification for Phase 1
- [ ] Page header displays with correct stats format
- [ ] Sync all and Add flake buttons render with icons
- [ ] Filter bar search input works
- [ ] Table/Cards toggle switches view mode
- [ ] Result count updates based on search
- [ ] View compiles without errors

---

## Phase 2: Side Tray Foundation

### Goal
Create the side tray container with backdrop, header, and two-pane layout.

### Files to Create
- `packages/web-ui/src/components/flake_tray.rs`

### Implementation Steps

#### Step 2.1: Tray Container Structure

**Design Reference:** Lines 113-262 in FlakesView.jsx

```rust
// packages/web-ui/src/components/flake_tray.rs

use dioxus::prelude::*;
use crate::api::models::*;
use crate::components::Icon;

#[component]
pub fn FlakeTray(
    flake: FlakeListItem,
    on_close: EventHandler<()>,
) -> Element {
    let mut selected_commit = use_signal(|| None::<CommitInfo>);
    let mut selected_file = use_signal(|| None::<FileChange>);
    let mut commit_query = use_signal(String::new);
    
    // Fetch commits for this flake
    let commits = use_resource(move || {
        let flake_id = flake.id;
        async move {
            // TODO: Fetch commits from API
            Vec::new()
        }
    });
    
    // ESC key handler
    use_effect(move || {
        let handler = move |evt: Event<KeyboardData>| {
            if evt.key() == Key::Escape {
                on_close.call(());
            }
        };
        // TODO: Register keydown listener
    });
    
    rsx! {
        // Backdrop
        div {
            class: "fl-tray-backdrop",
            onclick: move |_| on_close.call(()),
        }
        
        // Tray panel
        aside {
            class: "fl-tray",
            role: "dialog",
            "aria-label": "{flake.name} commits",
            
            TrayHeader { flake: flake.clone(), on_close }
            
            div { class: "fl-tray-body",
                CommitsList {
                    flake: flake.clone(),
                    commits: commits.read().clone().unwrap_or_default(),
                    selected_commit: selected_commit.read().clone(),
                    commit_query: commit_query.read().clone(),
                    on_commit_query: move |q| commit_query.set(q),
                    on_select: move |c| selected_commit.set(Some(c)),
                }
                
                CommitDetail {
                    flake: flake.clone(),
                    commit: selected_commit.read().clone(),
                    on_file_select: move |f| selected_file.set(Some(f)),
                }
            }
        }
        
        // Diff modal (if file selected)
        if let Some(file) = selected_file.read().clone() {
            if let Some(commit) = selected_commit.read().clone() {
                rsx! {
                    DiffModal {
                        file,
                        commit,
                        flake: flake.clone(),
                        on_close: move |_| selected_file.set(None),
                    }
                }
            }
        }
    }
}
```

#### Step 2.2: Tray Header

**Design Reference:** Lines 118-134 in FlakesView.jsx

```rust
#[component]
fn TrayHeader(
    flake: FlakeListItem,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        header { class: "fl-tray-head",
            div {
                style: "display:flex; align-items:center; gap:10px; min-width:0; flex:1",
                Icon {
                    name: "git",
                    size: 18,
                    style: "color:var(--cf-brand-purple); flex-shrink:0"
                }
                div { style: "min-width:0",
                    div { style: "display:flex; align-items:center; gap:8px",
                        span { style: "font-weight:700; font-size:15px",
                            "{flake.name}"
                        }
                        span { class: "chip chip-unknown", style: "font-size:10px",
                            "{flake.branch}"
                        }
                        FlakeSyncChip { flake: flake.clone() }
                    }
                    div {
                        class: "mono",
                        style: "font-size:11px; color:var(--cf-text-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap",
                        "{flake.url}"
                    }
                }
            }
            div { style: "display:flex; gap:6px; align-items:center",
                button {
                    class: "btn btn-ghost focus-ring xs",
                    onclick: move |_| {
                        // TODO: Trigger sync for this flake
                    },
                    Icon { name: "sync", size: 11 }
                    " Sync"
                }
                button {
                    class: "btn-icon focus-ring",
                    onclick: move |_| on_close.call(()),
                    "aria-label": "Close",
                    Icon { name: "x", size: 16 }
                }
            }
        }
    }
}
```

**Key Details:**
- Git icon (18px, purple) on left
- Flake name (bold, 15px) + branch chip (10px) + sync status chip
- URL in mono font (11px, muted, ellipsis overflow)
- Sync button (xs size, ghost style) + Close button (icon only)

#### Step 2.3: Two-Pane Body Layout

**CSS Classes:**
```css
.fl-tray-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 280px 1fr;
}
```

This creates a fixed 280px left pane (commit list) and flexible right pane (detail).

### Verification for Phase 2
- [ ] Tray slides in from right when flake selected
- [ ] Backdrop appears with correct opacity
- [ ] Clicking backdrop closes tray
- [ ] ESC key closes tray
- [ ] Tray header displays flake info correctly
- [ ] Two-pane layout renders with correct proportions
- [ ] Close button works

---

## Phase 3: Commit Timeline

### Goal
Implement the left pane of the tray: commit list with search, time buckets, and timeline rail.

### Implementation Steps

#### Step 3.1: Time Bucketing Logic

**Design Reference:** Lines 84-95 in FlakesView.jsx

```rust
fn bucket_commits_by_time(commits: &[CommitInfo]) -> Vec<(&str, Vec<CommitInfo>)> {
    let mut today = Vec::new();
    let mut this_week = Vec::new();
    let mut earlier = Vec::new();
    
    for commit in commits {
        let time_str = format_relative_time(commit.timestamp);
        let time_lower = time_str.to_lowercase();
        
        if time_lower.contains("h ago")
            || time_lower.contains("now")
            || time_lower.contains("min ago")
        {
            today.push(commit.clone());
        } else if time_lower.starts_with("1d ago")
            || time_lower.starts_with("2d ago")
            || time_lower.starts_with("3d ago")
            || time_lower.starts_with("4d ago")
            || time_lower.starts_with("5d ago")
            || time_lower.starts_with("6d ago")
        {
            this_week.push(commit.clone());
        } else {
            earlier.push(commit.clone());
        }
    }
    
    let mut result = Vec::new();
    if !today.is_empty() {
        result.push(("Today", today));
    }
    if !this_week.is_empty() {
        result.push(("This week", this_week));
    }
    if !earlier.is_empty() {
        result.push(("Earlier", earlier));
    }
    
    result
}

fn format_relative_time(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);
    
    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_weeks() < 4 {
        format!("{}w ago", duration.num_weeks())
    } else {
        format!("{}mo ago", duration.num_days() / 30)
    }
}
```

#### Step 3.2: Commit Search Filter

**Design Reference:** Lines 140-150 in FlakesView.jsx

```rust
#[component]
fn CommitSearchBar(
    query: String,
    on_query: EventHandler<String>,
    filtered_count: usize,
    total_count: usize,
) -> Element {
    rsx! {
        div { class: "fl-tray-commits-search",
            Icon {
                name: "search",
                size: 12,
                style: "color:var(--cf-text-muted); flex-shrink:0"
            }
            input {
                class: "input focus-ring",
                placeholder: "Filter commits…",
                value: "{query}",
                oninput: move |evt| on_query.call(evt.value().clone()),
                style: "background:transparent; border:none; padding:4px 0; font-size:12px; flex:1"
            }
            span {
                style: "font-size:10px; color:var(--cf-text-muted)",
                "{filtered_count}/{total_count}"
            }
        }
    }
}
```

#### Step 3.3: Commit Timeline Rail

**Design Reference:** Lines 151-189 in FlakesView.jsx

```rust
#[component]
fn CommitsList(
    flake: FlakeListItem,
    commits: Vec<CommitInfo>,
    selected_commit: Option<CommitInfo>,
    commit_query: String,
    on_commit_query: EventHandler<String>,
    on_select: EventHandler<CommitInfo>,
) -> Element {
    // Filter commits
    let filtered_commits = use_memo(move || {
        if commit_query.is_empty() {
            commits.clone()
        } else {
            let query_lower = commit_query.to_lowercase();
            commits
                .iter()
                .filter(|c| {
                    c.message.to_lowercase().contains(&query_lower)
                        || c.sha.to_lowercase().contains(&query_lower)
                        || c.author.to_lowercase().contains(&query_lower)
                })
                .cloned()
                .collect()
        }
    });
    
    let commit_groups = use_memo(move || {
        bucket_commits_by_time(&filtered_commits)
    });
    
    rsx! {
        nav { class: "fl-tray-commits",
            CommitSearchBar {
                query: commit_query.clone(),
                on_query: on_commit_query,
                filtered_count: filtered_commits.len(),
                total_count: commits.len(),
            }
            
            for (bucket_name, bucket_commits) in commit_groups.iter() {
                div { key: "{bucket_name}",
                    div { class: "fl-commits-bucket", "{bucket_name}" }
                    
                    for (idx, commit) in bucket_commits.iter().enumerate() {
                        {
                            let is_selected = selected_commit.as_ref().map_or(false, |s| s.sha == commit.sha);
                            let is_last_in_bucket = idx == bucket_commits.len() - 1;
                            let is_last_bucket = bucket_name == commit_groups.last().map(|(n, _)| *n).unwrap_or("");
                            
                            rsx! {
                                CommitItem {
                                    key: "{commit.sha}",
                                    commit: commit.clone(),
                                    is_selected,
                                    show_stem: !(is_last_in_bucket && is_last_bucket),
                                    on_select: move |c| on_select.call(c),
                                }
                            }
                        }
                    }
                }
            }
            
            if filtered_commits.is_empty() {
                div { class: "empty", style: "margin:24px",
                    "No commits match."
                }
            }
        }
    }
}
```

#### Step 3.4: Commit Item with Rail

**Design Reference:** Lines 161-182 in FlakesView.jsx

```rust
#[component]
fn CommitItem(
    commit: CommitInfo,
    is_selected: bool,
    show_stem: bool,
    on_select: EventHandler<CommitInfo>,
) -> Element {
    let active_class = if is_selected { " active" } else { "" };
    let sel_class = if is_selected { " sel" } else { "" };
    let sha_color = if is_selected {
        "var(--cf-brand-purple)"
    } else {
        "var(--cf-text-primary)"
    };
    
    rsx! {
        div {
            class: "fl-commit-item{active_class}",
            onclick: move |_| on_select.call(commit.clone()),
            
            // Timeline rail
            div { class: "fl-rail",
                div { class: "fl-dot{sel_class}" }
                if show_stem {
                    div { class: "fl-stem" }
                }
            }
            
            // Commit info
            div { style: "min-width:0; flex:1",
                // SHA and timestamp
                div { style: "display:flex; align-items:baseline; gap:6px",
                    span {
                        class: "mono",
                        style: "font-size:11px; font-weight:700; color:{sha_color}",
                        "{commit.sha}"
                    }
                    span {
                        style: "font-size:11px; color:var(--cf-text-muted); margin-left:auto",
                        "{format_relative_time(commit.timestamp)}"
                    }
                }
                
                // Commit message
                div {
                    class: "truncate",
                    style: "font-size:12px; margin-top:3px; color:var(--cf-text-primary)",
                    "{commit.message}"
                }
                
                // Pipeline dots and author
                div { style: "display:flex; gap:5px; margin-top:6px; flex-wrap:wrap",
                    if let Some(eval_status) = &commit.eval_status {
                        PipelineDot { kind: "eval", val: eval_status.clone() }
                    }
                    if let Some(build_status) = &commit.build_status {
                        PipelineDot { kind: "build", val: build_status.clone() }
                    }
                    span {
                        class: "mono",
                        style: "font-size:10px; color:var(--cf-text-muted); margin-left:auto",
                        "{commit.author}"
                    }
                }
            }
        }
    }
}
```

**Key Details:**
- `.fl-rail` contains `.fl-dot` (always) and `.fl-stem` (conditionally)
- `.fl-dot` gets `.sel` class when commit is selected
- SHA color changes to purple when selected
- Pipeline dots show eval/build status
- Author in mono font on the right

### Verification for Phase 3
- [ ] Commit search filters correctly
- [ ] Result count shows filtered/total
- [ ] Commits grouped into Today/This week/Earlier
- [ ] Timeline rail renders with dots and stems
- [ ] Stems hidden on last commit
- [ ] Selected commit highlights in purple
- [ ] Click commit updates selection
- [ ] Pipeline dots show correct status colors

---

## Phase 4: Commit Detail Panel

### Goal
Implement the right pane of the tray: commit header with pipeline strip and files grid.

### Implementation Steps

#### Step 4.1: Commit Detail Container

**Design Reference:** Lines 192-260 in FlakesView.jsx

```rust
#[component]
fn CommitDetail(
    flake: FlakeListItem,
    commit: Option<CommitInfo>,
    on_file_select: EventHandler<FileChange>,
) -> Element {
    // Fetch files for selected commit
    let commit_files = use_resource(move || {
        if let Some(c) = &commit {
            let sha = c.sha.clone();
            async move {
                // TODO: Fetch file changes from API
                Vec::new()
            }
        } else {
            async move { Vec::new() }
        }
    });
    
    rsx! {
        section { class: "fl-tray-detail",
            if let Some(c) = commit {
                rsx! {
                    CommitHeader { commit: c.clone(), flake: flake.clone() }
                    
                    FilesChangedSection {
                        commit: c.clone(),
                        files: commit_files.read().clone().unwrap_or_default(),
                        on_file_select,
                    }
                }
            } else {
                div { class: "empty", style: "margin:32px",
                    "No commits yet for this flake."
                }
            }
        }
    }
}
```

#### Step 4.2: Commit Header with Stats

**Design Reference:** Lines 196-217 in FlakesView.jsx

```rust
#[component]
fn CommitHeader(
    commit: CommitInfo,
    flake: FlakeListItem,
) -> Element {
    // Mock pipeline status based on commit index (should come from API)
    let eval_status = commit.eval_status.unwrap_or("pending".to_string());
    let build_status = commit.build_status.unwrap_or("pending".to_string());
    let rollout_on = commit.rollout_count.unwrap_or(0);
    let rollout_total = flake.system_count;
    
    rsx! {
        div { class: "fl-tray-commit-h",
            // SHA and message
            div { style: "display:flex; align-items:baseline; gap:10px; flex-wrap:wrap",
                span {
                    class: "mono",
                    style: "font-size:14px; font-weight:700; color:var(--cf-brand-purple)",
                    "{commit.sha}"
                }
                span { style: "font-size:14px; font-weight:600",
                    "{commit.message}"
                }
            }
            
            // Metadata row
            div {
                style: "display:flex; gap:12px; margin-top:6px; font-size:11px; color:var(--cf-text-muted); flex-wrap:wrap",
                span {
                    Icon { name: "user", size: 11 }
                    span { class: "mono", " {commit.author}" }
                }
                span { "{format_relative_time(commit.timestamp)}" }
                span { style: "color:#34d399", "+{commit.additions}" }
                span { style: "color:#f87171", "-{commit.deletions}" }
                span { "{commit.files_changed} files" }
            }
            
            // Pipeline strip
            div { class: "fl-pipeline",
                PipelinePill { stage: "eval", val: eval_status.clone() }
                PipelineArrow {}
                PipelinePill { stage: "build", val: build_status.clone() }
                PipelineArrow {}
                RolloutPill {
                    on: rollout_on,
                    total: rollout_total,
                    failed: 0, // TODO: Calculate from pipeline status
                }
            }
        }
    }
}
```

**Key Details:**
- SHA (14px, bold, purple) + message (14px, bold)
- Metadata: user icon + author (mono) | timestamp | +add (green) | -del (red) | files count
- Pipeline strip below with Eval → Build → Rollout

#### Step 4.3: Files Changed Grid

**Design Reference:** Lines 220-255 in FlakesView.jsx

```rust
#[component]
fn FilesChangedSection(
    commit: CommitInfo,
    files: Vec<FileChange>,
    on_file_select: EventHandler<FileChange>,
) -> Element {
    rsx! {
        div { class: "fl-files-section",
            // Section header
            div { class: "fl-tray-section-h",
                span {
                    "{files.len()} files changed · click to view diff"
                }
                span {
                    style: "color:var(--cf-text-muted); font-weight:400; font-size:10px",
                    span { style: "color:#34d399", "+{commit.additions}" }
                    " / "
                    span { style: "color:#f87171", "-{commit.deletions}" }
                }
            }
            
            // Files grid
            div { class: "fl-files-grid",
                for file in files {
                    FileCard {
                        key: "{file.filename}",
                        file: file.clone(),
                        on_select: move |f| on_file_select.call(f),
                    }
                }
            }
        }
    }
}
```

#### Step 4.4: File Card Component

**Design Reference:** Lines 232-252 in FlakesView.jsx

```rust
#[component]
fn FileCard(
    file: FileChange,
    on_select: EventHandler<FileChange>,
) -> Element {
    let total = file.additions + file.deletions + 1; // +1 to avoid divide by zero
    let add_pct = (file.additions as f32 / total as f32 * 100.0).round() as u32;
    let del_pct = (file.deletions as f32 / total as f32 * 100.0).round() as u32;
    
    let filename = file.filename.split('/').last().unwrap_or(&file.filename);
    let path = file.filename.rsplitn(2, '/').nth(1).unwrap_or(".");
    
    rsx! {
        button {
            class: "fl-file-card focus-ring",
            onclick: move |_| on_select.call(file.clone()),
            
            // File name and path
            div { class: "fl-file-card-head",
                Icon { name: "file", size: 13, style: "opacity:0.55; flex-shrink:0" }
                div { style: "min-width:0; flex:1",
                    div {
                        class: "fl-file-name truncate",
                        title: "{file.filename}",
                        "{filename}"
                    }
                    div {
                        class: "fl-file-path truncate",
                        title: "{file.filename}",
                        "{path}"
                    }
                }
            }
            
            // Stats and bar
            div { class: "fl-file-stats",
                span {
                    class: "mono",
                    style: "font-size:11px; color:#34d399",
                    "+{file.additions}"
                }
                span {
                    class: "mono",
                    style: "font-size:11px; color:#f87171",
                    "-{file.deletions}"
                }
                div { class: "fl-file-bar",
                    div {
                        style: "width:{add_pct}%; height:100%; background:#34d399; display:inline-block; vertical-align:top"
                    }
                    div {
                        style: "width:{del_pct}%; height:100%; background:#f87171; display:inline-block; vertical-align:top"
                    }
                }
            }
        }
    }
}
```

**Key Details:**
- `.fl-file-card` is a button (clickable)
- File icon (13px, 0.55 opacity)
- Filename truncated, path below in muted color
- Stats: +add (green) | -del (red) | visual bar
- Bar uses inline divs with percentage widths

### Verification for Phase 4
- [ ] Commit header shows SHA, message, author, timestamp
- [ ] Stats display correctly (+/- in correct colors)
- [ ] Pipeline strip renders (placeholder for Phase 5)
- [ ] Files changed section shows count
- [ ] File cards display in grid
- [ ] Filename and path truncate correctly
- [ ] Add/del stats and bar show proportions
- [ ] Clicking file card triggers on_file_select

---

## Phase 5: Pipeline Components

### Goal
Implement the pipeline status visualization components.

### Files to Create
- `packages/web-ui/src/components/pipeline_status.rs`

### Implementation Steps

#### Step 5.1: PipelineDot (Tiny Status Indicator)

**Design Reference:** Lines 396-414 in FlakesView.jsx

```rust
// packages/web-ui/src/components/pipeline_status.rs

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum PipelineKind {
    Eval,
    Build,
}

#[component]
pub fn PipelineDot(
    kind: PipelineKind,
    val: String,
) -> Element {
    if val.is_empty() {
        return None;
    }
    
    let color = match val.as_str() {
        "complete" | "cache-pushed" | "up-to-date" => "#34d399",
        "building" | "pending" | "in_progress" => "#60a5fa",
        "failed" => "#f87171",
        "behind" => "#f59e0b",
        _ => "#6b7280",
    };
    
    let label = match kind {
        PipelineKind::Eval => "E",
        PipelineKind::Build => "B",
    };
    
    rsx! {
        span {
            title: "{kind:?}: {val}",
            style: "display:inline-flex; align-items:center; justify-content:center; width:14px; height:14px; border-radius:4px; font-size:9px; font-weight:700; color:{color}; background:color-mix(in oklab, {color} 15%, transparent); font-family:var(--font-mono)",
            "{label}"
        }
    }
}
```

**Key Details:**
- 14x14px square with 4px border radius
- Color mapping for status values
- Label: E for eval, B for build
- Background uses color-mix for 15% opacity
- Mono font

#### Step 5.2: PipelinePill (Larger Status Chip)

**Design Reference:** Lines 417-424 in FlakesView.jsx

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum PipelineStage {
    Eval,
    Build,
}

#[component]
pub fn PipelinePill(
    stage: PipelineStage,
    val: String,
) -> Element {
    let (chip_class, label) = match (stage, val.as_str()) {
        (PipelineStage::Eval, "complete") => ("chip-healthy", "Eval ✓"),
        (PipelineStage::Eval, "pending") => ("chip-info", "Eval…"),
        (PipelineStage::Eval, "failed") => ("chip-critical", "Eval ✗"),
        
        (PipelineStage::Build, "cache-pushed") => ("chip-healthy", "Cached"),
        (PipelineStage::Build, "complete") => ("chip-healthy", "Built"),
        (PipelineStage::Build, "building") => ("chip-info", "Building"),
        (PipelineStage::Build, "failed") => ("chip-critical", "Build ✗"),
        (PipelineStage::Build, "pending") => ("chip-unknown", "Queued"),
        
        _ => ("chip-unknown", &val),
    };
    
    rsx! {
        span {
            class: "chip {chip_class}",
            style: "font-weight:600",
            "{label}"
        }
    }
}
```

**Key Details:**
- Uses existing `.chip` classes with status variants
- Specific labels for each stage/status combination
- Font weight 600

#### Step 5.3: PipelineArrow (Separator)

**Design Reference:** Lines 426-428 in FlakesView.jsx

```rust
#[component]
pub fn PipelineArrow() -> Element {
    rsx! {
        span {
            style: "color:var(--cf-text-muted); font-size:11px",
            "→"
        }
    }
}
```

#### Step 5.4: RolloutPill (Deployment Status)

**Design Reference:** Lines 431-443 in FlakesView.jsx

```rust
#[component]
pub fn RolloutPill(
    on: i32,
    total: i32,
    failed: i32,
) -> Element {
    let pct = if total > 0 {
        (on as f32 / total as f32 * 100.0).round() as u32
    } else {
        0
    };
    
    let chip_class = if failed > 0 {
        "chip-critical"
    } else if pct == 100 {
        "chip-healthy"
    } else if pct == 0 {
        "chip-unknown"
    } else {
        "chip-warning"
    };
    
    rsx! {
        span {
            class: "chip {chip_class}",
            style: "display:inline-flex; align-items:center; gap:6px; font-weight:600",
            Icon { name: "server", size: 10 }
            "Rollout {on}/{total}"
            div {
                style: "width:32px; height:3px; background:rgba(255,255,255,0.2); border-radius:99px; overflow:hidden",
                div {
                    style: "width:{pct}%; height:100%; background:currentColor"
                }
            }
        }
    }
}
```

**Key Details:**
- Server icon (10px)
- Text: "Rollout N/M"
- Progress bar: 32px wide, 3px tall, rounded
- Background is 20% white, fill uses currentColor (inherits from chip)
- Chip class based on completion status

#### Step 5.5: FlakeSyncChip (Status Indicator)

**Design Reference:** Lines 445-448 in FlakesView.jsx

```rust
#[component]
pub fn FlakeSyncChip(
    flake: FlakeListItem,
) -> Element {
    let (chip_class, color, label) = match flake.status.as_str() {
        "synced" => ("chip-healthy", "#34d399", "synced"),
        "syncing" => ("chip-info", "#60a5fa", "syncing"),
        "error" => ("chip-critical", "#f87171", "error"),
        _ => ("chip-unknown", "#6b7280", &flake.status),
    };
    
    let title = flake.error_msg.as_deref().unwrap_or("");
    
    rsx! {
        span {
            class: "chip {chip_class}",
            title: "{title}",
            span {
                class: "chip-dot",
                style: "background:{color}"
            }
            "{label}"
        }
    }
}
```

### Verification for Phase 5
- [ ] PipelineDot renders with correct size (14x14px)
- [ ] PipelineDot shows E/B labels
- [ ] PipelineDot colors match status
- [ ] PipelinePill shows correct labels
- [ ] PipelinePill uses correct chip classes
- [ ] PipelineArrow renders → character
- [ ] RolloutPill shows N/M format
- [ ] RolloutPill progress bar width matches percentage
- [ ] FlakeSyncChip shows status with colored dot

---

## Phase 6: Diff Modal

### Goal
Implement full-screen diff viewer with line-by-line rendering and keyboard navigation.

### Files to Create
- `packages/web-ui/src/components/diff_modal.rs`

### Implementation Steps

#### Step 6.1: Modal Structure

**Design Reference:** Lines 271-393 in FlakesView.jsx

```rust
// packages/web-ui/src/components/diff_modal.rs

use dioxus::prelude::*;
use crate::api::models::*;
use crate::components::Icon;

#[component]
pub fn DiffModal(
    file: FileChange,
    commit: CommitInfo,
    flake: FlakeListItem,
    on_close: EventHandler<()>,
) -> Element {
    let mut active_hunk = use_signal(|| 0usize);
    let mut wrap_lines = use_signal(|| false);
    let body_ref = use_signal(|| None::<MountedData>);
    
    // Fetch diff content
    let diff_content = use_resource(move || {
        let file_name = file.filename.clone();
        let commit_sha = commit.sha.clone();
        async move {
            // TODO: Fetch actual diff from API
            // For now, return mock diff
            format!("--- a/{}\n+++ b/{}\n@@ -1,3 +1,4 @@\n line 1\n-old line 2\n+new line 2\n+added line\n line 3\n", file_name, file_name)
        }
    });
    
    // Parse diff into annotated lines
    let annotated_lines = use_memo(move || {
        if let Some(content) = diff_content.read().as_ref() {
            parse_unified_diff(content)
        } else {
            Vec::new()
        }
    });
    
    // ESC key handler
    use_effect(move || {
        let handler = move |evt: Event<KeyboardData>| {
            match evt.key() {
                Key::Escape => on_close.call(()),
                Key::Character(ch) if ch == "j" || ch == "n" => {
                    evt.prevent_default();
                    jump_hunk(1, &annotated_lines.read(), active_hunk);
                }
                Key::Character(ch) if ch == "k" || ch == "p" => {
                    evt.prevent_default();
                    jump_hunk(-1, &annotated_lines.read(), active_hunk);
                }
                _ => {}
            }
        };
        // TODO: Register keydown listener
    });
    
    let hunks: Vec<&DiffLine> = annotated_lines.read().iter()
        .filter(|line| matches!(line.line_type, DiffLineType::Hunk))
        .collect();
    
    let total_add = annotated_lines.read().iter()
        .filter(|line| matches!(line.line_type, DiffLineType::Add))
        .count();
    
    let total_del = annotated_lines.read().iter()
        .filter(|line| matches!(line.line_type, DiffLineType::Del))
        .count();
    
    rsx! {
        div {
            class: "modal-backdrop",
            style: "z-index:90",
            onclick: move |_| on_close.call(()),
            
            div {
                class: "diff-modal",
                onclick: |evt| evt.stop_propagation(),
                
                DiffModalHeader {
                    file: file.clone(),
                    commit: commit.clone(),
                    flake: flake.clone(),
                    total_add,
                    total_del,
                    hunk_count: hunks.len(),
                    line_count: annotated_lines.read().len(),
                    active_hunk: active_hunk.read().clone(),
                    wrap_lines: wrap_lines.read().clone(),
                    on_wrap_toggle: move |_| wrap_lines.set(!wrap_lines.read().clone()),
                    on_prev_hunk: move |_| jump_hunk(-1, &annotated_lines.read(), active_hunk),
                    on_next_hunk: move |_| jump_hunk(1, &annotated_lines.read(), active_hunk),
                    on_close,
                }
                
                div {
                    class: "diff-modal-body",
                    onmounted: move |evt| body_ref.set(Some(evt.data())),
                    
                    DiffTable {
                        lines: annotated_lines.read().clone(),
                        wrap: wrap_lines.read().clone(),
                    }
                }
            }
        }
    }
}

fn jump_hunk(direction: i32, lines: &[DiffLine], active_hunk: Signal<usize>) {
    let hunks: Vec<usize> = lines.iter()
        .enumerate()
        .filter_map(|(i, line)| {
            if matches!(line.line_type, DiffLineType::Hunk) {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    
    if hunks.is_empty() {
        return;
    }
    
    let current = active_hunk.read().clone();
    let next = if direction > 0 {
        (current + 1).min(hunks.len() - 1)
    } else {
        current.saturating_sub(1)
    };
    
    active_hunk.set(next);
    
    // TODO: Scroll to hunk position
}
```

#### Step 6.2: Diff Parsing

```rust
#[derive(Clone, Debug)]
pub enum DiffLineType {
    Meta,
    Hunk,
    Add,
    Del,
    Context,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub text: String,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub hunk_index: Option<usize>,
}

fn parse_unified_diff(diff: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_no = 0;
    let mut new_no = 0;
    let mut hunk_idx = None;
    
    for line in diff.lines() {
        if line.starts_with("@@") {
            // Parse hunk header: @@ -1,3 +1,4 @@
            if let Some(captures) = regex::Regex::new(r"-(\d+)(?:,\d+)?\s+\+(\d+)")
                .ok()
                .and_then(|re| re.captures(line))
            {
                old_no = captures.get(1).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0) - 1;
                new_no = captures.get(2).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0) - 1;
            }
            
            hunk_idx = Some(hunk_idx.map_or(0, |i: usize| i + 1));
            
            lines.push(DiffLine {
                line_type: DiffLineType::Hunk,
                text: line.to_string(),
                old_line_no: None,
                new_line_no: None,
                hunk_index: hunk_idx,
            });
        } else if line.starts_with("+++") || line.starts_with("---") {
            lines.push(DiffLine {
                line_type: DiffLineType::Meta,
                text: line.to_string(),
                old_line_no: None,
                new_line_no: None,
                hunk_index: None,
            });
        } else if line.starts_with("+") {
            new_no += 1;
            lines.push(DiffLine {
                line_type: DiffLineType::Add,
                text: line.to_string(),
                old_line_no: None,
                new_line_no: Some(new_no),
                hunk_index: hunk_idx,
            });
        } else if line.starts_with("-") {
            old_no += 1;
            lines.push(DiffLine {
                line_type: DiffLineType::Del,
                text: line.to_string(),
                old_line_no: Some(old_no),
                new_line_no: None,
                hunk_index: hunk_idx,
            });
        } else {
            old_no += 1;
            new_no += 1;
            lines.push(DiffLine {
                line_type: DiffLineType::Context,
                text: line.to_string(),
                old_line_no: Some(old_no),
                new_line_no: Some(new_no),
                hunk_index: hunk_idx,
            });
        }
    }
    
    lines
}
```

#### Step 6.3: Diff Modal Header

**Design Reference:** Lines 333-368 in FlakesView.jsx

```rust
#[component]
fn DiffModalHeader(
    file: FileChange,
    commit: CommitInfo,
    flake: FlakeListItem,
    total_add: usize,
    total_del: usize,
    hunk_count: usize,
    line_count: usize,
    active_hunk: usize,
    wrap_lines: bool,
    on_wrap_toggle: EventHandler<()>,
    on_prev_hunk: EventHandler<()>,
    on_next_hunk: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        header { class: "diff-modal-head",
            div { style: "min-width:0; flex:1",
                // Breadcrumb
                div {
                    style: "display:flex; align-items:center; gap:8px; font-size:11px; color:var(--cf-text-muted)",
                    Icon { name: "git", size: 11 }
                    span { class: "mono", "{flake.name}" }
                    span { "·" }
                    span { class: "mono", "{commit.sha}" }
                    span {
                        style: "overflow:hidden; text-overflow:ellipsis; white-space:nowrap",
                        "{commit.message}"
                    }
                }
                
                // File info
                div {
                    style: "display:flex; align-items:center; gap:10px; margin-top:4px; flex-wrap:wrap",
                    Icon { name: "file", size: 13, style: "opacity:0.6" }
                    span {
                        class: "mono",
                        style: "font-size:13px; font-weight:600",
                        "{file.filename}"
                    }
                    span { class: "chip chip-healthy", style: "font-size:10px", "+{total_add}" }
                    span { class: "chip chip-critical", style: "font-size:10px", "-{total_del}" }
                    span {
                        style: "font-size:11px; color:var(--cf-text-muted)",
                        "· {hunk_count} hunk{if hunk_count == 1 { \"\" } else { \"s\" }} · {line_count} lines"
                    }
                }
            }
            
            div { style: "display:flex; gap:6px; align-items:center",
                // Hunk navigation
                if hunk_count > 1 {
                    div { class: "diff-hunk-nav",
                        button {
                            class: "btn-icon focus-ring",
                            title: "Previous hunk (k)",
                            disabled: active_hunk == 0,
                            onclick: move |_| on_prev_hunk.call(()),
                            Icon { name: "chevron-up", size: 13 }
                        }
                        span {
                            class: "mono",
                            style: "font-size:11px; color:var(--cf-text-secondary); padding:0 6px",
                            "{active_hunk + 1}/{hunk_count}"
                        }
                        button {
                            class: "btn-icon focus-ring",
                            title: "Next hunk (j)",
                            disabled: active_hunk == hunk_count - 1,
                            onclick: move |_| on_next_hunk.call(()),
                            Icon { name: "chevron-down", size: 13 }
                        }
                    }
                }
                
                // Wrap toggle
                button {
                    class: if wrap_lines { "btn-icon focus-ring active" } else { "btn-icon focus-ring" },
                    title: if wrap_lines { "Disable line wrap" } else { "Wrap long lines" },
                    onclick: move |_| on_wrap_toggle.call(()),
                    Icon { name: "rows", size: 14 }
                }
                
                // Copy path button
                button {
                    class: "btn-icon focus-ring",
                    title: "Copy path",
                    Icon { name: "link", size: 14 }
                }
                
                // Close button
                button {
                    class: "btn-icon focus-ring",
                    title: "Close (Esc)",
                    onclick: move |_| on_close.call(()),
                    Icon { name: "x", size: 16 }
                }
            }
        }
    }
}
```

#### Step 6.4: Diff Table

**Design Reference:** Lines 370-388 in FlakesView.jsx

```rust
#[component]
fn DiffTable(
    lines: Vec<DiffLine>,
    wrap: bool,
) -> Element {
    let wrap_class = if wrap { " wrap" } else { "" };
    
    rsx! {
        table { class: "diff-table{wrap_class}",
            tbody {
                for (idx, line) in lines.iter().enumerate() {
                    {
                        match &line.line_type {
                            DiffLineType::Meta => None,
                            DiffLineType::Hunk => rsx! {
                                tr {
                                    key: "{idx}",
                                    class: "diff-hunk",
                                    td { colspan: 3, "{line.text}" }
                                }
                            },
                            _ => {
                                let row_class = match line.line_type {
                                    DiffLineType::Add => "diff-row diff-add",
                                    DiffLineType::Del => "diff-row diff-del",
                                    DiffLineType::Context => "diff-row diff-ctx",
                                    _ => "diff-row",
                                };
                                
                                rsx! {
                                    tr {
                                        key: "{idx}",
                                        class: "{row_class}",
                                        td {
                                            class: "diff-gutter mono",
                                            if let Some(old_no) = line.old_line_no {
                                                "{old_no}"
                                            }
                                        }
                                        td {
                                            class: "diff-gutter mono",
                                            if let Some(new_no) = line.new_line_no {
                                                "{new_no}"
                                            }
                                        }
                                        td {
                                            class: "diff-code mono",
                                            "{line.text}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

**Key Details:**
- Three columns: old line number | new line number | code
- `.diff-gutter` for line numbers (mono font)
- `.diff-code` for code content (mono font)
- Row classes: `.diff-add`, `.diff-del`, `.diff-ctx`, `.diff-hunk`
- Hunk rows span all 3 columns

### Verification for Phase 6
- [ ] Modal opens on file card click
- [ ] Backdrop click closes modal
- [ ] ESC key closes modal
- [ ] Diff header shows file path and stats
- [ ] Breadcrumb shows flake > commit > message
- [ ] Hunk navigation buttons work
- [ ] j/k keyboard shortcuts navigate hunks
- [ ] Line wrap toggle works
- [ ] Diff table renders with 3 columns
- [ ] Line numbers show correctly
- [ ] Add/del/context rows have correct styling

---

## Phase 7: Table & Cards Views

### Goal
Implement the two view modes for displaying flakes.

### Implementation Steps

#### Step 7.1: FlakeTable Component

**Design Reference:** Lines 451-495 in FlakesView.jsx

```rust
#[component]
pub fn FlakeTable(
    flakes: Vec<FlakeListItem>,
    selected: Option<FlakeListItem>,
    on_select: EventHandler<FlakeListItem>,
) -> Element {
    rsx! {
        div { class: "card", style: "overflow:hidden",
            table { class: "sys-table",
                thead {
                    tr {
                        th { "Flake" }
                        th { "Status" }
                        th { "Branch" }
                        th { "Systems" }
                        th { "Latest commit" }
                        th { "Author" }
                        th { "Synced" }
                        th { style: "text-align:right", " " }
                    }
                }
                tbody {
                    for flake in flakes {
                        {
                            let is_selected = selected.as_ref().map_or(false, |s| s.id == flake.id);
                            let row_class = if is_selected { "selected" } else { "" };
                            
                            rsx! {
                                tr {
                                    key: "{flake.id}",
                                    class: "{row_class}",
                                    style: "cursor:pointer",
                                    onclick: move |_| on_select.call(flake.clone()),
                                    
                                    td {
                                        div { style: "font-weight:600; font-size:13px", "{flake.name}" }
                                        div {
                                            style: "font-size:11px; color:var(--cf-text-muted)",
                                            "{flake.description.as_deref().unwrap_or(\"\")}"
                                        }
                                    }
                                    td { FlakeSyncChip { flake: flake.clone() } }
                                    td { span { class: "chip chip-unknown", "{flake.branch}" } }
                                    td { style: "font-size:13px", "{flake.system_count}" }
                                    td {
                                        span {
                                            class: "mono",
                                            style: "font-size:12px; font-weight:600",
                                            "{flake.latest_commit.as_deref().unwrap_or(\"-\")}"
                                        }
                                        div {
                                            style: "font-size:11px; color:var(--cf-text-muted); max-width:260px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap",
                                            "{flake.latest_message.as_deref().unwrap_or(\"\")}"
                                        }
                                    }
                                    td {
                                        class: "mono",
                                        style: "font-size:12px; color:var(--cf-text-secondary)",
                                        "{flake.latest_author.as_deref().unwrap_or(\"-\")}"
                                    }
                                    td {
                                        style: "font-size:12px; color:var(--cf-text-muted)",
                                        "{format_relative_time(flake.last_sync.unwrap_or(Utc::now()))}"
                                    }
                                    td {
                                        div { class: "row-actions",
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Sync",
                                                onclick: |evt| evt.stop_propagation(),
                                                Icon { name: "sync", size: 14 }
                                            }
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "More",
                                                onclick: |evt| evt.stop_propagation(),
                                                Icon { name: "more", size: 14 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

**Key Details:**
- 8 columns: Flake | Status | Branch | Systems | Latest commit | Author | Synced | Actions
- `.sys-table` class
- Selected row gets `.selected` class
- Row is clickable (opens tray)
- Actions column has sync and more buttons
- Flake cell has name (bold, 13px) and description (muted, 11px)
- Commit cell shows SHA + message (truncated at 260px)

#### Step 7.2: FlakeCards Component

**Design Reference:** Lines 498-540 in FlakesView.jsx

```rust
#[component]
pub fn FlakeCards(
    flakes: Vec<FlakeListItem>,
    selected: Option<FlakeListItem>,
    on_select: EventHandler<FlakeListItem>,
) -> Element {
    rsx! {
        div { class: "cards-grid",
            for flake in flakes {
                {
                    let is_selected = selected.as_ref().map_or(false, |s| s.id == flake.id);
                    let status_color = match flake.status.as_str() {
                        "synced" => "#34d399",
                        "syncing" => "#60a5fa",
                        "error" => "#f87171",
                        _ => "#6b7280",
                    };
                    
                    rsx! {
                        div {
                            key: "{flake.id}",
                            class: "sys-card",
                            style: if is_selected {
                                "border-color:var(--cf-brand-purple)"
                            } else {
                                ""
                            },
                            onclick: move |_| on_select.call(flake.clone()),
                            
                            // Status rail
                            div {
                                class: "status-rail",
                                style: "--status-color:{status_color}"
                            }
                            
                            // Card header
                            div { class: "sys-card-head",
                                div { class: "sys-title",
                                    div { class: "sys-hostname",
                                        Icon { name: "git", size: 13 }
                                        " {flake.name}"
                                    }
                                    div { class: "sys-fqdn", "{flake.url}" }
                                }
                                EnvBadge { env: flake.environment.clone() }
                            }
                            
                            // Description
                            div {
                                style: "font-size:12px; color:var(--cf-text-secondary)",
                                "{flake.description.as_deref().unwrap_or(\"\")}"
                            }
                            
                            // Card body (key-value grid)
                            div { class: "sys-card-body",
                                div {
                                    div { class: "sys-kv-key", "Branch" }
                                    div { class: "sys-kv-val", "{flake.branch}" }
                                }
                                div {
                                    div { class: "sys-kv-key", "Systems" }
                                    div {
                                        class: "sys-kv-val",
                                        style: "font-family:inherit",
                                        "{flake.system_count}"
                                    }
                                }
                                div {
                                    div { class: "sys-kv-key", "Commit" }
                                    div {
                                        class: "sys-kv-val",
                                        "{flake.latest_commit.as_deref().unwrap_or(\"-\")}"
                                    }
                                }
                                div {
                                    div { class: "sys-kv-key", "Synced" }
                                    div {
                                        class: "sys-kv-val",
                                        style: "font-family:inherit",
                                        "{format_relative_time(flake.last_sync.unwrap_or(Utc::now()))}"
                                    }
                                }
                            }
                            
                            // Error callout (if error)
                            if let Some(error_msg) = &flake.error_msg {
                                div {
                                    class: "sd-callout sd-callout-danger",
                                    style: "padding:8px 10px",
                                    Icon { name: "warn", size: 12 }
                                    div { style: "font-size:11px", "{error_msg}" }
                                }
                            }
                            
                            // Card footer
                            div { class: "sys-card-foot",
                                div { class: "chips-row",
                                    FlakeSyncChip { flake: flake.clone() }
                                    span {
                                        class: "chip chip-unknown",
                                        "{flake.total_commits.unwrap_or(0)} commits"
                                    }
                                }
                                button {
                                    class: "btn btn-subtle focus-ring",
                                    style: "padding:4px 10px; font-size:12px",
                                    onclick: |evt| evt.stop_propagation(),
                                    Icon { name: "sync", size: 12 }
                                    " Sync"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

**Key Details:**
- `.cards-grid` container
- `.sys-card` with `.status-rail`
- Status rail color set via CSS variable `--status-color`
- Selected card has purple border
- Card structure: header | description | body (kv-grid) | error callout (optional) | footer
- Footer has chips row (status + commit count) and sync button

### Verification for Phase 7
- [ ] Table view displays 8 columns
- [ ] Cards view displays in grid
- [ ] Clicking flake opens tray in both views
- [ ] Selected flake highlights (purple border in cards, selected class in table)
- [ ] Status rail shows correct color in cards
- [ ] Action buttons work (stop propagation)
- [ ] All data fields display correctly

---

## Phase 8: Integration & Testing

### Goal
Wire up API endpoints, handle loading/error states, and perform comprehensive testing.

### Implementation Steps

#### Step 8.1: API Integration

Create or update API client methods:

```rust
// packages/web-ui/src/api/client.rs

impl ApiClient {
    pub async fn list_flakes(&self) -> Result<Vec<FlakeListItem>, Error> {
        self.get("/api/v1/flakes").await
    }
    
    pub async fn get_flake_commits(&self, flake_id: i32) -> Result<Vec<CommitInfo>, Error> {
        self.get(&format!("/api/v1/flakes/{}/commits", flake_id)).await
    }
    
    pub async fn get_commit_files(&self, flake_id: i32, commit_sha: &str) -> Result<Vec<FileChange>, Error> {
        self.get(&format!("/api/v1/flakes/{}/commits/{}/files", flake_id, commit_sha)).await
    }
    
    pub async fn get_file_diff(&self, flake_id: i32, commit_sha: &str, file_path: &str) -> Result<String, Error> {
        self.get(&format!("/api/v1/flakes/{}/commits/{}/diff?file={}", flake_id, commit_sha, file_path)).await
    }
    
    pub async fn sync_flake(&self, flake_id: i32) -> Result<(), Error> {
        self.post(&format!("/api/v1/flakes/{}/sync", flake_id), &()).await
    }
    
    pub async fn sync_all_flakes(&self) -> Result<(), Error> {
        self.post("/api/v1/flakes/sync-all", &()).await
    }
}
```

#### Step 8.2: Loading States

Add loading indicators:

```rust
// In FlakesListView
let flakes = use_resource(move || async move {
    api_client.list_flakes().await
});

match flakes.read().as_ref() {
    None => rsx! {
        div { class: "card", style: "padding:48px; text-align:center",
            div { class: "spinner" }
            p { style: "margin-top:12px; color:var(--cf-text-muted)",
                "Loading flakes..."
            }
        }
    },
    Some(Err(e)) => rsx! {
        div { class: "sd-callout sd-callout-danger",
            Icon { name: "warn", size: 14 }
            div { "Failed to load flakes: {e}" }
        }
    },
    Some(Ok(flakes_data)) => {
        // Render main view
    }
}
```

#### Step 8.3: Empty States

Add empty states for no data:

```rust
// No flakes
if flakes.is_empty() {
    rsx! {
        div { class: "empty", style: "margin:48px auto; max-width:400px; text-align:center",
            Icon { name: "git", size: 48, style: "opacity:0.3" }
            h3 { style: "margin-top:16px", "No flakes configured" }
            p { style: "color:var(--cf-text-muted); margin-top:8px",
                "Add a NixOS flake repository to get started."
            }
            button {
                class: "btn btn-primary focus-ring",
                style: "margin-top:16px",
                onclick: move |_| show_add_modal.set(true),
                Icon { name: "plus", size: 14 }
                " Add flake"
            }
        }
    }
}

// No commits in tray
if commits.is_empty() {
    rsx! {
        div { class: "empty", style: "margin:32px",
            "No commits yet for this flake."
        }
    }
}

// No files changed
if files.is_empty() {
    rsx! {
        div { class: "empty", style: "margin:24px",
            "No files changed in this commit."
        }
    }
}
```

#### Step 8.4: Error Handling

Add error boundaries and fallbacks:

```rust
// Wrap resources with error handling
let commits = use_resource(move || {
    let flake_id = selected_flake.read().as_ref().map(|f| f.id);
    async move {
        if let Some(id) = flake_id {
            api_client.get_flake_commits(id).await.ok()
        } else {
            None
        }
    }
});

// Handle None/Some(None)/Some(Some(data))
match commits.read().as_ref() {
    None => {
        // Still loading
        rsx! { div { class: "spinner" } }
    }
    Some(None) => {
        // Error or no flake selected
        None
    }
    Some(Some(data)) => {
        // Render commits
    }
}
```

### Verification for Phase 8
- [ ] API calls fetch real data
- [ ] Loading spinners show during fetch
- [ ] Error messages display on failure
- [ ] Empty states render when no data
- [ ] Sync buttons trigger API calls
- [ ] Add flake modal submits to API
- [ ] All user interactions work end-to-end

---

## CSS Classes Reference

### Main View Classes

```css
.page-head { /* Header container */ }
.page-title { /* H1 title */ }
.page-subtitle { /* Stats subtitle */ }
.filterbar { /* Filter bar container */ }
.filter-search { /* Search input container */ }
.seg { /* Segmented control */ }
.seg button.active { /* Active segment */ }
.filter-count { /* Result count */ }
```

### Side Tray Classes

```css
.fl-tray-backdrop { /* Dark overlay */ }
.fl-tray { /* Slide-in panel */ }
.fl-tray-head { /* Tray header */ }
.fl-tray-body { /* Two-pane layout */ }
```

### Commit List Classes

```css
.fl-tray-commits { /* Left pane container */ }
.fl-tray-commits-search { /* Search bar */ }
.fl-commits-bucket { /* Time bucket header */ }
.fl-commit-item { /* Commit row */ }
.fl-commit-item.active { /* Selected commit */ }
.fl-rail { /* Timeline rail container */ }
.fl-dot { /* Timeline dot */ }
.fl-dot.sel { /* Selected dot */ }
.fl-stem { /* Connecting line */ }
```

### Detail Pane Classes

```css
.fl-tray-detail { /* Right pane container */ }
.fl-tray-commit-h { /* Commit header */ }
.fl-tray-section-h { /* Section header */ }
.fl-pipeline { /* Pipeline strip */ }
.fl-files-section { /* Files section */ }
.fl-files-grid { /* Files grid */ }
.fl-file-card { /* File card button */ }
.fl-file-card-head { /* File name/path */ }
.fl-file-name { /* Filename (truncated) */ }
.fl-file-path { /* Path (muted) */ }
.fl-file-stats { /* Add/del stats */ }
.fl-file-bar { /* Visual bar */ }
```

### Diff Modal Classes

```css
.modal-backdrop { /* Full-screen backdrop */ }
.diff-modal { /* Modal container */ }
.diff-modal-head { /* Modal header */ }
.diff-modal-body { /* Scrollable body */ }
.diff-hunk-nav { /* Hunk navigation */ }
.diff-table { /* Diff table */ }
.diff-table.wrap { /* Wrapped lines */ }
.diff-row { /* Diff table row */ }
.diff-add { /* Added line */ }
.diff-del { /* Deleted line */ }
.diff-ctx { /* Context line */ }
.diff-hunk { /* Hunk header row */ }
.diff-gutter { /* Line number cell */ }
.diff-code { /* Code content cell */ }
```

### Table & Cards Classes

```css
.sys-table { /* Table view */ }
.sys-table thead { /* Table header */ }
.sys-table tbody tr { /* Table row */ }
.sys-table tbody tr.selected { /* Selected row */ }
.row-actions { /* Action buttons */ }

.cards-grid { /* Cards container */ }
.sys-card { /* Individual card */ }
.status-rail { /* Colored left rail */ }
.sys-card-head { /* Card header */ }
.sys-title { /* Title section */ }
.sys-hostname { /* Flake name */ }
.sys-fqdn { /* URL */ }
.sys-card-body { /* KV grid */ }
.sys-kv-key { /* Key label */ }
.sys-kv-val { /* Value */ }
.sys-card-foot { /* Card footer */ }
.chips-row { /* Chip row */ }
```

---

## Data Models

### FlakeListItem

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlakeListItem {
    pub id: i32,
    pub name: String,
    pub url: String,
    pub branch: String,
    pub description: Option<String>,
    pub status: String, // "synced" | "syncing" | "error"
    pub error_msg: Option<String>,
    pub system_count: i32,
    pub environment: Option<String>,
    pub latest_commit: Option<String>,
    pub latest_message: Option<String>,
    pub latest_author: Option<String>,
    pub last_sync: Option<DateTime<Utc>>,
    pub total_commits: Option<i32>,
}
```

### CommitInfo

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitInfo {
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
```

### FileChange

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChange {
    pub filename: String,
    pub additions: i32,
    pub deletions: i32,
}
```

---

## API Requirements

### Endpoints Needed

1. **GET /api/v1/flakes**
   - Returns: `Vec<FlakeListItem>`
   - Lists all registered flakes

2. **GET /api/v1/flakes/{id}/commits**
   - Returns: `Vec<CommitInfo>`
   - Lists commits for a flake
   - Should include eval/build status if available

3. **GET /api/v1/flakes/{id}/commits/{sha}/files**
   - Returns: `Vec<FileChange>`
   - Lists files changed in a commit

4. **GET /api/v1/flakes/{id}/commits/{sha}/diff?file={path}**
   - Returns: `String` (unified diff format)
   - Gets diff content for a specific file

5. **POST /api/v1/flakes/{id}/sync**
   - Returns: `{ "status": "ok" }`
   - Triggers sync for one flake

6. **POST /api/v1/flakes/sync-all**
   - Returns: `{ "status": "ok" }`
   - Triggers sync for all flakes

### Backend Implementation Notes

- Commits should be returned in reverse chronological order
- Eval/build status should come from existing evaluation/build tables
- Diff content can be generated using `git show` or stored if available
- Consider caching commit metadata for performance

---

## Testing Checklist

### Manual Testing

#### Main View
- [ ] Navigate to /flakes
- [ ] Verify page header shows correct stats (tracked, systems, synced)
- [ ] Test search input filters flakes by name/description
- [ ] Test table/cards view toggle switches mode
- [ ] Verify result count updates with search
- [ ] Click "Sync all" button (verify API call)
- [ ] Click "Add flake" button (modal opens)

#### Table View
- [ ] All 8 columns display correctly
- [ ] Clicking row opens tray
- [ ] Selected row highlights
- [ ] Sync button in actions column works
- [ ] Flake name, description, commit, author all display

#### Cards View
- [ ] Cards display in grid
- [ ] Status rail shows correct color
- [ ] Clicking card opens tray
- [ ] Selected card has purple border
- [ ] Sync button works
- [ ] Error callout shows if present

#### Side Tray
- [ ] Tray slides in from right
- [ ] Backdrop appears
- [ ] Click backdrop closes tray
- [ ] ESC key closes tray
- [ ] Tray header shows flake info correctly
- [ ] Two-pane layout renders

#### Commit Timeline
- [ ] Commits group by time (Today/This week/Earlier)
- [ ] Search input filters commits
- [ ] Result count shows filtered/total
- [ ] Timeline rail renders with dots
- [ ] Stems connect commits (except last)
- [ ] Clicking commit selects it
- [ ] Selected commit highlights in purple
- [ ] Pipeline dots show eval/build status

#### Commit Detail
- [ ] Commit header shows SHA, message, author, timestamp
- [ ] Stats display (+add/-del in correct colors)
- [ ] Pipeline strip shows Eval → Build → Rollout
- [ ] Rollout pill shows N/M systems with progress bar
- [ ] Files changed section displays
- [ ] File cards show in grid

#### File Cards
- [ ] File icon displays
- [ ] Filename and path display (truncated if long)
- [ ] +add and -del stats show
- [ ] Visual bar shows proportion correctly
- [ ] Clicking file card opens diff modal

#### Diff Modal
- [ ] Modal opens full-screen
- [ ] Backdrop click closes modal
- [ ] ESC key closes modal
- [ ] Header shows breadcrumb (flake > commit > message)
- [ ] Filename, +/- stats, hunk count display
- [ ] Hunk navigation buttons work
- [ ] j/k keyboard shortcuts navigate hunks
- [ ] Wrap toggle works
- [ ] Diff table renders with 3 columns
- [ ] Line numbers show correctly
- [ ] Add/del/context rows have correct colors
- [ ] Hunk headers span all columns

### Automated Testing

```bash
# Format check
cargo fmt --check

# Lint check
cargo clippy -- -D warnings

# Unit tests (if any)
cargo test flakes

# Integration tests
nix build .#checks.x86_64-linux.web-ui
```

### Visual Verification

- [ ] Open design file side-by-side with browser
- [ ] Compare page header layout and spacing
- [ ] Compare filter bar elements
- [ ] Compare table column widths
- [ ] Compare card layout and spacing
- [ ] Compare tray width (840px max)
- [ ] Compare commit timeline spacing
- [ ] Compare pipeline strip layout
- [ ] Compare file card grid
- [ ] Compare diff modal layout
- [ ] Verify all font sizes match (11px, 12px, 13px, 14px, 15px)
- [ ] Verify all colors match (purple, green, red, muted)
- [ ] Verify all gaps/margins match (4px, 6px, 8px, 10px, 12px, 16px, etc.)

### Performance Testing

- [ ] Load time with 10 flakes < 2s
- [ ] Load time with 50 flakes < 5s
- [ ] Tray opens smoothly (no lag)
- [ ] Commit list scrolls smoothly with 100+ commits
- [ ] Search filtering is instant
- [ ] View mode toggle is instant
- [ ] Diff modal opens without delay
- [ ] No console errors
- [ ] No memory leaks (check DevTools)

---

## Common Issues & Solutions

### Issue: ESC key not working
**Solution:** Ensure keyboard event listeners are properly registered in `use_effect` hooks. Check that events are not being prevented by other components.

### Issue: Timeline rail misaligned
**Solution:** Verify `.fl-rail` CSS is applied correctly. Check that `.fl-dot` and `.fl-stem` have correct positioning.

### Issue: Diff modal backdrop z-index conflict
**Solution:** Ensure modal backdrop has `z-index: 90` and tray has `z-index: 81`.

### Issue: File cards not clickable
**Solution:** Verify buttons are not nested inside other buttons. Use `onclick` with proper event handling.

### Issue: Commit groups empty
**Solution:** Check time bucketing logic. Ensure `format_relative_time` returns expected format.

### Issue: API calls not working
**Solution:** Check CORS headers. Verify API client is initialized with correct base URL. Check network tab in DevTools.

### Issue: Pipeline status not updating
**Solution:** Ensure `eval_status` and `build_status` are returned from API. Check data model matches API response.

### Issue: Search not filtering
**Solution:** Verify `use_memo` dependencies. Ensure search query is lowercase compared with lowercase fields.

---

## Completion Checklist

### Phase Completion
- [ ] Phase 1: Main View Structure complete
- [ ] Phase 2: Side Tray Foundation complete
- [ ] Phase 3: Commit Timeline complete
- [ ] Phase 4: Commit Detail Panel complete
- [ ] Phase 5: Pipeline Components complete
- [ ] Phase 6: Diff Modal complete
- [ ] Phase 7: Table & Cards Views complete
- [ ] Phase 8: Integration & Testing complete

### Acceptance Criteria (All 27)
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

### Final Verification
- [ ] Side-by-side visual comparison passes
- [ ] All interactions work as expected
- [ ] No console errors
- [ ] cargo fmt --check passes
- [ ] cargo clippy passes
- [ ] cargo test passes (if tests exist)
- [ ] nix build .#checks.x86_64-linux.web-ui passes
- [ ] Performance targets met (<2s initial load, <5s with 50 flakes)

---

## Next Steps After Completion

1. Create merge request
2. Include screenshots in MR description
3. Link to TASK-297
4. Request review
5. Address feedback
6. Merge into dev
7. Mark TASK-297 as Done
8. Clean up worktree

---

## Notes for Implementer

- **Study the design first:** Read the entire FlakesView.jsx before writing code
- **Match exactly:** Every pixel, every color, every spacing value
- **Use design system classes:** Don't invent new classes, use what exists
- **Test frequently:** Build and test after each phase
- **Commit incrementally:** Don't wait until everything is done
- **Ask for help:** If stuck, reference this guide or ask for clarification
- **Visual verification is key:** Side-by-side comparison catches subtle differences

Good luck! 🚀
