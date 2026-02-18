---
id: TASK-47
title: 'Refactor: Extract components from systems_list.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:45'
labels:
  - refactoring
  - web-ui
  - systems
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The views/systems_list.rs file is 1400+ lines and contains multiple components that should be extracted to the components/ directory.

## Components to Extract

### To components/forms/
| Component | Description |
|-----------|-------------|
| AddSystemForm | Form for registering a new system |

### To components/modals/
| Component | Description |
|-----------|-------------|
| KeyPairModal | Modal displaying generated key pair |
| RemoveSystemDialog | Confirmation dialog for system removal |

### To components/tables/
| Component | Description |
|-----------|-------------|
| SystemsTable | Sortable table of systems |

### To components/filters/ (already partially exists)
| Component | Current Location | Target File |
|-----------|------------------|-------------|
| EnvironmentFilterDropdown | systems_list.rs | filters/environment_dropdown.rs |
| HealthFilterDropdown | systems_list.rs | filters/health_dropdown.rs |
| DeploymentFilterDropdown | systems_list.rs | filters/deployment_dropdown.rs |

### Helper functions to move
| Function | Target |
|----------|--------|
| validate_new_system | components/forms/add_system.rs |
| generate_key_pair | components/modals/key_pair.rs |
| remove_system_by_id | Keep in view (view-specific logic) |

## Acceptance Criteria
- [ ] views/systems_list.rs reduced to ~400-500 lines
- [ ] AddSystemForm moved to components/forms/add_system.rs
- [ ] KeyPairModal moved to components/modals/key_pair.rs
- [ ] RemoveSystemDialog moved to components/modals/remove_system_dialog.rs
- [ ] SystemsTable moved to components/tables/systems_table.rs
- [ ] Filter dropdowns consolidated in components/filters/
- [ ] All components properly re-exported through mod.rs files
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
