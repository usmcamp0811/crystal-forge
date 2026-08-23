---
id: TASK-433.3
title: >-
  TASK-433 Phase 2: Unified policy editor
  (Basics/Enforcement/Compliance/Evidence/Provenance)
status: In Progress
assignee: []
created_date: '2026-08-23 01:42'
updated_date: '2026-08-23 14:19'
labels:
  - design-parity
  - policy
  - web-ui
  - server
  - phase-2
dependencies:
  - TASK-433.2
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/components/PolicyEditor.jsx
  - docs/design/CrystalForge/data-mappings.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 435000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 2 of 8 (contextual only). Implements one common policy editor for every policy origin (custom, imported/STIG, cross-framework) from `PolicyEditor.jsx` in the design delta, replacing/unifying any existing per-origin editing paths without rewriting immutable version history or provenance.

## Explicit scope
- One editor shell with Basics, Enforcement, Compliance, Evidence tabs/sections and a read-only Provenance section for imported policies.
- Category changes preserve every existing rule and change guidance only; rules remain composable across categories.
- Zero mappings save as a valid "Unmapped" state; "mapped but no enforcement" and "No enforcement" are visually and semantically distinct states.
- Manual mappings support full CRUD; imported mappings/provenance remain read-only and survive reload.

## Explicit non-scope
No new enforcement kinds, Nix metadata typing, or composite execution (that is TASK-433 Phase 3/4). Do not make provenance editable. Do not rewrite unrelated TASK-422/compliance architecture.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All policy origins use one editor with Basics, Enforcement, Compliance, Evidence and read-only Provenance.
- [ ] #2 Category changes preserve every rule and change guidance only; cross-category rules remain composable.
- [ ] #3 Zero mappings save as valid Unmapped; mapped/no-enforcement and No enforcement are distinct states.
- [ ] #4 Manual mappings have permitted CRUD; imported mappings/provenance remain read-only and survive reload.
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-23 14:19
---
Started Phase 2 under explicit user override of the pending Phase-1 CI gate. Phase-1 head is present and working tree is clean; MR !318 pipeline remains pending/running at this time. Scope is limited to unified policy editor and mapping/provenance behavior; later phases will not be implemented.
---
<!-- COMMENTS:END -->
