---
id: TASK-142
title: >-
  Add TOML-based Configuration for Systems, Builders, and Flakes with DB
  Override
status: To Do
assignee: []
created_date: '2026-03-01 14:16'
updated_date: '2026-03-01 14:30'
labels:
  - backend
  - infrastructure
  - config
  - devops
  - systems
  - builders
  - flakes
milestone: m-15
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge currently requires systems (agents), builders, and flakes to be registered via UI/database only. This creates friction for:
- **Infrastructure-as-Code** - Can't declare fleet in version-controlled TOML
- **Automated provisioning** - New servers/builders/flakes require manual UI registration
- **Development environments** - Setting up test environments requires clicking through UI
- **GitOps workflows** - Configuration drift between git and database

We established the pattern with cache configuration (TASK-141) where TOML provides defaults and DB overrides. Apply the same pattern to systems, builders, and flakes.

## Goal

Enable declaring systems, builders, and flakes in TOML, following **DB > TOML > defaults** precedence (same as TASK-141).

**Enable:**
1. **TOML-declared systems/builders/flakes** - Auto-registered on startup
2. **DB override** - UI changes override TOML declarations
3. **Sync-on-startup** - Server syncs TOML → DB (idempotent)
4. **Source tracking** - Items marked "from TOML" vs "DB-managed"

**Preserve:** UI-based management, existing DB-only deployments, no breaking changes

## Configuration Hierarchy

**Precedence:** Database (UI edits) > TOML file > Code defaults

**Rules:**
- **DB overrides TOML** when admin has modified via UI
- **TOML auto-syncs on startup** (creates/updates if not DB-managed)
- **Track source** - `managed_by` field ('toml' | 'ui')
- **UI edits change source** - Sets `managed_by = 'ui'`, stops TOML sync
- **TOML is declarative** - Re-adding removed item recreates it (unless DB-managed)

## TOML Schema

### Systems
```toml
[[systems]]
name = "prod-web-01"
hostname = "prod-web-01.example.com"
environment = "production"
deployment_strategy = "systemd"
public_key = "ssh-ed25519 AAAA..."
```

### Builders
```toml
[[builders]]
name = "builder-prod-01"
public_key = "ssh-ed25519 CCCC..."
max_cpu_cores = 8
max_memory_mb = 16384
max_concurrent_jobs = 2
environments = ["production"]
```

### Flakes
```toml
[[flakes.registry]]
name = "nixpkgs"
repo_url = "https://github.com/NixOS/nixpkgs"
branch = "nixos-unstable"
```

## Scope

### Phase 1: Database Migration
Add `managed_by` ('toml'|'ui') and `toml_name` to systems, builders, flakes tables.

### Phase 2: TOML Schema
Define config structs for systems, builders, flakes registry entries.

### Phase 3: Sync Logic
Idempotent sync on startup: create if missing, update if `managed_by='toml'`, skip if `managed_by='ui'`.

### Phase 4: UI Integration
- Show source badge (TOML/UI)
- Prompt on first edit: "Convert to UI-managed?"
- Filter by source

### Phase 5: Admin Controls
- Sync status dashboard
- Manual sync trigger
- Force re-sync with confirmation
- Bulk convert to UI-managed

## Example Scenarios

**Pure TOML:** System created on startup, updates from TOML each start
**UI Override:** Admin edits → `managed_by='ui'` → TOML sync skipped
**Mixed Mode:** TOML manages dev, UI manages prod
**Remove from TOML:** Item NOT deleted (safety, admin must delete via UI)

## Non-Goals

- ❌ Auto-deletion on TOML removal
- ❌ Two-way sync (TOML ← DB)
- ❌ Hot-reload TOML
- ❌ TOML-based secrets (use UI)

## Security

- TOML protected (contains keys)
- Sync on startup only
- Audit log records sync
- UI-managed items cannot be hijacked

## Risk Level

Medium (backward compatible, follows TASK-141 pattern)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Database migration adds managed_by and toml_name to systems, builders, flakes
- [ ] #2 TOML schema defined for systems, builders, and flakes arrays
- [ ] #3 Server startup syncs TOML to DB (create/update if managed_by=toml)
- [ ] #4 UI-managed items (managed_by=ui) skip TOML sync
- [ ] #5 UI shows source badge for systems, builders, flakes
- [ ] #6 First UI edit prompts to convert to UI-managed
- [ ] #7 UI edit sets managed_by=ui and prevents future sync
- [ ] #8 List views show source indicator
- [ ] #9 Admin sync status dashboard implemented
- [ ] #10 Manual sync trigger without restart
- [ ] #11 Force re-sync with confirmation
- [ ] #12 TOML items with UI-managed DB entry not overwritten
- [ ] #13 Removing from TOML does not delete from DB
- [ ] #14 Unit tests for sync logic (create, update, skip)
- [ ] #15 Integration tests for TOML sync on startup
- [ ] #16 Integration test for UI edit preventing sync
- [ ] #17 Documentation explains TOML vs UI precedence
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 TOML systems/builders/flakes sync on startup
- [ ] #2 UI edits persist and prevent TOML sync
- [ ] #3 Manual sync works without restart
- [ ] #4 Audit log captures sync ops
- [ ] #5 Docs include example TOML configs
<!-- DOD:END -->
