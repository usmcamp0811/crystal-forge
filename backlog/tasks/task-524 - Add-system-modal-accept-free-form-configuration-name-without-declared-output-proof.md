---
id: TASK-524
title: Add system modal accepts a free-form configuration name without declared-output proof
status: Backlog
assignee: []
created_date: '2026-11-14 05:30'
labels:
  - systems
  - web-ui
  - onboarding
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner report during TASK-521: an operator cannot register a system whose flake has not evaluated yet. Operators register a system in Crystal Forge first and then add it to the flake, or push the flake commit that declares it, so the configuration name often does not exist yet.

The Add system modal blocks Save until the typed configuration name appears in the flake's declared outputs (`components/forms/add_system_form.rs`: `EnrollmentConfigurationFields` reports `valid` only when `names.contains(&configuration)`, and `validate_new_system` rejects an empty name). A flake without a synchronized, evaluated revision has no declared outputs, so Save never enables.

A wrong name has no harmful side effect. The server matches the name against built derivations (`d.derivation_name = COALESCE(NULLIF(BTRIM(system_configuration_name),''), hostname)`), so a name that matches nothing is not built. The flake reconciliation already reports such a system as managed-but-undeclared, and tests already use undeclared names.

Make declared-output discovery a helper for choosing a name, not a gate for saving.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The configuration field accepts any non-empty free-form name. Declared outputs from the selected flake are offered as suggestions only when discovery succeeds.
- [ ] #2 A failed, unavailable, or not-yet-evaluated discovery shows a non-blocking notice. It does not disable Save and does not clear or reset values already entered.
- [ ] #3 When discovery succeeds and the typed name is not declared, the modal shows a non-blocking warning that the system will not be built until the flake declares that name. Save stays enabled.
- [ ] #4 The footer state no longer reports "Declared configuration required" as a blocking reason.
- [ ] #5 Unit tests cover `validate_new_system` and the footer state for: declared name, undeclared name, and discovery failure. No test expects Save to be blocked by discovery.
- [ ] #6 The owning integration scenario registers a system whose configuration name is not declared and proves persistence and the resulting managed-undeclared state.
- [ ] #7 The `docs/knowledge` description of system registration states that the configuration name is not verified at registration.
<!-- AC:END -->

## Non-goals

- Changing how builds select derivations or how deployment targets resolve.
- Changing management-mode obligations on the server beyond what the owner decides below.

## Open decisions for the owner

- **Flake:** keep a registered flake required, or allow no flake for Managed systems? The server accepts no flake for Managed systems today and requires one for External and Reference.
- **Empty name:** for Managed systems, allow an empty name that falls back to the hostname (the server's existing rule), or require a typed name?
- **External and Reference modes:** keep the server rule that requires a flake and an explicit name (the name is still free-form)?

## Impact and risk

Public behavior of registration validation changes in the browser only. A typo now registers a system that is never built and shows as undeclared, instead of being blocked at Save.
