---
id: TASK-311
title: Refactor Builders view to pixel-perfect JSX parity
status: In Progress
assignee: []
created_date: '2026-05-24 01:24'
updated_date: '2026-05-24 01:27'
labels:
  - ui
  - ux
  - builders
  - web-ui
  - design-system
  - pixel-perfect
milestone: UI/UX Design System
dependencies: []
priority: high
ordinal: 2100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current Builders view does NOT match the reference JSX design at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx`. The implementation is incomplete, uses wrong patterns, and lacks critical functionality.

## Goal

Implement a **PIXEL-FOR-PIXEL IDENTICAL** port of BuildersView.jsx into Dioxus. This is NOT a "close enough" task. This is NOT a "mock it out" task. This is a FULL IMPLEMENTATION with REAL BACKEND INTEGRATION.

## Design Source (AUTHORITATIVE)

- JSX Reference: `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx`
- Mock Data: `/home/mcamp/code/crystal-forge/CrystalForgelatest/data-builds.js` (BUILD_WORKERS, lines 51-57)
- Environments: `/home/mcamp/code/crystal-forge/CrystalForgelatest/data.js` (ENVIRONMENTS, lines 3-9)

## CRITICAL CONSTRAINTS (MANDATORY - NO EXCEPTIONS)

1. **PIXEL-PERFECT MATCHING REQUIRED**
   - Every spacing value MUST match JSX exactly
   - Every color MUST match JSX exactly
   - Every font size MUST match JSX exactly
   - Every class name MUST match JSX semantic classes where used

2. **REAL BACKEND INTEGRATION REQUIRED**
   - NO mocked data in the component
   - ALL data MUST come from real API endpoints
   - ALL actions (add/edit/delete) MUST call real backend APIs
   - Backend endpoints MUST be implemented if they don't exist

3. **COMPLETE FUNCTIONALITY REQUIRED**
   - ALL filters MUST work (search, status, architecture, view mode)
   - ALL modals MUST work (add builder, edit builder, delete confirmation)
   - ALL form fields MUST work and persist to database

4. **NO HALF-MEASURES ALLOWED**
   - If you run low on context, STOP and report progress
   - If something is hard, implement it anyway
   - If backend doesn't support something, ADD backend support
   - NO shortcuts. NO "good enough". NO "mostly done"

## Risk Level

HIGH: Complex full-stack feature requiring pixel-perfect UI, complete backend integration, and comprehensive state management. Cutting corners WILL result in task rejection.

## Architectural Constraints

- NO business logic in view components
- State management separated from presentation
- API client properly handles errors
- DTOs match backend models exactly
- No unwrap() in production code paths
- All user inputs validated on frontend AND backend
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 EVERY checklist item in implementation plan is complete and verified
- [ ] #2 Visual appearance is PIXEL-IDENTICAL to JSX reference (with screenshots proving it)
- [ ] #3 ALL functionality works with REAL backend data (NO mocked data in component)
- [ ] #4 ALL CRUD operations work correctly and persist to database
- [ ] #5 ALL filters work correctly and can be combined
- [ ] #6 View mode toggle (cards/table) works correctly
- [ ] #7 Add/edit/delete modals work with full validation and API integration
- [ ] #8 Environment badges display correct colors for all environment types
- [ ] #9 Builder status chips display correct colors and labels for all statuses
- [ ] #10 Progress bars calculate correctly with conditional colors
- [ ] #11 Icon sizes match JSX reference exactly (no Unicode fallbacks)
- [ ] #12 Backend API endpoints exist and return correct data structure
- [ ] #13 Database migrations applied if schema changes were needed
- [ ] #14 RBAC checks implemented (admin-only for add/edit/delete)
- [ ] #15 Error handling implemented (validation errors, API errors, network errors)
- [ ] #16 `nix develop -c cargo check` (web-ui) passes
- [ ] #17 `nix build .#checks.x86_64-linux.web-ui` passes with screenshot evidence
- [ ] #18 NO shortcuts taken, NO features mocked out, NO compromises
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## MANDATORY IMPLEMENTATION CHECKLIST - BY THE NUMBERS

Execute each phase in order. Do NOT skip ahead. Do NOT mark complete until verified.

