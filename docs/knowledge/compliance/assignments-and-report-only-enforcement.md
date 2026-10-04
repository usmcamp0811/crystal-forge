---
type: Operator Guide
title: "Compliance Assignments, Overlays, and Report-Only Enforcement"
description: "Explains how a compliance bundle assignment resolves its effective policy set (exclusions, additions, overrides, system over environment), the enforce versus report_only modes, composite assessments, and how report-only failures affect blocking, waivers, and POA&Ms."
tags:
  - crystal-forge
  - compliance
  - report-only
  - enforcement
  - assignments
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:56:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/operator/compliance-interchange.md at commit 3b23d36f"
    title: "CF-XCCDF Compliance Interchange Operator Guide"
---
# Compliance Assignments, Overlays, and Report-Only Enforcement

> **Status:** implemented as server/API behavior per the source guide. `report_only` handling exists in `packages/default/crates/cf-server/tests/composite_policy.rs`, `tests/poam_workflows.rs`, and `src/server/mod.rs`; the assignment preview and effective-policy routes exist in `src/bin/server.rs`. The full text was not compared line by line with the code.

## Assignments and overlays

An assignment references one exact bundle version and can contain baseline
exclusions, added policy versions, supported value overrides, and `enforce` or
`report_only` mode.

The assignment effective set is resolved server-side:

```text
bundle baseline - exclusions + additions + value overrides
```

For system resolution, specificity is system over environment over bundle
baseline. Same-version contributions are deduplicated. Different versions of
one policy lineage at the same specificity produce a typed conflict; the server
does not silently choose the newest version.

At runtime, native policy versions in the `nix-evaluation` or `multi-phase`
execution phase run during Nix evaluation when their parsed policy has a Nix
check. Both `enforce` and `report_only` assignments produce outcomes and retain
failure evidence. An `enforce` failure blocks only when the policy or rule is
intrinsically strict. A `report_only` failure never blocks and does not change
the intrinsic `strict` value stored with the result. The result's `blocking`
field records the exact effective decision after assignment mode, top-level
strictness, and constituent-rule strictness are applied. Unbound, opaque,
manual, external, and non-Nix-phase policy versions do not enter
`nix-eval-jobs`.
The unconditional `cfAgentEnabled` gate remains blocking independently of
assignment mode.

An invalid or unsupported `report_only` implementation is not executable. The
server records a structured warning with its system, configuration, policy
version, and policy type, and excludes the implementation from evaluator
evidence. Its deterministic invalid-implementation identity still participates
in shared-configuration conflict detection. An invalid `enforce`
implementation continues to fail closed.

Legacy custom checks retain their boolean result format in `enforce` mode. In
`report_only` mode, the evaluator contains thrown and non-boolean expressions
with `builtins.tryEval`. The persisted result has `passed: false`,
`blocking: false`, and an `evaluation_error` detail. The evaluator does not
convert an evaluation error to a pass. Before evaluation, the server reserves
all built-in, stable policy, composite-rule, and configured `enforce` result
keys for the complete assignment slice. It then allocates UUID-based
`report_only` custom-check keys with deterministic suffixes when necessary.
This allocation preserves existing `enforce` keys and prevents a report-only
result from colliding with another evaluator field.

Composite deployment authorization uses a canonical digest of enforced
composite policy versions and their effective configurations. The complete
effective-set digest remains the general policy-resolution identity. Composite
assessments are persisted for both assignment modes from the normal exact-target
evaluation, scan, and deployment lifecycle. Each report-only composite uses a
mode-bound policy-version/configuration assessment identity. That identity is
not a deployment authorization digest or a weaker finding observation. A
report-only failure remains a compliance failure: operators can waive it or
create or link a POA&M, but it does not block deployment. Creating a POA&M
does not change the assessment result. A report-only or non-composite assignment
change does not stale an enforced composite deployment assessment. Changing
mode preserves stable findings and historical POA&M/waiver evidence; current
remediation must match an exact assessment created under the current assignment
snapshot. A mode round-trip does not reactivate an assessment from an older
snapshot; scan updates do not refresh its creation time. Assessments written
before the enforced digest split remain valid for enforcement only when one
complete legacy digest group exactly matches every current enforced composite
policy version, effective configuration, and ordered rule result. Ambiguous,
incomplete, malformed, or mismatched legacy groups remain stale. Legacy
enforcement evidence cannot stand in for report-only evidence.

Use `POST /api/v1/compliance/assignments/preview` before saving when a preview is
needed. Effective policies are available from:

- `GET /api/v1/compliance/assignments/:id/effective-policies`;
- `GET /api/v1/systems/:id/effective-policies`; and
- the assignment-aware resolver used by assignment previews and exports.

## Related concepts

- [CF-XCCDF compliance interchange operator guide](cf-xccdf-interchange-operator-guide.md): versions, import, trust, and export.
- [Fleet CVE triage operator guide](../cves/fleet-cve-triage.md): risk acceptance and POA&M handling for CVE findings.
