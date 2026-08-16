---
id: TASK-426
title: Fix STIG refinement NixOS assertion handling
status: In Progress
assignee:
  - '@Matt Camp'
created_date: '2026-08-16 15:51'
updated_date: '2026-08-16 15:51'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/compliance/xccdf/inference.rs
  - packages/web-ui/src/components/compliance/refine_policy.rs
  - packages/web-ui/src/components/compliance/stig_import.rs
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 421000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Preserve structured inferred NixOS option assertions from STIG preview through refinement and import serialization. Inferred option_path and typed literals must become PolicyAssertionDraft::NixosOption rather than CustomExpression. Normalize the public Nix expression scope to config after tracing the evaluator boundary. Make manually authored NixOS option assertions preserve Boolean Integer and String values. Ensure repeated Add NixOS Option actions append independent assertions and preserve assertion_mode all plus source order. Add focused Rust and browser regression coverage for the auditd example with security.auditd.enable = true and security.audit.enable = true.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Two inferred structured NixOS option assertions enter refinement as NixosOption assertions
- [ ] #2 Inferred Boolean Integer and StringLiteral values retain their typed PolicyAssertionDraft values
- [ ] #3 Inferred assertions never become CustomExpression when structured fields are valid
- [ ] #4 Generated NixOS expressions use config.option.path and stored option paths exclude config
- [ ] #5 Manual NixOS option editing preserves Boolean Integer and String values without quoting Boolean literals
- [ ] #6 Repeated Add NixOS Option actions append exactly one independent assertion each
- [ ] #7 Removing one assertion leaves other assertions intact and order is preserved
- [ ] #8 Import serialization sends all assertions once with assertion_mode all
- [ ] #9 Rust regression tests cover inference conversion editing and round-trip serialization
- [ ] #10 Focused browser coverage proves the auditd two-assertion case and manual third assertion
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: OpenAI
created: 2026-08-16 15:51
---
Implementation authorized by the user. This task will use a dedicated worktree from dev and remain narrowly scoped to STIG NixOS assertion inference editing and serialization.
---
<!-- COMMENTS:END -->