### PHASE 1: DATA STRUCTURE & API CONTRACT (COMPLETE BEFORE ANY UI WORK)

- [ ] 1.1: Inspect backend Builder model in `packages/default/src/models/builders.rs`
- [ ] 1.2: Verify ALL required JSX fields exist in backend:
  - [ ] id, name, host, arch (x86_64-linux|aarch64-linux|aarch64-darwin|x86_64-darwin)
  - [ ] cores, mem (GiB), slots.used, slots.total (max_concurrent_slots)
  - [ ] status (running|paused|offline|draining), load (f32 0.0-1.0)
  - [ ] lastSeen, uptimeDays, completed24h, failed24h
  - [ ] environments (Vec<String>), publicKey, enabled (bool)
- [ ] 1.3: Add missing fields to backend WITH MIGRATION if needed
- [ ] 1.4: Verify/create API endpoints:
  - [ ] GET /api/builders (list all), POST /api/builders (create)
  - [ ] PUT /api/builders/:id (update), DELETE /api/builders/:id (delete)
  - [ ] GET /api/builders/stats (aggregated: total, running, slots, builds)
- [ ] 1.5: Create Dioxus DTO structs matching JSX data exactly
- [ ] 1.6: Test API with curl/httpie - verify response structure matches DTO

### PHASE 2: PAGE STRUCTURE (EXACT JSX LINE 34-117)

- [ ] 2.1: Container div with gap:16 (Tailwind: gap-4, NOT gap-6)
- [ ] 2.2: Page head (className="page-head"):
  - [ ] Left div: h1.page-title + p.page-subtitle
  - [ ] Subtitle computes: "X of Y running · A/B slots · C builds in 24h"
  - [ ] Right: button.btn.btn-primary.focus-ring with Icon plus size=14
- [ ] 2.3: Subtitle values from REAL computed data (NOT hardcoded)

### PHASE 3: STAT STRIP (EXACT JSX LINE 47-61)

