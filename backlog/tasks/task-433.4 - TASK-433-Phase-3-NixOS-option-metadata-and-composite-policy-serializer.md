---
id: TASK-433.4
title: 'TASK-433 Phase 3: NixOS option metadata and composite policy serializer'
status: Backlog
assignee: []
created_date: '2026-08-23 01:42'
labels:
  - design-parity
  - policy
  - server
  - phase-3
dependencies:
  - TASK-433.3
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/data-enforcement.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 436000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 3 of 8 (contextual only). Adds production NixOS option search/type/enum metadata with unknown/custom fallback, and the smallest repository-consistent versioned composite rule-set with stable rule IDs, typed kind/config, deterministic serialization/digest, `all` semantics, and per-rule outcomes, while keeping legacy single-type policies compatible without rewriting immutable history.

## Explicit scope
- NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback sourced from real metadata (with safe fallback when metadata is unavailable).
- Long semantic values round-trip exactly, including a long multiline banner value.
- Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history (no rewriting existing versions).

## Explicit non-scope
No enforcement execution wiring (that is Phase 4). No POA&M changes. Do not flatten non-Nix rule kinds into Nix representation.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix develop -c bash -c 'cd packages/default && cargo sqlx prepare --workspace'
nix build .#checks.x86_64-linux.server-regressions --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback from real metadata or safe fallback.
- [ ] #2 Long semantic values round-trip exact difficult strings including the DoD multiline banner.
- [ ] #3 Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history.
<!-- AC:END -->
