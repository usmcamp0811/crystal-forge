# Web-UI Component Reorganization

## Goal
Reorganize the web-ui crate to follow best practices for component/view separation, eliminating duplicate definitions and extracting reusable components from large view files.

## Current State

| View File | Current Lines | Target Lines | Reduction |
|-----------|---------------|--------------|-----------|
| dashboard.rs | 1921 | ~300-400 | 80% |
| system_detail.rs | 2817 | ~800 | 72% |
| flakes_list.rs | 2203 | ~500 | 77% |
| systems_list.rs | 2227 | ~400-500 | 80% |
| builds.rs | 1153 | ~300 | 74% |
| policies.rs | 835 | ~300 | 64% |
| environments_list.rs | 891 | ~400 | 55% |
| **Total** | **12,365** | **~3,500** | **72%** |

## Tasks

### High Priority (Critical Duplicates)
- [ ] TASK-46: Clean up dashboard.rs duplicate components
- [ ] TASK-47: Extract components from systems_list.rs

### Medium Priority (Large Files)
- [ ] TASK-48: Standardize layout module to use mod.rs pattern
- [ ] TASK-49: Move flake_timeline.rs to components/flake/
- [ ] TASK-54: Extract components from system_detail.rs

### Low Priority (Placeholder Modules)
- [ ] TASK-50: Extract build components from views/builds.rs
- [ ] TASK-51: Extract diff viewer components
- [ ] TASK-52: Extract policy components from views/policies.rs
- [ ] TASK-53: Extract flake components from views/flakes_list.rs
- [ ] TASK-55: Extract components from environments_list.rs
- [ ] TASK-56: Create AddFlakeForm component in components/forms/

## Component Directory Structure

### After Reorganization
```
components/
├── charts/
│   ├── mod.rs
│   └── donut.rs              ✓ Already complete
├── dashboard/
│   ├── mod.rs
│   ├── build_queue.rs        ✓ Already exists
│   ├── build_summary.rs      ✓ Already exists
│   ├── cve_summary.rs        ✓ Already exists
│   ├── deployment_status.rs  ✓ Already exists
│   ├── fleet_health.rs       ✓ Already exists
│   └── recent_deployments.rs ✓ Already exists
├── diff/
│   ├── mod.rs
│   ├── diff_viewer.rs        ← From system_detail.rs
│   └── friendly_diff.rs      ← From flakes_list.rs
├── filters/
│   ├── mod.rs
│   ├── dropdown.rs           ✓ Already exists
│   ├── view_toggle.rs        ✓ Already exists
│   ├── environment_dropdown.rs ← From systems_list.rs
│   ├── health_dropdown.rs    ← From systems_list.rs
│   └── deployment_dropdown.rs ← From systems_list.rs
├── forms/
│   ├── mod.rs
│   ├── add_system.rs         ← From systems_list.rs
│   └── add_flake.rs          ← From flakes_list.rs
├── flake/
│   ├── mod.rs
│   ├── flake_timeline.rs     ← Move from components/
│   ├── flake_card.rs         ← From flakes_list.rs
│   └── flake_history.rs      ← From flakes_list.rs
├── layout/
│   ├── mod.rs                ← Rename from layout.rs
│   ├── app_shell.rs
│   ├── card.rs
│   ├── sidebar.rs
│   └── topbar.rs
├── modals/
│   ├── mod.rs
│   ├── confirm_dialog.rs     ✓ Already exists
│   ├── key_pair.rs           ← From systems_list.rs
│   └── remove_system.rs      ← From systems_list.rs
├── policy/
│   ├── mod.rs
│   ├── policy_card.rs        ← From policies.rs
│   └── policy_editor.rs      ← From policies.rs
├── system/
│   ├── mod.rs
│   ├── system_card.rs        ✓ Already exists
│   └── (others from system_detail.rs)
├── tables/
│   ├── mod.rs
│   ├── sortable_header.rs    ✓ Already exists
│   └── systems_table.rs      ← From systems_list.rs
├── builds/
│   ├── mod.rs
│   └── (components from builds.rs)
├── loading.rs                ✓ Generic utility
├── stat_card.rs              ✓ Generic utility
├── status_badge.rs           ✓ Generic utility
└── widget_grid.rs            ✓ Generic utility
```

## Success Criteria
- [ ] All view files under 500 lines (except system_detail which may be ~800)
- [ ] No duplicate component definitions
- [ ] All component directories properly populated (no empty TODOs)
- [ ] Consistent module pattern (mod.rs) across all component directories
- [ ] Build passes: `nix build .#checks.x86_64-linux.web-ui`
- [ ] All existing functionality preserved
