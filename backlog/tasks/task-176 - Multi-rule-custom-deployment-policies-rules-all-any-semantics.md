---
id: TASK-176
title: 'Multi-rule custom deployment policies (rules[] + all/any semantics)'
status: Backlog
assignee: []
created_date: '2026-03-09 00:50'
labels:
  - policies
  - backend
  - frontend
  - evaluation
dependencies:
  - TASK-123
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Current `custom_check` policies support a single expression only. Users need a way to author one policy composed of multiple checks (e.g. firewall + service checks) with clear pass/fail semantics.

Goal
Add first-class multi-rule policy support to `custom_check` using `config.rules[]` with policy-level combination mode (`all` / `any`) while maintaining backward compatibility for existing single-expression policies.

Non-goals
- No visual rule debugger in this task.
- No migration of historical results format beyond what is needed for compatibility.
- No changes to non-custom policy types (`require_packages`, `require_cf_agent`) except shared validation plumbing.

Proposed schema (v1)
`custom_check` config accepts:
- `mode`: "all" | "any" (default: "all")
- `rules`: array of rule objects

Rule object:
- `id`: string (optional; generated if omitted)
- `expression`: string (required)
- `description`: string (required)
- `strict`: boolean (optional; default true)
- `field_name`: string (optional; generated if omitted)

Backward compatibility:
- Existing single-rule shape (`expression`, `description`, `strict`) remains valid.
- Server converts single-rule shape to internal rules[] representation for evaluation.

Impact areas
- Backend validation for `custom_check` config shape
- Evaluator rule execution and aggregation logic
- API handler/DTO validation errors
- Web UI advanced editor + basic builder for multiple rules
- Tests for rule parsing, aggregation, and compatibility

Risk level
Medium

Dependencies
- TASK-123
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `custom_check` accepts `config.mode` (`all`|`any`) and `config.rules[]` with validated rule objects.
- [ ] #2 Existing single-expression `custom_check` configs continue to work without migration breakage.
- [ ] #3 Evaluator executes every rule in `rules[]`, records per-rule outcomes, and computes final pass/fail based on `mode`.
- [ ] #4 `strict` behavior is honored per-rule; policy result fails only according to `mode` + strict failing rules.
- [ ] #5 Generated field names/ids are deterministic when omitted, and collisions are prevented.
- [ ] #6 API returns clear 400 validation errors for malformed `rules[]` payloads (missing expression, empty rules array, invalid mode, etc.).
- [ ] #7 UI supports adding/removing/reordering multiple rules in basic/advanced authoring paths.
- [ ] #8 UI can represent and edit both legacy single-rule and new multi-rule configs.
- [ ] #9 Unit tests cover config validation, rule aggregation (`all`/`any`), and legacy compatibility.
- [ ] #10 Integration test demonstrates a multi-rule policy affecting evaluation result as expected.
<!-- AC:END -->