- [ ] 3.1: Container className="stat-strip"
- [ ] 3.2: Five stat cards (EXACT order):
  - [ ] Total (#a78bfa), Running (#34d399)
  - [ ] Slot use (>85%:#fbbf24 else #60a5fa), Built 24h (#34d399)
  - [ ] Failed 24h (>0:#f87171 else #34d399)
- [ ] 3.3: Each stat: span.stat-accent (--stat-color) + div.stat-label + div.stat-value
- [ ] 3.4: Compute from builder data:
  - [ ] total=len, running=filter(status=running).len
  - [ ] slotsUsed=sum(slots.used), slotsTotal=sum(slots.total)
  - [ ] completed=sum(completed24h), failed=sum(failed24h), slotPct=round(used/total*100)

### PHASE 4: FILTER BAR (ALL FILTERS MUST WORK - JSX LINE 63-82)

- [ ] 4.1: Container className="filterbar"
- [ ] 4.2: Search (maxWidth:320px):
  - [ ] div.filter-search: Icon search + input.input.focus-ring
  - [ ] Filters name OR host OR arch (lowercase)
- [ ] 4.3: Status filter div.seg:
  - [ ] Buttons: all, running, paused, offline (active class on selected)
- [ ] 4.4: Arch dropdown:
  - [ ] select.input.filter-select.focus-ring (width:auto)
  - [ ] Options: "All architectures" + unique arches from data
- [ ] 4.5: View mode div.seg:
  - [ ] Cards (Icon grid size=12), Table (Icon rows size=12)
- [ ] 4.6: span.filter-count: "{filtered.length} builders"

### PHASE 5: CARDS VIEW (PIXEL-PERFECT - JSX LINE 84-182)

- [ ] 5.1: div.cards-grid (when viewMode=cards)
- [ ] 5.2: BuilderCard structure for each filtered builder:
  - [ ] div.sys-card
  - [ ] div.status-rail --status-color: running=#34d399, paused=#fbbf24, else=#f87171
  - [ ] div.sys-card-head:
    - [ ] div.sys-title: div.sys-hostname (Icon cpu 13px + name), div.sys-fqdn (host)
    - [ ] builderStatusChip(w)
  - [ ] div.sys-card-body (4 rows):
    - [ ] Arch: sys-kv-key + sys-kv-val
    - [ ] Cores·mem: sys-kv-key + sys-kv-val (Xc · Y GiB)
    - [ ] Environments: sys-kv-key + EnvBadges (or italic "none" if empty)
    - [ ] Last seen: sys-kv-key + sys-kv-val
  - [ ] Slot use bar: label row + h:5 bar + fill (>85%:#fbbf24 else #34d399)
  - [ ] Load bar: label row + h:5 bar + fill (>85%:#f87171, >60%:#fbbf24, else:#60a5fa)
  - [ ] div.sys-card-foot:
    - [ ] div.chips-row: chip-healthy "X built", chip-critical "Y failed" (if >0)
    - [ ] btn.btn-subtle (pad:4px 10px, font:12): Icon gear 12px + "Edit"

### PHASE 6: TABLE VIEW (EXACT STRUCTURE - JSX LINE 88-224)

- [ ] 6.1: div.card overflow:hidden wrapper
- [ ] 6.2: table.sys-table with 8 columns:
  - [ ] thead: Builder, Status, Arch·envs, Resources, Slot use, Built 24h, Last seen, (actions)
- [ ] 6.3: BuilderRow for each filtered builder (tr cursor:pointer onClick=edit):
  - [ ] td1: div font:600/13 (name), div.mono font:11 muted (host)
  - [ ] td2: builderStatusChip
  - [ ] td3: div.mono font:12 (arch), div font:11 flex gap:4 mt:2 (EnvBadges)
  - [ ] td4: mono font:12 "Xc · Y GiB"
  - [ ] td5: flex align:center gap:8 minW:130 (h:4 bar + mono text)
  - [ ] td6: flex col gap:1 (mono completed, color:#f87171 failed if >0)
  - [ ] td7: font:12 muted (lastSeen)
  - [ ] td8: div.row-actions: btn-icon.focus-ring Icon gear 14px

### PHASE 7: STATUS CHIP (EXACT MAPPING - JSX LINE 121-129)

- [ ] 7.1: builderStatusChip function returns chip element
- [ ] 7.2: Config map:
  - [ ] running: chip-healthy, #34d399, "running"
  - [ ] paused: chip-warning, #fbbf24, "paused"
  - [ ] offline: chip-critical, #f87171, "offline"
  - [ ] draining: chip-info, #60a5fa, "draining"
  - [ ] fallback: chip-unknown, #6b7280, status
- [ ] 7.3: span.chip.{cls}: span.chip-dot (bg:color) + label

### PHASE 8: ENVIRONMENT BADGE (JSX USES ENVIRONMENTS CONSTANT)

- [ ] 8.1: EnvBadge component accepts env:String
- [ ] 8.2: Lookup color (production:#dc2626, staging:#d97706, dev:#2563eb, edge:#0f766e, lab:#7c3aed)
- [ ] 8.3: Structure: pad:4px/10px, radius:99, font:11, border:1px {color}
  - [ ] bg: color-mix(in oklab, {color} 14%, var(--cf-card-bg))
  - [ ] 6x6 dot (bg:color) + text

### PHASE 9: ADD/EDIT MODAL (FULL FORM - JSX LINE 226-367)

- [ ] 9.1: Modal triggered by Add button or card/row Edit
- [ ] 9.2: div.modal-backdrop onClick=close
- [ ] 9.3: div.modal (w:min(620px,96vw), maxH:92vh, stopPropagation)
- [ ] 9.4: Form state (edit uses builder data, add uses defaults):
  - [ ] name:"", host:"", arch:"x86_64-linux", environments:["production"]
  - [ ] cores:16, mem:64, maxSlots:4, publicKey:"", enabled:true
- [ ] 9.5: div.modal-head: h2 (Icon gear|plus + title), p (subtitle)
- [ ] 9.6: div.modal-body (overflowY:auto):
  - [ ] Grid 1fr/1fr gap:14: Name input, Environments toggles (help text below)
  - [ ] Host input (mono font:12)
  - [ ] Grid 1fr/1fr/1fr gap:14: Arch select, Cores input, Mem input
  - [ ] Grid 1fr/1fr gap:14: MaxSlots input (help), Enabled checkbox
  - [ ] PublicKey textarea rows:3 font:11 (help)
  - [ ] If edit: Danger zone (border-top, Remove button color:#f87171)
- [ ] 9.7: div.modal-foot: Cancel (btn-ghost), Save (btn-primary Icon check 13px)
- [ ] 9.8: Save onClick: POST|PUT API, on success close+refetch, on error SHOW ERROR

### PHASE 10: DELETE MODAL (JSX LINE 369-410)

- [ ] 10.1: Shown when confirmDelete=true
- [ ] 10.2: modal-head (bg:rgba(248,113,113,0.06)):
  - [ ] h2 color:#fecaca: Icon warn 16px + "Remove builder"
  - [ ] p: "This unregisters {name} from build queue"
- [ ] 10.3: modal-body:
  - [ ] If slots.used>0: sd-callout-danger with Icon warn + warning text
  - [ ] Confirmation input: label, input.mono autoFocus, borderColor red if wrong
- [ ] 10.4: modal-foot: Cancel, Remove (disabled if !match, bg:#dc2626 if match)
- [ ] 10.5: Remove onClick: DELETE API, success close+refetch, error SHOW ERROR

### PHASE 11: BACKEND API (IMPLEMENT IF MISSING)

- [ ] 11.1: Check handlers in `packages/default/src/handlers/api/builders.rs`
- [ ] 11.2: Implement if missing:
  - [ ] list_builders, create_builder, update_builder, delete_builder, builder_stats
- [ ] 11.3: Add DB migrations for new fields
- [ ] 11.4: Error handling: 400 validation, 404 not found, 500 DB error
- [ ] 11.5: RBAC: admin role required for create/update/delete

### PHASE 12: ICONS (EXACT SIZES - NO UNICODE)

- [ ] 12.1: Plus:14px, Search:default, Grid:12px, Rows:12px, CPU:13px
- [ ] 12.2: Gear:12px(card)|14px(table), Check:13px, X:12px|13px, Warn:14px|16px
- [ ] 12.3: Use SVG Icon component, NO Unicode fallbacks

### PHASE 13: FILTERING LOGIC (JSX LINE 13-21)

- [ ] 13.1: Combine all filters:
  - [ ] statusFilter!="all" → filter by status
  - [ ] archFilter!="all" → filter by arch
  - [ ] query → lowercase match name|host|arch
- [ ] 13.2: Updates immediately on any filter change
- [ ] 13.3: Count shows filtered.length

### PHASE 14: STATE MANAGEMENT

- [ ] 14.1: State: builders, query, statusFilter, archFilter, viewMode, editingBuilder, addOpen, confirmDelete
- [ ] 14.2: Load from API on mount
- [ ] 14.3: Reload after add/edit/delete
- [ ] 14.4: Loading state during fetch
- [ ] 14.5: Error state if API fails

### PHASE 15: VERIFICATION (SCREENSHOTS REQUIRED)

- [ ] 15.1: `nix develop -c cargo check` PASS
- [ ] 15.2: `nix build .#checks.x86_64-linux.web-ui` PASS
- [ ] 15.3: Screenshots proving pixel-parity:
  - [ ] Page head + stats (empty and with data)
  - [ ] Stat strip (slot>85%, failed>0 color variations)
  - [ ] Filter bar, Cards view (all statuses), Table view
  - [ ] Add modal, Edit modal (with danger zone), Delete modal (with warning)
  - [ ] Search working, Status filter working, Arch filter working, View toggle
- [ ] 15.4: Functional verification:
  - [ ] Add: form submits, API called, list refreshes
  - [ ] Edit: pre-fills, submits, refreshes
  - [ ] Delete: requires confirmation, API called, refreshes
  - [ ] Filters work independently and combined
  - [ ] Env badges correct colors, Status chips correct colors
  - [ ] Progress bars calculate correctly with threshold colors
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Execution Start: 2026-05-24 01:26

Commencing pixel-perfect JSX port of Builders view.
Executing phases sequentially - no skipping allowed.
<!-- SECTION:NOTES:END -->
