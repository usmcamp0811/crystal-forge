---
type: Concept
title: "Composite policy enforcement"
description: "Defines composite policy schema version 1 with its eight typed rule kinds, authoritative phases, rule result aggregation, evaluation expression rules, assessment scoping, and fail-closed final authorization before target updates."
tags:
  - crystal-forge
  - deployment-policy
  - composite
  - authorization
  - fail-closed
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/deployment-policy-checks.md at commit 3b23d36f"
    title: "Deployment Policy Checks"
---
# Composite enforcement

Composite policies expose eight typed rule kinds: `nixos_option`,
`packages_installed`, `packages_absent`, `custom_eval`, `eval_passed`,
`pin_required`, `cve_block`, and `time_window`. These are the exact eight kinds
exposed by the policy editor. `approval_required` remains hidden because the
existing approval records are not bound to the exact deployment target and
policy version with an authoritative delivery-time authorization and immutable
audit trail. `rollout_percent` remains hidden because canary state has no
production path that advances rollout phases. Exposing either would present a
control that can be saved without safely governing deployment. A rule UUID and
its array order are stable policy data; imports and exports must preserve both.

> **Status:** `rollout_percent` is described here as hidden because canary state has no production path that advances rollout phases. [Advanced Policy Types](advanced-policy-types.md#canary_rollout) describes phase advancement, and `packages/default/crates/cf-server/src/services/canary_rollout.rs` contains `advance_to_next_phase`. This migration did not reconcile the two statements.

Rules execute at an authoritative phase:

- Evaluation: NixOS options, installed/absent packages, custom expressions,
  successful configuration evaluation, and immutable source pinning.
- Scan: CVE thresholds use the newest scan attempt for the exact derivation.
- Deployment: time windows use the configured IANA timezone at authorization time.

Every rule result records `rule_id`, kind, phase, `pass`/`fail`/`error`/
`not_checked`, blocking state, detail, and structured evidence. Aggregation is
deterministic `all` semantics: `error` takes precedence over `fail`, then
`not_checked`; the final result is `pass` only when every rule passes. A due
deployment phase that is failed, errored, or not checked blocks deployment.

Evaluation expressions use stable policy-version/rule keys. NixOS option lookup
is performed against the target configuration's actual module graph; packaged
option metadata is authoring guidance only. String and lines values are emitted
as escaped semantic Nix literals, including quotes, backslashes, literal
`${...}`, and newlines. Package presence and absence use the legacy package
identity contract: each `environment.systemPackages` entry is matched by `pname`
only; `name` is not a fallback. `custom_eval` uses canonical `config.*`
expressions. Both evaluator forms bind `config` to the target's `cfg.config`, and
legacy `cfg.config.*` expressions are normalized to `config.*`. Evaluation is
contained with `builtins.tryEval`; exceptions and non-boolean values become
`error` rather than crashing the evaluator. `pin_required` compares the expected
full immutable revision extracted from the exact requested flake reference with
Nix's resolved `flake.sourceInfo.rev`; it does not compare display labels or an
unresolved commit string.

Composite assessment rows are normalized and scoped by system, exact derivation
and store path, policy lineage and immutable version, and effective-set digest.
Phase merges are transactional and reject mismatched rule, phase, version,
lineage, or target context. CVE evidence includes the exact scan ID, status,
severity count, threshold, and completion time. Time-window evidence includes
the evaluation timestamp and configured timezone/window. Existing derivation
`policy_results` remains the compatibility evidence envelope; readers prefer an
exact policy-version key and retain lineage-key fallback for legacy rows.

Final composite authorization runs before automatic and manual deployment
target updates, commit and generation rollbacks, and agent target delivery.
Heartbeat delivery re-resolves the exact system target and effective policy set,
so a stale or newly disallowed queued target is withheld. Resolution conflicts,
missing exact-target evidence, stale policy context, and authorization errors all
fail closed.

Legacy standalone policy types and their representations retain their existing
behavior. Composite execution does not replace or weaken the unconditional
Crystal Forge agent check.


## Related concepts

- [Deployment Policy Checks](deployment-policy-checks.md)
- [Deployment Policies](deployment-policies.md)
- [Advanced Policy Types](advanced-policy-types.md)
